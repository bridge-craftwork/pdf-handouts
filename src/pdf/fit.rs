//! Fitting page content around headers and footers.
//!
//! Headers and footers are drawn into bands at the top and bottom of the page.
//! Source documents know nothing about those bands, so their content can collide
//! with the title — a full-bleed handout ends up with the title printed over its
//! own heading.
//!
//! This module measures where a page's ink actually is and works out the
//! smallest correction that clears the bands:
//!
//! - Content already inside the safe area is left alone.
//! - Content that merely sits too high is **shifted** down; nothing is resized.
//! - Content too tall to shift is **scaled** down about the page centre until it
//!   fits, then centred in the safe area.
//!
//! Everything is expressed in an upright *layout frame*. For a portrait page
//! that frame is the page itself; for a landscape page it is the page turned a
//! quarter turn, so "top" and "bottom" mean the short edges. See [`PageFrame`].

use lopdf::{Document, Object, ObjectId};

/// A PDF transformation matrix `[a b c d e f]`.
///
/// Maps a point as `(x, y) -> (a*x + c*y + e, b*x + d*y + f)`, matching the
/// operands of the `cm` and `Tm` content stream operators.
pub type Matrix = [f32; 6];

/// The identity transform.
pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Combine two transforms: the result applies `first`, then `second`.
pub fn concat(first: Matrix, second: Matrix) -> Matrix {
    let [a1, b1, c1, d1, e1, f1] = first;
    let [a2, b2, c2, d2, e2, f2] = second;
    [
        a1 * a2 + b1 * c2,
        a1 * b2 + b1 * d2,
        c1 * a2 + d1 * c2,
        c1 * b2 + d1 * d2,
        e1 * a2 + f1 * c2 + e2,
        e1 * b2 + f1 * d2 + f2,
    ]
}

/// Apply a transform to a point.
pub fn apply(m: Matrix, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Invert a transform, or `None` if it is degenerate.
pub fn invert(m: Matrix) -> Option<Matrix> {
    let [a, b, c, d, e, f] = m;
    let det = a * d - b * c;
    if det.abs() < 1e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        d * inv_det,
        -b * inv_det,
        -c * inv_det,
        a * inv_det,
        (c * f - d * e) * inv_det,
        (b * e - a * f) * inv_det,
    ])
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Minimum x
    pub x0: f32,
    /// Minimum y
    pub y0: f32,
    /// Maximum x
    pub x1: f32,
    /// Maximum y
    pub y1: f32,
}

impl Rect {
    /// Width of the rectangle.
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    /// Height of the rectangle.
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    /// The rectangle enclosing this one after `m` is applied to its corners.
    fn transformed(self, m: Matrix) -> Rect {
        let corners = [
            apply(m, self.x0, self.y0),
            apply(m, self.x1, self.y0),
            apply(m, self.x1, self.y1),
            apply(m, self.x0, self.y1),
        ];
        let mut r = Rect {
            x0: f32::MAX,
            y0: f32::MAX,
            x1: f32::MIN,
            y1: f32::MIN,
        };
        for (x, y) in corners {
            r.x0 = r.x0.min(x);
            r.y0 = r.y0.min(y);
            r.x1 = r.x1.max(x);
            r.y1 = r.y1.max(y);
        }
        r
    }
}

/// The upright coordinate frame that headers and footers are composed in.
///
/// Header and footer text is always laid out as though the page were portrait —
/// title across the top, footer along the bottom. On a landscape page that frame
/// is rotated a quarter turn onto the page, which puts the text along the short
/// edges. Printed on portrait paper (where the driver turns landscape pages to
/// fit) the footer then lands along the bottom of the sheet, consistent with
/// every other page in the stack, while on screen the page still displays
/// landscape and reads normally.
#[derive(Debug, Clone, Copy)]
pub struct PageFrame {
    /// Page width in points, as given by the MediaBox.
    pub page_width: f32,
    /// Page height in points, as given by the MediaBox.
    pub page_height: f32,
    /// Whether the layout frame is rotated relative to the page.
    pub rotated: bool,
}

impl PageFrame {
    /// Build the layout frame for a page of the given size.
    pub fn new(page_width: f32, page_height: f32) -> Self {
        PageFrame {
            page_width,
            page_height,
            rotated: page_width > page_height,
        }
    }

    /// Width of the upright layout frame.
    pub fn frame_width(&self) -> f32 {
        if self.rotated {
            self.page_height
        } else {
            self.page_width
        }
    }

    /// Height of the upright layout frame.
    pub fn frame_height(&self) -> f32 {
        if self.rotated {
            self.page_width
        } else {
            self.page_height
        }
    }

    /// Transform mapping layout-frame coordinates onto the page.
    ///
    /// For a landscape page this is a quarter turn: frame `(0, 0)` — the bottom
    /// left of the footer — lands at the top left of the page, so that the text
    /// runs up the page's right-hand short edge and reads correctly once the
    /// sheet is turned.
    pub fn frame_to_page(&self) -> Matrix {
        if self.rotated {
            [0.0, 1.0, -1.0, 0.0, self.page_width, 0.0]
        } else {
            IDENTITY
        }
    }
}

/// How page content should be adjusted to clear the header and footer bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    /// Shift content when that is enough, scale it down when it is not.
    Auto,
    /// Only ever shift content; never resize it.
    ShiftOnly,
    /// Leave source content exactly as it is.
    Off,
}

/// What was done to a page's content, for reporting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitAction {
    /// Content already cleared the bands.
    Unchanged,
    /// Content was moved by the given distance in points.
    Shifted(f32),
    /// Content was scaled by the given factor (< 1.0).
    Scaled(f32),
}

/// The adjustment to apply to one page.
#[derive(Debug, Clone, Copy)]
pub struct Fit {
    /// Transform to prepend to the page's content stream, in page space.
    pub transform: Matrix,
    /// What the transform does, for logging.
    pub action: FitAction,
}

/// Work out how to fit `content` (page space) into the safe band of `frame`.
///
/// `safe_low` and `safe_high` bound the usable area along the layout frame's
/// vertical axis: everything below `safe_low` belongs to the footer and
/// everything above `safe_high` to the title.
///
/// Returns `None` when the content already fits or cannot be helped.
pub fn fit_content(
    frame: &PageFrame,
    content: Rect,
    safe_low: f32,
    safe_high: f32,
    mode: FitMode,
) -> Option<Fit> {
    if mode == FitMode::Off {
        return None;
    }

    let to_page = frame.frame_to_page();
    let to_frame = invert(to_page)?;

    // Measure the content in the upright frame, where the bands are horizontal.
    let content = content.transformed(to_frame);

    let available = safe_high - safe_low;
    if available <= 0.0 {
        return None;
    }

    // Already clear of both bands.
    if content.y0 >= safe_low && content.y1 <= safe_high {
        return None;
    }

    let (scale, action) = if content.height() <= available || mode == FitMode::ShiftOnly {
        (1.0, FitAction::Unchanged)
    } else {
        let s = available / content.height();
        (s, FitAction::Scaled(s))
    };

    // Scale about the frame's horizontal centre so the composition stays put.
    let centre_x = frame.frame_width() / 2.0;
    let tx = (1.0 - scale) * centre_x;

    let scaled_low = scale * content.y0;
    let scaled_high = scale * content.y1;

    // Move as little as possible: down if the content overruns the title band,
    // up if it overruns the footer, and centre it when it fills the band.
    let ty = if scaled_high - scaled_low >= available {
        safe_low + (available - (scaled_high - scaled_low)) / 2.0 - scaled_low
    } else if scaled_high > safe_high {
        safe_high - scaled_high
    } else if scaled_low < safe_low {
        safe_low - scaled_low
    } else {
        0.0
    };

    let action = match action {
        FitAction::Scaled(s) => FitAction::Scaled(s),
        _ if ty.abs() < 0.01 => return None,
        _ => FitAction::Shifted(ty),
    };

    // Build the frame-space adjustment, then express it in page space:
    // page -> frame, adjust there, then back to the page.
    let adjust: Matrix = [scale, 0.0, 0.0, scale, tx, ty];
    let transform = concat(concat(to_frame, adjust), to_page);

    Some(Fit { transform, action })
}

/// Read a page's MediaBox, falling back to US Letter when it is missing.
pub fn page_media_box(doc: &Document, page_id: ObjectId) -> Rect {
    fn as_f32(obj: &Object) -> Option<f32> {
        match obj {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        }
    }

    // MediaBox is inheritable, so walk up the page tree if the page lacks one.
    let mut node = page_id;
    for _ in 0..32 {
        let Ok(dict) = doc.get_dictionary(node) else {
            break;
        };
        if let Ok(value) = dict.get(b"MediaBox") {
            let resolved = match value {
                Object::Reference(id) => doc.get_object(*id).ok(),
                other => Some(other),
            };
            if let Some(Object::Array(arr)) = resolved {
                if arr.len() == 4 {
                    let v: Vec<f32> = arr.iter().filter_map(as_f32).collect();
                    if v.len() == 4 {
                        return Rect {
                            x0: v[0].min(v[2]),
                            y0: v[1].min(v[3]),
                            x1: v[0].max(v[2]),
                            y1: v[1].max(v[3]),
                        };
                    }
                }
            }
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(parent)) => node = *parent,
            _ => break,
        }
    }

    Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 612.0,
        y1: 792.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    #[test]
    fn identity_round_trips() {
        let m = concat(IDENTITY, IDENTITY);
        assert_eq!(m, IDENTITY);
        assert_eq!(apply(IDENTITY, 3.0, 4.0), (3.0, 4.0));
    }

    #[test]
    fn landscape_frame_maps_footer_to_right_edge() {
        let frame = PageFrame::new(792.0, 612.0);
        assert!(frame.rotated);
        assert_eq!(frame.frame_width(), 612.0);
        assert_eq!(frame.frame_height(), 792.0);

        // The footer's bottom-left in the layout frame sits near the page's
        // right-hand short edge, which becomes the bottom of a printed sheet.
        let m = frame.frame_to_page();
        let (x, y) = apply(m, 50.0, 30.0);
        assert!((x - 762.0).abs() < 0.01, "x was {}", x);
        assert!((y - 50.0).abs() < 0.01, "y was {}", y);

        // The title band maps to the opposite short edge.
        let (tx, _) = apply(m, 306.0, 742.0);
        assert!((tx - 50.0).abs() < 0.01, "title x was {}", tx);
    }

    #[test]
    fn portrait_frame_is_the_page() {
        let frame = PageFrame::new(612.0, 792.0);
        assert!(!frame.rotated);
        assert_eq!(frame.frame_to_page(), IDENTITY);
    }

    #[test]
    fn content_already_clear_is_left_alone() {
        let frame = PageFrame::new(612.0, 792.0);
        let fit = fit_content(
            &frame,
            rect(50.0, 100.0, 550.0, 700.0),
            60.0,
            720.0,
            FitMode::Auto,
        );
        assert!(fit.is_none());
    }

    #[test]
    fn short_content_is_shifted_not_scaled() {
        let frame = PageFrame::new(612.0, 792.0);
        // 300pt of content sitting too high; the safe band is 660pt tall.
        let fit = fit_content(
            &frame,
            rect(50.0, 450.0, 550.0, 750.0),
            60.0,
            720.0,
            FitMode::Auto,
        )
        .expect("content overruns the title band");

        match fit.action {
            FitAction::Shifted(dy) => assert!((dy - -30.0).abs() < 0.01, "dy was {}", dy),
            other => panic!("expected a shift, got {:?}", other),
        }
        // A pure shift must not resize anything.
        assert_eq!(fit.transform[0], 1.0);
        assert_eq!(fit.transform[3], 1.0);
    }

    #[test]
    fn tall_content_is_scaled_to_the_band() {
        let frame = PageFrame::new(612.0, 792.0);
        // Full-bleed content: 780pt tall into a 660pt band.
        let fit = fit_content(
            &frame,
            rect(10.0, 6.0, 602.0, 786.0),
            60.0,
            720.0,
            FitMode::Auto,
        )
        .expect("content cannot fit without scaling");

        let scale = match fit.action {
            FitAction::Scaled(s) => s,
            other => panic!("expected a scale, got {:?}", other),
        };
        assert!((scale - 660.0 / 780.0).abs() < 0.001, "scale was {}", scale);

        // The scaled content must land inside the band.
        let (_, low) = apply(fit.transform, 10.0, 6.0);
        let (_, high) = apply(fit.transform, 10.0, 786.0);
        assert!(low >= 60.0 - 0.01, "bottom landed at {}", low);
        assert!(high <= 720.0 + 0.01, "top landed at {}", high);
    }

    #[test]
    fn shift_only_mode_never_scales() {
        let frame = PageFrame::new(612.0, 792.0);
        let fit = fit_content(
            &frame,
            rect(10.0, 6.0, 602.0, 786.0),
            60.0,
            720.0,
            FitMode::ShiftOnly,
        );
        if let Some(fit) = fit {
            assert_eq!(fit.transform[0], 1.0, "shift-only must not scale");
        }
    }

    #[test]
    fn landscape_content_is_adjusted_along_the_page_x_axis() {
        // On a landscape page the bands are vertical strips at the short edges,
        // so the correction has to move content sideways, not up and down.
        let frame = PageFrame::new(792.0, 612.0);
        // Content pushed hard against the page's left edge (the title band).
        let content = rect(20.0, 50.0, 300.0, 560.0);
        let fit = fit_content(&frame, content, 60.0, 720.0, FitMode::Auto)
            .expect("content overruns the title band");

        let (x0, _) = apply(fit.transform, content.x0, content.y0);
        // Title band occupies page x < 792 - 720 = 72.
        assert!(
            x0 >= 72.0 - 0.01,
            "content still in the title band at {}",
            x0
        );
    }

    #[test]
    fn media_box_falls_back_to_letter() {
        let doc = Document::with_version("1.5");
        let r = page_media_box(&doc, (1, 0));
        assert_eq!(r.width(), 612.0);
        assert_eq!(r.height(), 792.0);
    }
}
