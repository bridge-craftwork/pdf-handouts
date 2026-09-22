//! Measuring where a page's ink actually falls.
//!
//! To decide whether a page needs shifting or scaling to clear the header and
//! footer bands, we need its content bounding box. PDF does not record one, so
//! this module walks the page's content stream and accumulates the extent of
//! everything that paints: filled and stroked paths, text, images, and form
//! XObjects.
//!
//! The result is an estimate, deliberately biased toward being slightly
//! generous rather than slightly tight — an overestimate costs a little
//! unnecessary shrinking, while an underestimate would let content collide with
//! the title. Two approximations are worth knowing about:
//!
//! - **Text width** comes from the font's advance widths: its own `/Widths` or
//!   CID `/W` table, or Adobe's published metrics for the standard fonts, which
//!   carry no table. It matters on landscape pages, where the bands sit at the
//!   short edges and a line's width decides how far content must shrink. Only a
//!   font none of these describe falls back to half an em per character.
//!   Vertical extent uses the font size directly.
//! - **White fills are ignored.** Generated PDFs routinely paint a white
//!   background rectangle over the whole page; counting it would make every page
//!   look full-bleed. White ink on white paper is invisible, so skipping it
//!   matches what a reader sees.

use crate::pdf::fit::{apply, concat, Matrix, Rect, IDENTITY};
use crate::pdf::standard_widths;
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashMap;
use std::rc::Rc;

/// How far above the baseline a glyph may reach, as a fraction of font size.
const GLYPH_ASCENT: f32 = 0.9;
/// How far below the baseline a glyph may reach, as a fraction of font size.
const GLYPH_DESCENT: f32 = 0.25;
/// Assumed glyph advance for a font with no usable metrics, in 1/1000 em.
const AVERAGE_ADVANCE: f32 = 500.0;
/// Fill colours at or above this brightness count as white and are skipped.
const WHITE_THRESHOLD: f32 = 0.95;
/// How deep to follow nested form XObjects before falling back to their BBox.
const MAX_FORM_DEPTH: usize = 6;

/// Graphics state tracked while walking a content stream.
#[derive(Debug, Clone, Copy)]
struct GraphicsState {
    ctm: Matrix,
    fill_is_white: bool,
    clip: Option<Rect>,
}

/// Text state tracked between `BT` and `ET`.
#[derive(Debug, Clone)]
struct TextState {
    matrix: Matrix,
    line_matrix: Matrix,
    font: Rc<FontWidths>,
    font_size: f32,
    leading: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scale: f32,
    render_mode: i64,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            matrix: IDENTITY,
            line_matrix: IDENTITY,
            font: Rc::new(FontWidths::Unknown),
            font_size: 0.0,
            leading: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 1.0,
            render_mode: 0,
        }
    }
}

/// Accumulates the bounding box of everything a content stream paints.
struct Walker<'a> {
    doc: &'a Document,
    bounds: Option<Rect>,
}

/// Estimate the bounding box of a page's visible content, in page space.
///
/// Returns `None` for a page that paints nothing, or whose content stream
/// cannot be decoded — in both cases the caller should leave the page alone.
pub fn content_bounds(doc: &Document, page_id: ObjectId) -> Option<Rect> {
    let content = doc.get_and_decode_page_content(page_id).ok()?;
    let resources = page_resources(doc, page_id);

    let mut walker = Walker { doc, bounds: None };
    walker.walk(&content, &resources, IDENTITY, None, 0);
    walker.bounds
}

impl Walker<'_> {
    /// Add a rectangle to the running bounds, clipped if a clip path is active.
    fn add(&mut self, rect: Rect, clip: Option<Rect>) {
        if !rect.x0.is_finite()
            || !rect.y0.is_finite()
            || !rect.x1.is_finite()
            || !rect.y1.is_finite()
        {
            return;
        }
        let rect = match clip {
            Some(c) => match clip_to(rect, c) {
                Some(r) => r,
                None => return,
            },
            None => rect,
        };
        self.bounds = Some(match self.bounds {
            Some(existing) => union(existing, rect),
            None => rect,
        });
    }

    /// Walk one content stream, accumulating painted extents.
    fn walk(
        &mut self,
        content: &Content<Vec<lopdf::content::Operation>>,
        resources: &Dictionary,
        initial_ctm: Matrix,
        initial_clip: Option<Rect>,
        depth: usize,
    ) {
        let mut gs = GraphicsState {
            ctm: initial_ctm,
            fill_is_white: false,
            clip: initial_clip,
        };
        let mut stack: Vec<GraphicsState> = Vec::new();
        let mut text = TextState::default();
        let mut fonts: HashMap<Vec<u8>, Rc<FontWidths>> = HashMap::new();
        let mut path: Option<Rect> = None;
        let mut pending_clip = false;

        for op in &content.operations {
            let operands = &op.operands;
            match op.operator.as_str() {
                "q" => stack.push(gs),
                "Q" => {
                    if let Some(prev) = stack.pop() {
                        gs = prev;
                    }
                }
                "cm" => {
                    if let Some(m) = matrix_operand(operands) {
                        gs.ctm = concat(m, gs.ctm);
                    }
                }

                // Path construction. Points are transformed as they are added,
                // so a later `cm` cannot retroactively move them.
                "m" | "l" => {
                    if let (Some(x), Some(y)) = (num(operands, 0), num(operands, 1)) {
                        extend(&mut path, apply(gs.ctm, x, y));
                    }
                }
                "c" | "v" | "y" => {
                    // Curve control points bound the curve, which is enough here.
                    let mut i = 0;
                    while i + 1 < operands.len() {
                        if let (Some(x), Some(y)) = (num(operands, i), num(operands, i + 1)) {
                            extend(&mut path, apply(gs.ctm, x, y));
                        }
                        i += 2;
                    }
                }
                "re" => {
                    if let (Some(x), Some(y), Some(w), Some(h)) = (
                        num(operands, 0),
                        num(operands, 1),
                        num(operands, 2),
                        num(operands, 3),
                    ) {
                        for (px, py) in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
                            extend(&mut path, apply(gs.ctm, px, py));
                        }
                    }
                }

                // Clipping: `W` marks the current path as the next clip, which
                // takes effect when the path-painting operator arrives.
                "W" | "W*" => pending_clip = true,

                // Path painting.
                "n" | "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => {
                    if let Some(rect) = path {
                        if pending_clip {
                            gs.clip = Some(match gs.clip {
                                Some(c) => clip_to(rect, c).unwrap_or(rect),
                                None => rect,
                            });
                        }
                        let strokes =
                            matches!(op.operator.as_str(), "S" | "s" | "B" | "B*" | "b" | "b*");
                        let fills_only = matches!(op.operator.as_str(), "f" | "F" | "f*");
                        // A white fill is invisible on white paper; a stroke is
                        // not, so stroked paths always count.
                        if strokes || (fills_only && !gs.fill_is_white) {
                            self.add(rect, gs.clip);
                        }
                    }
                    pending_clip = false;
                    path = None;
                }

                // Fill colour, tracked only to recognise white backgrounds.
                "g" => gs.fill_is_white = num(operands, 0).is_some_and(|v| v >= WHITE_THRESHOLD),
                "rg" => {
                    gs.fill_is_white = (0..3)
                        .filter_map(|i| num(operands, i))
                        .all(|v| v >= WHITE_THRESHOLD)
                        && operands.len() >= 3
                }
                "k" => {
                    gs.fill_is_white = (0..4)
                        .filter_map(|i| num(operands, i))
                        .all(|v| v <= 1.0 - WHITE_THRESHOLD)
                        && operands.len() >= 4
                }
                "sc" | "scn" => {
                    let values: Vec<f32> = operands.iter().filter_map(as_num).collect();
                    gs.fill_is_white =
                        !values.is_empty() && values.iter().all(|v| *v >= WHITE_THRESHOLD);
                }
                "cs" => gs.fill_is_white = false,

                // Text.
                "BT" => {
                    text.matrix = IDENTITY;
                    text.line_matrix = IDENTITY;
                }
                "ET" => {}
                "Tf" => {
                    if let Some(Object::Name(name)) = operands.first() {
                        text.font = fonts
                            .entry(name.clone())
                            .or_insert_with(|| Rc::new(font_widths(self.doc, resources, name)))
                            .clone();
                    }
                    if let Some(size) = num(operands, 1) {
                        text.font_size = size;
                    }
                }
                "TL" => text.leading = num(operands, 0).unwrap_or(text.leading),
                "Tc" => text.char_spacing = num(operands, 0).unwrap_or(text.char_spacing),
                "Tw" => text.word_spacing = num(operands, 0).unwrap_or(text.word_spacing),
                "Tz" => {
                    text.horizontal_scale =
                        num(operands, 0).map_or(text.horizontal_scale, |v| v / 100.0)
                }
                "Tr" => {
                    text.render_mode = operands.first().and_then(|o| o.as_i64().ok()).unwrap_or(0)
                }
                "Tm" => {
                    if let Some(m) = matrix_operand(operands) {
                        text.matrix = m;
                        text.line_matrix = m;
                    }
                }
                "Td" => {
                    if let (Some(tx), Some(ty)) = (num(operands, 0), num(operands, 1)) {
                        text.line_matrix = concat([1.0, 0.0, 0.0, 1.0, tx, ty], text.line_matrix);
                        text.matrix = text.line_matrix;
                    }
                }
                "TD" => {
                    if let (Some(tx), Some(ty)) = (num(operands, 0), num(operands, 1)) {
                        text.leading = -ty;
                        text.line_matrix = concat([1.0, 0.0, 0.0, 1.0, tx, ty], text.line_matrix);
                        text.matrix = text.line_matrix;
                    }
                }
                "T*" => {
                    text.line_matrix =
                        concat([1.0, 0.0, 0.0, 1.0, 0.0, -text.leading], text.line_matrix);
                    text.matrix = text.line_matrix;
                }
                "Tj" | "'" | "\"" => {
                    if op.operator != "Tj" {
                        // Both move to the next line before showing text.
                        text.line_matrix =
                            concat([1.0, 0.0, 0.0, 1.0, 0.0, -text.leading], text.line_matrix);
                        text.matrix = text.line_matrix;
                    }
                    if let Some(bytes) = operands.last().and_then(|o| o.as_str().ok()) {
                        self.show_text(bytes, &mut text, &gs);
                    }
                }
                "TJ" => {
                    if let Some(Object::Array(items)) = operands.first() {
                        for item in items {
                            match item {
                                Object::String(bytes, _) => {
                                    self.show_text(bytes, &mut text, &gs);
                                }
                                other => {
                                    // A number nudges the next glyph horizontally.
                                    if let Some(adjust) = as_num(other) {
                                        let dx = -adjust / 1000.0
                                            * text.font_size
                                            * text.horizontal_scale;
                                        text.matrix =
                                            concat([1.0, 0.0, 0.0, 1.0, dx, 0.0], text.matrix);
                                    }
                                }
                            }
                        }
                    }
                }

                "Do" => {
                    if let Some(Object::Name(name)) = operands.first() {
                        self.draw_xobject(name, resources, &gs, depth);
                    }
                }

                _ => {}
            }
        }
    }

    /// Add the extent of a shown string and advance the text matrix past it.
    fn show_text(&mut self, bytes: &[u8], text: &mut TextState, gs: &GraphicsState) {
        if text.font_size == 0.0 {
            return;
        }

        // tx = ((w0 / 1000) * Tfs + Tc + Tw) * Th for each glyph, where word
        // spacing applies only to a single-byte space (PDF 32000 §9.4.4).
        let font = &text.font;
        let per_glyph = |code: u32, single_byte: bool| {
            let word = if single_byte && code == 32 {
                text.word_spacing
            } else {
                0.0
            };
            font.width(code) / 1000.0 * text.font_size + text.char_spacing + word
        };
        let raw: f32 = if font.is_two_byte() {
            bytes
                .chunks(2)
                .map(|pair| {
                    let code = pair.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b));
                    per_glyph(code, false)
                })
                .sum()
        } else {
            bytes.iter().map(|b| per_glyph(u32::from(*b), true)).sum()
        };
        let advance = raw * text.horizontal_scale;

        // Render modes 3 and 7 paint nothing — typically an OCR layer under a
        // scanned image. Advance past them but do not count them as ink.
        if text.render_mode != 3 && text.render_mode != 7 {
            let box_ts = Rect {
                x0: 0.0,
                y0: -GLYPH_DESCENT * text.font_size,
                x1: advance,
                y1: GLYPH_ASCENT * text.font_size,
            };
            let to_page = concat(text.matrix, gs.ctm);
            self.add(transform_rect(box_ts, to_page), gs.clip);
        }

        text.matrix = concat([1.0, 0.0, 0.0, 1.0, advance, 0.0], text.matrix);
    }

    /// Add the extent of an image or form XObject.
    fn draw_xobject(
        &mut self,
        name: &[u8],
        resources: &Dictionary,
        gs: &GraphicsState,
        depth: usize,
    ) {
        let Some(xobjects) = resolve_dict(self.doc, resources, b"XObject") else {
            return;
        };
        let Ok(entry) = xobjects.get(name) else {
            return;
        };
        let Some(Object::Stream(stream)) = resolve(self.doc, entry) else {
            return;
        };

        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .unwrap_or(b"");

        if subtype == b"Image" {
            // An image always fills the unit square under the current CTM.
            let unit = Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            };
            self.add(transform_rect(unit, gs.ctm), gs.clip);
            return;
        }

        if subtype != b"Form" {
            return;
        }

        let form_matrix = stream
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|o| resolve(self.doc, o))
            .and_then(|o| match o {
                Object::Array(arr) => matrix_operand(&arr),
                _ => None,
            })
            .unwrap_or(IDENTITY);

        let inner_ctm = concat(form_matrix, gs.ctm);

        // The form's BBox clips its content, so it also bounds it.
        let bbox = stream
            .dict
            .get(b"BBox")
            .ok()
            .and_then(|o| resolve(self.doc, o))
            .and_then(|o| match o {
                Object::Array(arr) => rect_operand(&arr),
                _ => None,
            });

        let clip = match bbox {
            Some(b) => {
                let transformed = transform_rect(b, inner_ctm);
                Some(match gs.clip {
                    Some(c) => match clip_to(transformed, c) {
                        Some(r) => r,
                        None => return,
                    },
                    None => transformed,
                })
            }
            None => gs.clip,
        };

        // Prefer walking the form's own content — its BBox is often the whole
        // page even when it paints a small area. Fall back to the BBox if the
        // stream cannot be read or nesting gets too deep.
        if depth < MAX_FORM_DEPTH {
            if let Ok(data) = stream.decompressed_content() {
                if let Ok(inner) = Content::decode(&data) {
                    let inner_resources = stream
                        .dict
                        .get(b"Resources")
                        .ok()
                        .and_then(|o| resolve(self.doc, o))
                        .and_then(|o| match o {
                            Object::Dictionary(d) => Some(d),
                            _ => None,
                        })
                        .unwrap_or_else(|| resources.clone());

                    self.walk(&inner, &inner_resources, inner_ctm, clip, depth + 1);
                    return;
                }
            }
        }

        if let Some(rect) = clip {
            self.add(rect, None);
        }
    }
}

/// What is known about a font's glyph advances, in 1/1000 em.
#[derive(Debug, Clone)]
enum FontWidths {
    /// One byte per glyph, with the font's own `/Widths` from `first_char`.
    Simple {
        first_char: u32,
        widths: Vec<f32>,
        missing: Option<f32>,
    },
    /// A standard font with no table of its own, in WinAnsi order from 32.
    Standard(&'static [u16; 224]),
    /// Every glyph the same width (Courier).
    Fixed(f32),
    /// Two bytes per glyph through an Identity CMap, widths by CID.
    Composite {
        default: f32,
        widths: HashMap<u32, f32>,
    },
    /// Nothing usable: each byte is taken as one average glyph.
    Unknown,
}

impl FontWidths {
    /// Whether strings in this font use two-byte codes.
    fn is_two_byte(&self) -> bool {
        matches!(self, FontWidths::Composite { .. })
    }

    /// Advance width of one character code.
    fn width(&self, code: u32) -> f32 {
        let known = match self {
            FontWidths::Simple {
                first_char,
                widths,
                missing,
            } => code
                .checked_sub(*first_char)
                .and_then(|i| widths.get(i as usize).copied())
                .or(*missing),
            FontWidths::Standard(table) => code
                .checked_sub(32)
                .and_then(|i| table.get(i as usize))
                .filter(|w| **w != 0)
                .map(|w| f32::from(*w)),
            FontWidths::Fixed(w) => Some(*w),
            FontWidths::Composite { default, widths } => {
                Some(widths.get(&code).copied().unwrap_or(*default))
            }
            FontWidths::Unknown => None,
        };
        known.unwrap_or(AVERAGE_ADVANCE)
    }
}

/// Learn a font's glyph widths from the resource named `name`.
fn font_widths(doc: &Document, resources: &Dictionary, name: &[u8]) -> FontWidths {
    let font = resolve_dict(doc, resources, b"Font")
        .and_then(|fonts| resolve(doc, fonts.get(name).ok()?))
        .and_then(|o| match o {
            Object::Dictionary(d) => Some(d),
            _ => None,
        });
    let Some(font) = font else {
        return FontWidths::Unknown;
    };

    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .unwrap_or(b"");

    match subtype {
        b"Type0" => composite_widths(doc, &font),
        // Type 3 widths are in glyph space, not text space; not worth the
        // FontMatrix arithmetic for how rarely they turn up.
        b"Type3" => FontWidths::Unknown,
        _ => simple_widths(doc, &font),
    }
}

/// Widths of a simple (single-byte) font.
fn simple_widths(doc: &Document, font: &Dictionary) -> FontWidths {
    let widths = font
        .get(b"Widths")
        .ok()
        .and_then(|o| resolve(doc, o))
        .and_then(|o| match o {
            Object::Array(arr) => Some(arr.iter().filter_map(as_num).collect::<Vec<f32>>()),
            _ => None,
        })
        .filter(|w| !w.is_empty());

    if let Some(widths) = widths {
        let first_char = font
            .get(b"FirstChar")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        let missing = resolve_dict(doc, font, b"FontDescriptor")
            .and_then(|d| d.get(b"MissingWidth").ok().and_then(as_num));
        return FontWidths::Simple {
            first_char,
            widths,
            missing,
        };
    }

    let base = font
        .get(b"BaseFont")
        .ok()
        .and_then(|o| o.as_name().ok())
        .unwrap_or(b"");
    standard_font(base)
}

/// The metrics of a standard font — or a common stand-in for one, such as
/// Arial for Helvetica — named without a width table of its own.
fn standard_font(base_font: &[u8]) -> FontWidths {
    let name = String::from_utf8_lossy(base_font).to_ascii_lowercase();
    // Drop a subset tag such as "ABCDEF+".
    let name = name.split_once('+').map_or(name.as_str(), |(_, rest)| rest);
    let bold = name.contains("bold");
    let slanted = name.contains("italic") || name.contains("oblique");

    if name.starts_with("courier") {
        return FontWidths::Fixed(600.0);
    }
    let table = if name.starts_with("times") {
        match (bold, slanted) {
            (false, false) => &standard_widths::TIMES_ROMAN,
            (true, false) => &standard_widths::TIMES_BOLD,
            (false, true) => &standard_widths::TIMES_ITALIC,
            (true, true) => &standard_widths::TIMES_BOLD_ITALIC,
        }
    } else if name.starts_with("helvetica") || name.starts_with("arial") {
        match (bold, slanted) {
            (false, false) => &standard_widths::HELVETICA,
            (true, false) => &standard_widths::HELVETICA_BOLD,
            (false, true) => &standard_widths::HELVETICA_OBLIQUE,
            (true, true) => &standard_widths::HELVETICA_BOLD_OBLIQUE,
        }
    } else {
        return FontWidths::Unknown;
    };
    FontWidths::Standard(table)
}

/// Widths of a composite font, when its codes map straight to CIDs.
fn composite_widths(doc: &Document, font: &Dictionary) -> FontWidths {
    let identity = matches!(
        font.get(b"Encoding").ok().and_then(|o| o.as_name().ok()),
        Some(b"Identity-H") | Some(b"Identity-V")
    );
    if !identity {
        return FontWidths::Unknown;
    }

    let descendant = font
        .get(b"DescendantFonts")
        .ok()
        .and_then(|o| resolve(doc, o))
        .and_then(|o| match o {
            Object::Array(arr) => arr.first().and_then(|d| resolve(doc, d)),
            _ => None,
        })
        .and_then(|o| match o {
            Object::Dictionary(d) => Some(d),
            _ => None,
        });
    let Some(descendant) = descendant else {
        return FontWidths::Unknown;
    };

    let default = descendant
        .get(b"DW")
        .ok()
        .and_then(as_num)
        .unwrap_or(1000.0);

    // /W mixes two forms: `c [w1 w2 ...]` and `c_first c_last w`.
    let mut widths = HashMap::new();
    if let Some(Object::Array(w)) = descendant.get(b"W").ok().and_then(|o| resolve(doc, o)) {
        let mut i = 0;
        while i < w.len() {
            let Some(first) = w[i].as_i64().ok().and_then(|v| u32::try_from(v).ok()) else {
                break;
            };
            match w.get(i + 1).and_then(|o| resolve(doc, o)) {
                Some(Object::Array(run)) => {
                    for (offset, width) in run.iter().filter_map(as_num).enumerate() {
                        widths.insert(first + offset as u32, width);
                    }
                    i += 2;
                }
                Some(last) => {
                    let (Some(last), Some(width)) = (
                        last.as_i64().ok().and_then(|v| u32::try_from(v).ok()),
                        w.get(i + 2).and_then(as_num),
                    ) else {
                        break;
                    };
                    // Guard against a corrupt range claiming millions of CIDs.
                    for cid in first..=last.min(first.saturating_add(0xFFFF)) {
                        widths.insert(cid, width);
                    }
                    i += 3;
                }
                None => break,
            }
        }
    }

    FontWidths::Composite { default, widths }
}

/// Grow a rectangle to include a point, creating it if needed.
fn extend(path: &mut Option<Rect>, (x, y): (f32, f32)) {
    let point = Rect {
        x0: x,
        y0: y,
        x1: x,
        y1: y,
    };
    *path = Some(match *path {
        Some(existing) => union(existing, point),
        None => point,
    });
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

fn clip_to(rect: Rect, clip: Rect) -> Option<Rect> {
    let r = Rect {
        x0: rect.x0.max(clip.x0),
        y0: rect.y0.max(clip.y0),
        x1: rect.x1.min(clip.x1),
        y1: rect.y1.min(clip.y1),
    };
    if r.x1 >= r.x0 && r.y1 >= r.y0 {
        Some(r)
    } else {
        None
    }
}

fn transform_rect(rect: Rect, m: Matrix) -> Rect {
    let corners = [
        apply(m, rect.x0, rect.y0),
        apply(m, rect.x1, rect.y0),
        apply(m, rect.x1, rect.y1),
        apply(m, rect.x0, rect.y1),
    ];
    let mut out = Rect {
        x0: f32::MAX,
        y0: f32::MAX,
        x1: f32::MIN,
        y1: f32::MIN,
    };
    for (x, y) in corners {
        out.x0 = out.x0.min(x);
        out.y0 = out.y0.min(y);
        out.x1 = out.x1.max(x);
        out.y1 = out.y1.max(y);
    }
    out
}

fn as_num(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn num(operands: &[Object], index: usize) -> Option<f32> {
    operands.get(index).and_then(as_num)
}

fn matrix_operand(operands: &[Object]) -> Option<Matrix> {
    if operands.len() < 6 {
        return None;
    }
    let v: Vec<f32> = operands.iter().take(6).filter_map(as_num).collect();
    if v.len() == 6 {
        Some([v[0], v[1], v[2], v[3], v[4], v[5]])
    } else {
        None
    }
}

fn rect_operand(operands: &[Object]) -> Option<Rect> {
    if operands.len() < 4 {
        return None;
    }
    let v: Vec<f32> = operands.iter().take(4).filter_map(as_num).collect();
    if v.len() == 4 {
        Some(Rect {
            x0: v[0].min(v[2]),
            y0: v[1].min(v[3]),
            x1: v[0].max(v[2]),
            y1: v[1].max(v[3]),
        })
    } else {
        None
    }
}

/// Resolve an object that may be an indirect reference.
fn resolve(doc: &Document, obj: &Object) -> Option<Object> {
    match obj {
        Object::Reference(id) => doc.get_object(*id).ok().cloned(),
        other => Some(other.clone()),
    }
}

/// Look up a dictionary-valued entry, following a reference if present.
fn resolve_dict(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<Dictionary> {
    match resolve(doc, dict.get(key).ok()?)? {
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Get a page's Resources, walking up the page tree for inherited ones.
fn page_resources(doc: &Document, page_id: ObjectId) -> Dictionary {
    let mut node = page_id;
    for _ in 0..32 {
        let Ok(dict) = doc.get_dictionary(node) else {
            break;
        };
        if let Some(resources) = resolve_dict(doc, dict, b"Resources") {
            return resources;
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(parent)) => node = *parent,
            _ => break,
        }
    }
    Dictionary::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::Operation;

    /// Build a one-page document whose content stream is `ops`.
    fn page_with(ops: Vec<Operation>) -> (Document, ObjectId) {
        page_with_fonts(ops, Dictionary::new())
    }

    /// Build a one-page document with `fonts` as its font resources.
    fn page_with_fonts(ops: Vec<Operation>, fonts: Dictionary) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.5");
        let content = Content { operations: ops };
        let stream_id = doc.add_object(lopdf::Stream::new(
            Dictionary::new(),
            content.encode().expect("content should encode"),
        ));

        let pages_id = doc.new_object_id();
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(stream_id));
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        page.set("Resources", Object::Dictionary(resources));
        let page_id = doc.add_object(Object::Dictionary(page));

        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Count", Object::Integer(1));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(612),
                Object::Integer(792),
            ]),
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        (doc, page_id)
    }

    fn op(operator: &str, operands: Vec<Object>) -> Operation {
        Operation::new(operator, operands)
    }

    fn real(v: f32) -> Object {
        Object::Real(v)
    }

    #[test]
    fn measures_a_filled_rectangle() {
        let (doc, page_id) = page_with(vec![
            op("0 g", vec![]),
            op("g", vec![real(0.0)]),
            op(
                "re",
                vec![real(100.0), real(200.0), real(300.0), real(150.0)],
            ),
            op("f", vec![]),
        ]);

        let bounds = content_bounds(&doc, page_id).expect("rectangle should be measured");
        assert!((bounds.x0 - 100.0).abs() < 0.01, "{:?}", bounds);
        assert!((bounds.y0 - 200.0).abs() < 0.01, "{:?}", bounds);
        assert!((bounds.x1 - 400.0).abs() < 0.01, "{:?}", bounds);
        assert!((bounds.y1 - 350.0).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn ignores_a_white_background_but_keeps_the_content_on_top() {
        // The pattern that would otherwise make every generated PDF look
        // full-bleed: a white page-sized rectangle behind the real content.
        let (doc, page_id) = page_with(vec![
            op("g", vec![real(1.0)]),
            op("re", vec![real(0.0), real(0.0), real(612.0), real(792.0)]),
            op("f", vec![]),
            op("g", vec![real(0.0)]),
            op(
                "re",
                vec![real(100.0), real(300.0), real(200.0), real(100.0)],
            ),
            op("f", vec![]),
        ]);

        let bounds = content_bounds(&doc, page_id).expect("content should be measured");
        assert!(
            (bounds.y0 - 300.0).abs() < 0.01 && (bounds.y1 - 400.0).abs() < 0.01,
            "white background was counted: {:?}",
            bounds
        );
    }

    #[test]
    fn a_white_fill_that_is_stroked_still_counts() {
        let (doc, page_id) = page_with(vec![
            op("g", vec![real(1.0)]),
            op("re", vec![real(50.0), real(60.0), real(100.0), real(100.0)]),
            op("B", vec![]),
        ]);

        let bounds = content_bounds(&doc, page_id).expect("stroked box should be measured");
        assert!((bounds.y0 - 60.0).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn honours_the_current_transform() {
        let (doc, page_id) = page_with(vec![
            op("g", vec![real(0.0)]),
            op(
                "cm",
                vec![
                    real(1.0),
                    real(0.0),
                    real(0.0),
                    real(1.0),
                    real(100.0),
                    real(50.0),
                ],
            ),
            op("re", vec![real(0.0), real(0.0), real(10.0), real(10.0)]),
            op("f", vec![]),
        ]);

        let bounds = content_bounds(&doc, page_id).expect("translated box should be measured");
        assert!((bounds.x0 - 100.0).abs() < 0.01, "{:?}", bounds);
        assert!((bounds.y0 - 50.0).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn q_and_q_restore_the_transform() {
        let (doc, page_id) = page_with(vec![
            op("g", vec![real(0.0)]),
            op("q", vec![]),
            op(
                "cm",
                vec![
                    real(1.0),
                    real(0.0),
                    real(0.0),
                    real(1.0),
                    real(500.0),
                    real(500.0),
                ],
            ),
            op("Q", vec![]),
            op("re", vec![real(0.0), real(0.0), real(10.0), real(10.0)]),
            op("f", vec![]),
        ]);

        let bounds = content_bounds(&doc, page_id).expect("box should be measured");
        assert!(bounds.x1 < 20.0, "transform leaked past Q: {:?}", bounds);
    }

    #[test]
    fn invisible_ocr_text_is_not_counted() {
        let (doc, page_id) = page_with(vec![
            op("BT", vec![]),
            op("Tr", vec![Object::Integer(3)]),
            op("Tf", vec![Object::Name(b"F1".to_vec()), real(12.0)]),
            op(
                "Tm",
                vec![
                    real(1.0),
                    real(0.0),
                    real(0.0),
                    real(1.0),
                    real(50.0),
                    real(700.0),
                ],
            ),
            op(
                "Tj",
                vec![Object::String(
                    b"hidden".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            op("ET", vec![]),
        ]);

        assert!(
            content_bounds(&doc, page_id).is_none(),
            "invisible text should paint nothing"
        );
    }

    #[test]
    fn text_extent_uses_the_font_size_vertically() {
        let (doc, page_id) = page_with(vec![
            op("BT", vec![]),
            op("Tf", vec![Object::Name(b"F1".to_vec()), real(20.0)]),
            op(
                "Tm",
                vec![
                    real(1.0),
                    real(0.0),
                    real(0.0),
                    real(1.0),
                    real(50.0),
                    real(700.0),
                ],
            ),
            op(
                "Tj",
                vec![Object::String(
                    b"Hello".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            op("ET", vec![]),
        ]);

        let bounds = content_bounds(&doc, page_id).expect("text should be measured");
        // Baseline 700, ascending 0.9*20 and descending 0.25*20.
        assert!((bounds.y1 - 718.0).abs() < 0.01, "{:?}", bounds);
        assert!((bounds.y0 - 695.0).abs() < 0.01, "{:?}", bounds);
        assert!(
            bounds.x1 > bounds.x0,
            "text should have width: {:?}",
            bounds
        );
    }

    fn name(n: &str) -> Object {
        Object::Name(n.as_bytes().to_vec())
    }

    /// Show `text` at (100, 500) in font resource `F1` at 20pt.
    fn show_in_f1(text: &[u8]) -> Vec<Operation> {
        vec![
            op("BT", vec![]),
            op("Tf", vec![name("F1"), real(20.0)]),
            op("Td", vec![real(100.0), real(500.0)]),
            op(
                "Tj",
                vec![Object::String(text.to_vec(), lopdf::StringFormat::Literal)],
            ),
            op("ET", vec![]),
        ]
    }

    fn font_dict(entries: Vec<(&str, Object)>) -> Dictionary {
        let mut d = Dictionary::new();
        d.set("Type", name("Font"));
        for (k, v) in entries {
            d.set(k, v);
        }
        d
    }

    fn with_f1(font: Dictionary) -> Dictionary {
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Dictionary(font));
        fonts
    }

    #[test]
    fn a_standard_font_is_measured_with_its_published_widths() {
        // The heading that used to overshoot by 41pt on the declarer's-plan
        // pages. In Times-Roman at 18pt it is 184pt wide, not the 225pt that
        // half an em per character gives.
        let font = font_dict(vec![
            ("Subtype", name("Type1")),
            ("BaseFont", name("Times-Roman")),
            ("Encoding", name("WinAnsiEncoding")),
        ]);
        let mut ops = show_in_f1(b"Goal: at most ____ losers");
        ops[1] = op("Tf", vec![name("F1"), real(18.0)]);
        let (doc, page_id) = page_with_fonts(ops, with_f1(font));

        let bounds = content_bounds(&doc, page_id).expect("text should be measured");
        assert!((bounds.width() - 183.996).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn a_font_width_table_takes_precedence() {
        // "AB" with A = 700 and B = 300 at 20pt is 20pt wide.
        let font = font_dict(vec![
            ("Subtype", name("TrueType")),
            ("BaseFont", name("Helvetica")),
            ("FirstChar", Object::Integer(65)),
            (
                "Widths",
                Object::Array(vec![Object::Integer(700), Object::Integer(300)]),
            ),
        ]);
        let (doc, page_id) = page_with_fonts(show_in_f1(b"AB"), with_f1(font));

        let bounds = content_bounds(&doc, page_id).expect("text should be measured");
        assert!((bounds.width() - 20.0).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn a_composite_font_reads_two_byte_codes_and_cid_widths() {
        // Two glyphs, CIDs 1 and 2: CID 1 from a `c [w]` run, CID 2 from the
        // default width. 896 + 1000 at 20pt is 37.92pt.
        let descendant = font_dict(vec![
            ("Subtype", name("CIDFontType2")),
            ("DW", Object::Integer(1000)),
            (
                "W",
                Object::Array(vec![
                    Object::Integer(1),
                    Object::Array(vec![Object::Integer(896)]),
                ]),
            ),
        ]);
        let font = font_dict(vec![
            ("Subtype", name("Type0")),
            ("Encoding", name("Identity-H")),
            (
                "DescendantFonts",
                Object::Array(vec![Object::Dictionary(descendant)]),
            ),
        ]);
        let (doc, page_id) = page_with_fonts(show_in_f1(&[0, 1, 0, 2]), with_f1(font));

        let bounds = content_bounds(&doc, page_id).expect("text should be measured");
        assert!((bounds.width() - 37.92).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn an_unknown_font_falls_back_to_half_an_em_per_byte() {
        let font = font_dict(vec![
            ("Subtype", name("Type1")),
            ("BaseFont", name("Futura-Medium")),
        ]);
        let (doc, page_id) = page_with_fonts(show_in_f1(b"abcd"), with_f1(font));

        let bounds = content_bounds(&doc, page_id).expect("text should be measured");
        assert!((bounds.width() - 40.0).abs() < 0.01, "{:?}", bounds);
    }

    #[test]
    fn stand_ins_for_standard_fonts_use_their_metrics() {
        assert!(matches!(
            standard_font(b"ABCDEF+Arial,Bold"),
            FontWidths::Standard(t) if std::ptr::eq(t, &standard_widths::HELVETICA_BOLD)
        ));
        assert!(matches!(
            standard_font(b"TimesNewRomanPS-ItalicMT"),
            FontWidths::Standard(t) if std::ptr::eq(t, &standard_widths::TIMES_ITALIC)
        ));
        assert!(matches!(standard_font(b"Courier-Bold"), FontWidths::Fixed(w) if w == 600.0));
        assert!(matches!(standard_font(b"Symbol"), FontWidths::Unknown));
    }

    #[test]
    fn an_empty_page_has_no_bounds() {
        let (doc, page_id) = page_with(vec![]);
        assert!(content_bounds(&doc, page_id).is_none());
    }
}
