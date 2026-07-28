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
//! - **Text width** is estimated at roughly half an em per character rather than
//!   read from font metrics. Vertical extent, which is what the fitting decision
//!   turns on for portrait pages, uses the font size directly and is accurate.
//! - **White fills are ignored.** Generated PDFs routinely paint a white
//!   background rectangle over the whole page; counting it would make every page
//!   look full-bleed. White ink on white paper is invisible, so skipping it
//!   matches what a reader sees.

use crate::pdf::fit::{apply, concat, Matrix, Rect, IDENTITY};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

/// How far above the baseline a glyph may reach, as a fraction of font size.
const GLYPH_ASCENT: f32 = 0.9;
/// How far below the baseline a glyph may reach, as a fraction of font size.
const GLYPH_DESCENT: f32 = 0.25;
/// Assumed average glyph advance, as a fraction of font size.
const AVERAGE_ADVANCE: f32 = 0.5;
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
#[derive(Debug, Clone, Copy)]
struct TextState {
    matrix: Matrix,
    line_matrix: Matrix,
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

        let spaces = bytes.iter().filter(|b| **b == b' ').count() as f32;
        let advance = (bytes.len() as f32 * AVERAGE_ADVANCE * text.font_size
            + bytes.len() as f32 * text.char_spacing
            + spaces * text.word_spacing)
            * text.horizontal_scale;

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
        page.set("Resources", Object::Dictionary(Dictionary::new()));
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

    #[test]
    fn an_empty_page_has_no_bounds() {
        let (doc, page_id) = page_with(vec![]);
        assert!(content_bounds(&doc, page_id).is_none());
    }
}
