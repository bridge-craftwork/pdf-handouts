//! Direct header/footer writing to PDF pages using lopdf
//!
//! This module provides functionality to add headers and footers directly to PDF pages
//! without creating a separate watermark overlay file. This approach is simpler and more
//! reliable than the overlay method.

use crate::date::format_date;
use crate::error::Result;
use crate::pdf::bounds::content_bounds;
use crate::pdf::fit::{fit_content, page_media_box, Fit, FitAction, FitMode, Matrix, PageFrame};
use chrono::NaiveDate;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::path::Path;

/// Distance from the top of the layout frame to the title's baseline, in points.
const TITLE_BASELINE_FROM_TOP: f32 = 50.0;
/// Distance from the bottom of the layout frame to the lowest footer baseline.
const FOOTER_BASELINE_FROM_BOTTOM: f32 = 30.0;
/// Left and right margin for header and footer text, in points.
const SIDE_MARGIN: f32 = 50.0;
/// Footer line spacing as a multiple of the footer font size.
const FOOTER_LINE_SPACING: f32 = 1.2;
/// Glyph rise above the baseline, as a fraction of font size.
///
/// Used to work out how much vertical room the header and footer text needs so
/// source content can be moved clear of it.
const GLYPH_ASCENT_RATIO: f32 = 0.75;
/// Glyph drop below the baseline, as a fraction of font size.
const GLYPH_DESCENT_RATIO: f32 = 0.25;

/// Options for masking existing header/footer content
#[derive(Debug, Clone, Default)]
pub struct MaskOptions {
    /// Height of header mask on first page only (in inches)
    pub header_height: Option<f32>,
    /// Height of footer mask on first page only (in inches)
    pub footer_height: Option<f32>,
    /// Height of header mask on all pages (in inches)
    pub header_all_height: Option<f32>,
    /// Height of footer mask on all pages (in inches)
    pub footer_all_height: Option<f32>,
    /// Mask color as RGB tuple (0.0-1.0 for each component), defaults to white
    pub color: (f32, f32, f32),
}

impl MaskOptions {
    /// Create new MaskOptions with default white color
    pub fn new() -> Self {
        Self {
            header_height: None,
            footer_height: None,
            header_all_height: None,
            footer_all_height: None,
            color: (1.0, 1.0, 1.0), // White
        }
    }

    /// Check if any masking is configured
    pub fn has_any_mask(&self) -> bool {
        self.header_height.is_some()
            || self.footer_height.is_some()
            || self.header_all_height.is_some()
            || self.footer_all_height.is_some()
    }

    /// Get the effective header mask height for a given page
    pub fn effective_header_height(&self, is_first_page: bool) -> Option<f32> {
        // header_all takes precedence, then header (first page only)
        self.header_all_height.or({
            if is_first_page {
                self.header_height
            } else {
                None
            }
        })
    }

    /// Get the effective footer mask height for a given page
    pub fn effective_footer_height(&self, is_first_page: bool) -> Option<f32> {
        // footer_all takes precedence, then footer (first page only)
        self.footer_all_height.or({
            if is_first_page {
                self.footer_height
            } else {
                None
            }
        })
    }
}

/// Options for adding headers and footers to a PDF
#[derive(Debug, Clone)]
pub struct HeaderFooterOptions {
    /// Title to display on first page (centered at top)
    pub title: Option<String>,
    /// Footer left section content
    pub footer_left: Option<String>,
    /// Footer center section content
    pub footer_center: Option<String>,
    /// Footer right section content
    pub footer_right: Option<String>,
    /// Date to display in footer
    pub date: Option<NaiveDate>,
    /// Whether to show page numbers
    pub show_page_numbers: bool,
    /// Whether to show total page count (e.g., "Page 1 of 10")
    pub show_total_page_count: bool,
    /// Title font size in points (legacy - prefer header_font)
    pub title_font_size: f32,
    /// Footer font size in points (legacy - prefer footer_font)
    pub footer_font_size: f32,
    /// Header font specification (overrides title_font_size if set)
    pub header_font: Option<FontSpec>,
    /// Footer font specification (overrides footer_font_size if set)
    pub footer_font: Option<FontSpec>,
    /// Masking options for covering existing header/footer content
    pub mask: MaskOptions,
    /// How to adjust source content that collides with the header/footer bands
    pub fit: FitMode,
}

impl Default for HeaderFooterOptions {
    fn default() -> Self {
        Self {
            title: None,
            footer_left: None,
            footer_center: None,
            footer_right: None,
            date: None,
            show_page_numbers: true,
            show_total_page_count: false,
            title_font_size: 24.0,
            footer_font_size: 14.0,
            header_font: None,
            footer_font: None,
            mask: MaskOptions::new(),
            fit: FitMode::Auto,
        }
    }
}

impl HeaderFooterOptions {
    /// Get effective header font size
    pub fn effective_header_font_size(&self) -> f32 {
        self.header_font
            .as_ref()
            .and_then(|f| f.size)
            .unwrap_or(self.title_font_size)
    }

    /// Get effective footer font size
    pub fn effective_footer_font_size(&self) -> f32 {
        self.footer_font
            .as_ref()
            .and_then(|f| f.size)
            .unwrap_or(self.footer_font_size)
    }

    /// Get header color as PDF RGB string (e.g., "0 0 0" for black)
    pub fn header_color_pdf(&self) -> String {
        self.header_font
            .as_ref()
            .and_then(|f| f.color)
            .map(|(r, g, b)| format!("{:.3} {:.3} {:.3}", r, g, b))
            .unwrap_or_else(|| "0 0 0".to_string())
    }

    /// Get footer color as PDF RGB string (e.g., "0 0 0" for black)
    pub fn footer_color_pdf(&self) -> String {
        self.footer_font
            .as_ref()
            .and_then(|f| f.color)
            .map(|(r, g, b)| format!("{:.3} {:.3} {:.3}", r, g, b))
            .unwrap_or_else(|| "0 0 0".to_string())
    }
}

/// Add headers and footers directly to a PDF
///
/// This function uses the same approach as PDFtk's stamp command:
/// 1. Wrap original page content in q/Q to isolate its transformations
/// 2. Create a Form XObject containing headers/footers
/// 3. Draw the XObject after the Q (restore) so it uses clean coordinates
///
/// This approach works reliably with all PDFs, including Google Docs exports
/// that apply unusual coordinate transformations.
///
/// # Example
///
/// ```no_run
/// use pdf_handouts::pdf::{HeaderFooterOptions, add_headers_footers};
/// use std::path::Path;
/// use chrono::NaiveDate;
///
/// let options = HeaderFooterOptions {
///     title: Some("My Document".to_string()),
///     footer_center: Some("Page".to_string()),
///     show_page_numbers: true,
///     ..Default::default()
/// };
///
/// add_headers_footers(
///     Path::new("input.pdf"),
///     Path::new("output.pdf"),
///     &options
/// ).expect("Failed to add headers/footers");
/// ```
pub fn add_headers_footers(
    input_path: &Path,
    output_path: &Path,
    options: &HeaderFooterOptions,
) -> Result<()> {
    add_headers_footers_reporting(input_path, output_path, options).map(|_| ())
}

/// What happened to one page while headers and footers were added.
#[derive(Debug, Clone, Copy)]
pub struct PageFit {
    /// 1-based page number
    pub page: usize,
    /// Whether the page was laid out rotated (a landscape page)
    pub rotated: bool,
    /// The adjustment applied to the source content
    pub action: FitAction,
}

/// Add headers and footers, reporting how each page's content was adjusted.
///
/// Identical to [`add_headers_footers`], but returns a per-page record of what
/// the content fitting did so a caller can surface it to the user.
pub fn add_headers_footers_reporting(
    input_path: &Path,
    output_path: &Path,
    options: &HeaderFooterOptions,
) -> Result<Vec<PageFit>> {
    // Load the input (a PDF, or an image converted to a single-page PDF)
    let mut doc = crate::pdf::image::load_input_document(input_path)?;

    // Decompress for easier content stream parsing
    doc.decompress();

    let page_count = doc.get_pages().len();

    // Embed Liberation Serif font for proper text rendering
    let font_id = embed_liberation_serif(&mut doc)?;

    // Collect page info first (to avoid borrow issues)
    let pages: Vec<(usize, ObjectId)> = doc
        .get_pages()
        .iter()
        .enumerate()
        .map(|(i, (_num, id))| (i, *id))
        .collect();

    let mut report = Vec::with_capacity(pages.len());

    // For each page, wrap content in q/Q and add XObject overlay
    for (i, page_id) in pages.iter() {
        let page_number = i + 1;
        let is_first_page = page_number == 1;

        // Headers and footers are laid out in an upright frame, which for a
        // landscape page is the page turned a quarter turn (see PageFrame).
        let media_box = page_media_box(&doc, *page_id);
        let frame = PageFrame::new(media_box.width(), media_box.height());

        // Work out how much of the frame the header and footer will occupy,
        // then move or shrink the source content out of those bands.
        let fit = compute_page_fit(&doc, *page_id, &frame, is_first_page, options);
        report.push(PageFit {
            page: page_number,
            rotated: frame.rotated,
            action: fit.map_or(FitAction::Unchanged, |f| f.action),
        });

        // Generate the content stream for this page's headers/footers
        let content = generate_header_footer_content(
            page_number,
            page_count,
            is_first_page,
            frame.frame_width(),
            frame.frame_height(),
            options,
        );

        // Create a Form XObject (no inverse transform needed - we reset CTM with q/Q wrapper)
        let xobject_id = create_form_xobject(&mut doc, content, font_id)?;

        // Add the Form XObject to the page's Resources
        add_xobject_to_page_resources(&mut doc, *page_id, xobject_id)?;

        // Wrap original content in q/Q and append XObject invocation.
        // The Q resets the graphics state (including CTM), so the overlay is
        // drawn in clean page coordinates — rotated into place for a landscape
        // page. Any fit transform goes inside the wrapper, so it applies to the
        // source content only.
        wrap_content_and_append_xobject(
            &mut doc,
            *page_id,
            fit.map(|f| f.transform),
            frame.frame_to_page(),
        )?;
    }

    // Save the modified PDF
    doc.compress();
    doc.save(output_path)?;

    Ok(report)
}

/// Vertical breathing room left between source content and the header/footer.
const FIT_GAP: f32 = 6.0;

/// Decide how a page's content must move or shrink to clear the bands.
///
/// Returns `None` when nothing needs to change, when the page's ink cannot be
/// measured, or when a mask is active — a mask is a promise to cover a specific
/// region of the source, and moving the content underneath would break it.
fn compute_page_fit(
    doc: &Document,
    page_id: ObjectId,
    frame: &PageFrame,
    is_first_page: bool,
    options: &HeaderFooterOptions,
) -> Option<Fit> {
    if options.fit == FitMode::Off {
        return None;
    }
    if options
        .mask
        .effective_header_height(is_first_page)
        .is_some()
        || options
            .mask
            .effective_footer_height(is_first_page)
            .is_some()
    {
        return None;
    }

    let frame_height = frame.frame_height();

    // Top of the safe area: just under the title, on the page that has one.
    let safe_high = if is_first_page && options.title.is_some() {
        let header_size = options.effective_header_font_size();
        frame_height - TITLE_BASELINE_FROM_TOP - GLYPH_DESCENT_RATIO * header_size - FIT_GAP
    } else {
        frame_height
    };

    // Bottom of the safe area: just above the tallest footer column.
    let footer_lines = footer_line_count(options);
    let safe_low = if footer_lines > 0 {
        let footer_size = options.effective_footer_font_size();
        let line_height = footer_size * FOOTER_LINE_SPACING;
        FOOTER_BASELINE_FROM_BOTTOM
            + (footer_lines - 1) as f32 * line_height
            + GLYPH_ASCENT_RATIO * footer_size
            + FIT_GAP
    } else {
        0.0
    };

    let content = content_bounds(doc, page_id)?;
    fit_content(frame, content, safe_low, safe_high, options.fit)
}

/// The greatest number of lines any footer column will occupy.
fn footer_line_count(options: &HeaderFooterOptions) -> usize {
    [
        options.footer_left.as_ref(),
        options.footer_center.as_ref(),
        options.footer_right.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|text| parse_multiline_text(text).len())
    .max()
    .unwrap_or(0)
}

/// Use Helvetica (standard PDF font - simpler than embedding)
#[allow(dead_code)]
fn use_helvetica_font(doc: &mut Document) -> Result<ObjectId> {
    // Create a simple Type1 font using Helvetica (one of the 14 standard PDF fonts)
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));

    let font_id = doc.add_object(Object::Dictionary(font));
    Ok(font_id)
}

/// Embed Liberation Serif TrueType font with WinAnsiEncoding
///
/// This embeds the font data directly in the PDF so it renders correctly
/// on any system, regardless of whether the font is installed.
fn embed_liberation_serif(doc: &mut Document) -> Result<ObjectId> {
    // Load the embedded font data
    const LIBERATION_SERIF: &[u8] =
        include_bytes!("../../assets/fonts/LiberationSerif-Regular.ttf");

    // Create font stream object (the actual TTF data)
    let mut font_stream_dict = Dictionary::new();
    font_stream_dict.set("Length1", Object::Integer(LIBERATION_SERIF.len() as i64));

    let font_stream = Stream {
        dict: font_stream_dict,
        content: LIBERATION_SERIF.to_vec(),
        allows_compression: true,
        start_position: None,
    };
    let font_stream_id = doc.add_object(Object::Stream(font_stream));

    // Create font descriptor with metrics for Liberation Serif
    let mut font_descriptor = Dictionary::new();
    font_descriptor.set("Type", Object::Name(b"FontDescriptor".to_vec()));
    font_descriptor.set("FontName", Object::Name(b"LiberationSerif".to_vec()));
    font_descriptor.set(
        "FontFamily",
        Object::String(b"Liberation Serif".to_vec(), lopdf::StringFormat::Literal),
    );
    font_descriptor.set("Flags", Object::Integer(34)); // Serif + Nonsymbolic
    font_descriptor.set(
        "FontBBox",
        Object::Array(vec![
            Object::Integer(-543),
            Object::Integer(-303),
            Object::Integer(1300),
            Object::Integer(981),
        ]),
    );
    font_descriptor.set("ItalicAngle", Object::Integer(0));
    font_descriptor.set("Ascent", Object::Integer(891));
    font_descriptor.set("Descent", Object::Integer(-216));
    font_descriptor.set("CapHeight", Object::Integer(662));
    font_descriptor.set("XHeight", Object::Integer(450));
    font_descriptor.set("StemV", Object::Integer(84));
    font_descriptor.set("FontFile2", Object::Reference(font_stream_id));

    let font_descriptor_id = doc.add_object(Object::Dictionary(font_descriptor));

    // Create TrueType font with WinAnsiEncoding
    // WinAnsiEncoding allows us to use simple single-byte text strings
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"TrueType".to_vec()));
    font.set("BaseFont", Object::Name(b"LiberationSerif".to_vec()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    font.set("FontDescriptor", Object::Reference(font_descriptor_id));

    // First and last character codes (standard ASCII printable range)
    font.set("FirstChar", Object::Integer(32));
    font.set("LastChar", Object::Integer(255));

    // Widths array - Liberation Serif glyph widths for chars 32-255
    // These are approximate widths in 1/1000ths of the font size
    let widths = create_liberation_serif_widths();
    font.set("Widths", Object::Array(widths));

    let font_id = doc.add_object(Object::Dictionary(font));
    Ok(font_id)
}

/// Create widths array for Liberation Serif (chars 32-255)
/// Values are in 1/1000ths of the em square
fn create_liberation_serif_widths() -> Vec<Object> {
    // Liberation Serif widths for WinAnsiEncoding characters 32-255
    // These are approximate values based on typical serif font metrics
    let widths: Vec<i64> = vec![
        250,  // 32 space
        333,  // 33 !
        408,  // 34 "
        500,  // 35 #
        500,  // 36 $
        833,  // 37 %
        778,  // 38 &
        180,  // 39 '
        333,  // 40 (
        333,  // 41 )
        500,  // 42 *
        564,  // 43 +
        250,  // 44 ,
        333,  // 45 -
        250,  // 46 .
        278,  // 47 /
        500,  // 48 0
        500,  // 49 1
        500,  // 50 2
        500,  // 51 3
        500,  // 52 4
        500,  // 53 5
        500,  // 54 6
        500,  // 55 7
        500,  // 56 8
        500,  // 57 9
        278,  // 58 :
        278,  // 59 ;
        564,  // 60 <
        564,  // 61 =
        564,  // 62 >
        444,  // 63 ?
        921,  // 64 @
        722,  // 65 A
        667,  // 66 B
        667,  // 67 C
        722,  // 68 D
        611,  // 69 E
        556,  // 70 F
        722,  // 71 G
        722,  // 72 H
        333,  // 73 I
        389,  // 74 J
        722,  // 75 K
        611,  // 76 L
        889,  // 77 M
        722,  // 78 N
        722,  // 79 O
        556,  // 80 P
        722,  // 81 Q
        667,  // 82 R
        556,  // 83 S
        611,  // 84 T
        722,  // 85 U
        722,  // 86 V
        944,  // 87 W
        722,  // 88 X
        722,  // 89 Y
        611,  // 90 Z
        333,  // 91 [
        278,  // 92 \
        333,  // 93 ]
        469,  // 94 ^
        500,  // 95 _
        333,  // 96 `
        444,  // 97 a
        500,  // 98 b
        444,  // 99 c
        500,  // 100 d
        444,  // 101 e
        333,  // 102 f
        500,  // 103 g
        500,  // 104 h
        278,  // 105 i
        278,  // 106 j
        500,  // 107 k
        278,  // 108 l
        778,  // 109 m
        500,  // 110 n
        500,  // 111 o
        500,  // 112 p
        500,  // 113 q
        333,  // 114 r
        389,  // 115 s
        278,  // 116 t
        500,  // 117 u
        500,  // 118 v
        722,  // 119 w
        500,  // 120 x
        500,  // 121 y
        444,  // 122 z
        480,  // 123 {
        200,  // 124 |
        480,  // 125 }
        541,  // 126 ~
        350,  // 127 DEL (placeholder)
        500,  // 128 Euro (placeholder)
        350,  // 129 undefined
        333,  // 130 single low quote
        500,  // 131 f with hook
        444,  // 132 double low quote
        1000, // 133 ellipsis
        500,  // 134 dagger
        500,  // 135 double dagger
        333,  // 136 circumflex
        1000, // 137 per mille
        556,  // 138 S caron
        333,  // 139 single left angle quote
        889,  // 140 OE
        350,  // 141 undefined
        611,  // 142 Z caron
        350,  // 143 undefined
        350,  // 144 undefined
        333,  // 145 left single quote
        333,  // 146 right single quote
        444,  // 147 left double quote
        444,  // 148 right double quote
        350,  // 149 bullet
        500,  // 150 en dash
        1000, // 151 em dash
        333,  // 152 tilde
        980,  // 153 trademark
        389,  // 154 s caron
        333,  // 155 single right angle quote
        722,  // 156 oe
        350,  // 157 undefined
        444,  // 158 z caron
        722,  // 159 Y dieresis
        250,  // 160 non-breaking space
        333,  // 161 inverted !
        500,  // 162 cent
        500,  // 163 pound
        500,  // 164 currency
        500,  // 165 yen
        200,  // 166 broken bar
        500,  // 167 section
        333,  // 168 dieresis
        760,  // 169 copyright
        276,  // 170 feminine ordinal
        500,  // 171 left guillemet
        564,  // 172 not
        333,  // 173 soft hyphen
        760,  // 174 registered
        333,  // 175 macron
        400,  // 176 degree
        564,  // 177 plus minus
        300,  // 178 superscript 2
        300,  // 179 superscript 3
        333,  // 180 acute
        500,  // 181 mu
        453,  // 182 pilcrow
        250,  // 183 middle dot
        333,  // 184 cedilla
        300,  // 185 superscript 1
        310,  // 186 masculine ordinal
        500,  // 187 right guillemet
        750,  // 188 1/4
        750,  // 189 1/2
        750,  // 190 3/4
        444,  // 191 inverted ?
        722,  // 192 A grave
        722,  // 193 A acute
        722,  // 194 A circumflex
        722,  // 195 A tilde
        722,  // 196 A dieresis
        722,  // 197 A ring
        889,  // 198 AE
        667,  // 199 C cedilla
        611,  // 200 E grave
        611,  // 201 E acute
        611,  // 202 E circumflex
        611,  // 203 E dieresis
        333,  // 204 I grave
        333,  // 205 I acute
        333,  // 206 I circumflex
        333,  // 207 I dieresis
        722,  // 208 Eth
        722,  // 209 N tilde
        722,  // 210 O grave
        722,  // 211 O acute
        722,  // 212 O circumflex
        722,  // 213 O tilde
        722,  // 214 O dieresis
        564,  // 215 multiply
        722,  // 216 O stroke
        722,  // 217 U grave
        722,  // 218 U acute
        722,  // 219 U circumflex
        722,  // 220 U dieresis
        722,  // 221 Y acute
        556,  // 222 Thorn
        500,  // 223 sharp s
        444,  // 224 a grave
        444,  // 225 a acute
        444,  // 226 a circumflex
        444,  // 227 a tilde
        444,  // 228 a dieresis
        444,  // 229 a ring
        667,  // 230 ae
        444,  // 231 c cedilla
        444,  // 232 e grave
        444,  // 233 e acute
        444,  // 234 e circumflex
        444,  // 235 e dieresis
        278,  // 236 i grave
        278,  // 237 i acute
        278,  // 238 i circumflex
        278,  // 239 i dieresis
        500,  // 240 eth
        500,  // 241 n tilde
        500,  // 242 o grave
        500,  // 243 o acute
        500,  // 244 o circumflex
        500,  // 245 o tilde
        500,  // 246 o dieresis
        564,  // 247 divide
        500,  // 248 o stroke
        500,  // 249 u grave
        500,  // 250 u acute
        500,  // 251 u circumflex
        500,  // 252 u dieresis
        500,  // 253 y acute
        500,  // 254 thorn
        500,  // 255 y dieresis
    ];

    widths.into_iter().map(Object::Integer).collect()
}

/// Generate PDF content stream operators for headers/footers
fn generate_header_footer_content(
    page_num: usize,
    total_pages: usize,
    is_first_page: bool,
    page_width: f32,
    page_height: f32,
    options: &HeaderFooterOptions,
) -> String {
    let mut content = String::new();

    // Points per inch for converting mask heights
    const POINTS_PER_INCH: f32 = 72.0;

    // Draw mask rectangles FIRST (so they appear behind text)
    // Header mask (at top of page)
    if let Some(header_height_inches) = options.mask.effective_header_height(is_first_page) {
        let height_pt = header_height_inches * POINTS_PER_INCH;
        let (r, g, b) = options.mask.color;
        // Set fill color
        content.push_str(&format!("{:.3} {:.3} {:.3} rg\n", r, g, b));
        // Draw rectangle: x y width height re (rectangle) f (fill)
        // Header is at top of page, so y = page_height - height
        content.push_str(&format!(
            "0 {} {} {} re f\n",
            page_height - height_pt,
            page_width,
            height_pt
        ));
    }

    // Footer mask (at bottom of page)
    if let Some(footer_height_inches) = options.mask.effective_footer_height(is_first_page) {
        let height_pt = footer_height_inches * POINTS_PER_INCH;
        let (r, g, b) = options.mask.color;
        // Set fill color
        content.push_str(&format!("{:.3} {:.3} {:.3} rg\n", r, g, b));
        // Draw rectangle at bottom of page (y = 0)
        content.push_str(&format!("0 0 {} {} re f\n", page_width, height_pt));
    }

    // Get effective font sizes from options (respects FontSpec if set)
    let header_font_size = options.effective_header_font_size();
    let footer_font_size = options.effective_footer_font_size();

    // Add title on first page
    if is_first_page {
        if let Some(ref title) = options.title {
            // Expand placeholders in title
            let expanded_title =
                expand_placeholders(title, page_num, total_pages, options.date.as_ref());

            // Position title 50pt from top of page (PDF coordinates: bottom-left origin)
            let title_y = page_height - TITLE_BASELINE_FROM_TOP;
            let title_width = estimate_text_width(&expanded_title, header_font_size);
            let title_x = (page_width - title_width) / 2.0; // Center

            // Set header color (RGB)
            let header_color = options.header_color_pdf();
            content.push_str(&format!("{} rg\n", header_color)); // Fill color
            content.push_str(&format!("{} RG\n", header_color)); // Stroke color

            content.push_str("BT\n");
            content.push_str("0 Tr\n"); // Fill text
            content.push_str(&format!("/F1 {} Tf\n", header_font_size));
            content.push_str(&format!("1 0 0 1 {} {} Tm\n", title_x, title_y));
            content.push_str(&format!("({}) Tj\n", escape_pdf_string(&expanded_title)));
            content.push_str("ET\n");
        }
    }

    // Set footer color (RGB)
    let footer_color = options.footer_color_pdf();
    content.push_str(&format!("{} rg\n", footer_color)); // Fill color
    content.push_str(&format!("{} RG\n", footer_color)); // Stroke color

    // Add footers
    // We position footer lines starting from the bottom of the page, with the
    // first line at the top of the footer area and subsequent lines below it.
    let line_height = footer_font_size * FOOTER_LINE_SPACING;

    // Footer left
    if let Some(ref left_text) = options.footer_left {
        // Expand placeholders first, then parse lines
        let expanded = expand_placeholders(left_text, page_num, total_pages, options.date.as_ref());
        let lines = parse_multiline_text(&expanded);
        let num_lines = lines.len();
        // Calculate top of footer area: start high enough to fit all lines above the margin
        let footer_top = FOOTER_BASELINE_FROM_BOTTOM + ((num_lines - 1) as f32 * line_height);
        for (i, line) in lines.iter().enumerate() {
            // First line at top, subsequent lines below (Y decreases)
            let y = footer_top - (i as f32 * line_height);
            // Use font tag rendering for styled text
            content.push_str(&generate_line_with_font_tags(
                line,
                SIDE_MARGIN,
                y,
                footer_font_size,
            ));
        }
    }

    // Footer center
    if let Some(ref center_text) = options.footer_center {
        // Expand placeholders first, then parse lines
        let expanded =
            expand_placeholders(center_text, page_num, total_pages, options.date.as_ref());
        let lines = parse_multiline_text(&expanded);
        let num_lines = lines.len();
        let footer_top = FOOTER_BASELINE_FROM_BOTTOM + ((num_lines - 1) as f32 * line_height);
        for (i, line) in lines.iter().enumerate() {
            let y = footer_top - (i as f32 * line_height);
            // Use width calculation that excludes font tags
            let text_width = estimate_text_width_with_tags(line, footer_font_size);
            let x = (page_width - text_width) / 2.0;
            // Use font tag rendering for styled text
            content.push_str(&generate_line_with_font_tags(line, x, y, footer_font_size));
        }
    }

    // Footer right - now uses placeholder-based content like other footers
    if let Some(ref right_text) = options.footer_right {
        // Expand placeholders first, then parse lines
        let expanded =
            expand_placeholders(right_text, page_num, total_pages, options.date.as_ref());
        let lines = parse_multiline_text(&expanded);
        let num_lines = lines.len();
        let footer_top =
            FOOTER_BASELINE_FROM_BOTTOM + ((num_lines.saturating_sub(1)) as f32 * line_height);
        for (i, line) in lines.iter().enumerate() {
            let y = footer_top - (i as f32 * line_height);
            // Use width calculation that excludes font tags
            let text_width = estimate_text_width_with_tags(line, footer_font_size);
            let x = page_width - SIDE_MARGIN - text_width; // Right-aligned with margin
                                                           // Use font tag rendering for styled text
            content.push_str(&generate_line_with_font_tags(line, x, y, footer_font_size));
        }
    }

    content
}

/// Expand placeholders in text
///
/// Supported placeholders:
/// - `[page]` - current page number
/// - `[pages]` - total page count
/// - `[date]` - formatted date (if provided)
fn expand_placeholders(
    text: &str,
    page_num: usize,
    total_pages: usize,
    date: Option<&NaiveDate>,
) -> String {
    let mut result = text.to_string();

    // Replace page placeholders (case-insensitive)
    result = result.replace("[page]", &page_num.to_string());
    result = result.replace("[PAGE]", &page_num.to_string());
    result = result.replace("[pages]", &total_pages.to_string());
    result = result.replace("[PAGES]", &total_pages.to_string());

    // Replace date placeholder
    if let Some(d) = date {
        let formatted = format_date(d);
        result = result.replace("[date]", &formatted);
        result = result.replace("[DATE]", &formatted);
    } else {
        // Remove date placeholder if no date provided
        result = result.replace("[date]", "");
        result = result.replace("[DATE]", "");
    }

    result
}

/// Parsed font specification from CLI-style string
///
/// Format: `[weight] [style] [size] [family] [color]`
/// Examples:
/// - `14pt` (size only)
/// - `bold 14pt` (weight and size)
/// - `italic 12pt Liberation_Serif` (style, size, family)
/// - `bold italic 16pt Times_New_Roman #333333` (all components)
///
/// All components are optional. Underscores in family names are converted to spaces.
#[derive(Debug, Clone, Default)]
pub struct FontSpec {
    /// Font weight (normal or bold)
    pub bold: bool,
    /// Font style (normal or italic)
    pub italic: bool,
    /// Font size in points (None = use default)
    pub size: Option<f32>,
    /// Font family name (None = use default)
    pub family: Option<String>,
    /// Text color as RGB tuple (0.0-1.0 for each component)
    pub color: Option<(f32, f32, f32)>,
}

impl FontSpec {
    /// Parse a font specification string
    ///
    /// Format: `[bold] [italic] [size[pt]] [family_name] [#rrggbb]`
    ///
    /// Examples:
    /// - `"14pt"` -> size 14
    /// - `"bold 14pt"` -> bold, size 14
    /// - `"italic 12pt Liberation_Serif"` -> italic, size 12, Liberation Serif
    /// - `"bold italic 16pt #ff0000"` -> bold italic, size 16, red color
    pub fn parse(spec: &str) -> Self {
        let mut result = Self::default();
        let tokens: Vec<&str> = spec.split_whitespace().collect();

        for token in tokens {
            let lower = token.to_lowercase();

            if lower == "bold" {
                result.bold = true;
            } else if lower == "italic" {
                result.italic = true;
            } else if lower.starts_with('#') && (token.len() == 7 || token.len() == 4) {
                // Hex color
                result.color = parse_hex_color(token);
            } else if let Some(size) = parse_size_token(&lower) {
                result.size = Some(size);
            } else if !lower.is_empty() {
                // Assume it's a font family name (convert underscores to spaces)
                result.family = Some(token.replace('_', " "));
            }
        }

        result
    }

    /// Create a FontSpec with just a size
    pub fn with_size(size: f32) -> Self {
        Self {
            size: Some(size),
            ..Default::default()
        }
    }
}

/// Parse a size token like "14pt", "14", "14.5pt"
fn parse_size_token(token: &str) -> Option<f32> {
    let cleaned = token.trim_end_matches("pt");
    cleaned.parse::<f32>().ok()
}

/// Parse a hex color like "#ff0000" or "#f00" to RGB tuple (0.0-1.0)
fn parse_hex_color(hex: &str) -> Option<(f32, f32, f32)> {
    let hex = hex.trim_start_matches('#');

    if hex.len() == 6 {
        // Full hex: #rrggbb
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
    } else if hex.len() == 3 {
        // Short hex: #rgb -> #rrggbb
        let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
        Some((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
    } else {
        None
    }
}

/// Font style for inline text formatting
#[derive(Debug, Clone, Default)]
struct FontStyle {
    italic: bool,
    bold: bool,
}

/// A segment of text with optional font styling
#[derive(Debug, Clone)]
struct TextSegment {
    text: String,
    style: FontStyle,
}

/// Parse text containing [font]...[/font] tags into segments
///
/// Syntax: `[font italic]text[/font]` or `[font bold]text[/font]`
/// Multiple styles: `[font bold italic]text[/font]`
/// Underscores replace spaces in font names: `[font 12pt Liberation_Serif]`
fn parse_font_tags(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Look for [font ...] tag
        if let Some(font_start) = remaining.find("[font ") {
            // Add text before the tag as plain segment
            if font_start > 0 {
                segments.push(TextSegment {
                    text: remaining[..font_start].to_string(),
                    style: FontStyle::default(),
                });
            }

            // Find the closing ] of the opening tag
            if let Some(tag_end) = remaining[font_start..].find(']') {
                let tag_end = font_start + tag_end;
                let tag_content = &remaining[font_start + 6..tag_end]; // Skip "[font "

                // Parse style from tag content
                let style = parse_font_style(tag_content);

                // Find the closing [/font] tag
                let after_tag = &remaining[tag_end + 1..];
                if let Some(close_pos) = after_tag.find("[/font]") {
                    // Extract styled text
                    let styled_text = &after_tag[..close_pos];
                    segments.push(TextSegment {
                        text: styled_text.to_string(),
                        style,
                    });

                    // Continue after [/font]
                    remaining = &after_tag[close_pos + 7..];
                } else {
                    // No closing tag found, treat rest as styled
                    segments.push(TextSegment {
                        text: after_tag.to_string(),
                        style,
                    });
                    break;
                }
            } else {
                // Malformed tag, add rest as plain text
                segments.push(TextSegment {
                    text: remaining.to_string(),
                    style: FontStyle::default(),
                });
                break;
            }
        } else {
            // No more font tags, add rest as plain text
            segments.push(TextSegment {
                text: remaining.to_string(),
                style: FontStyle::default(),
            });
            break;
        }
    }

    segments
}

/// Parse font style from tag content like "italic", "bold", "bold italic"
fn parse_font_style(tag_content: &str) -> FontStyle {
    let lower = tag_content.to_lowercase();
    FontStyle {
        italic: lower.contains("italic"),
        bold: lower.contains("bold"),
    }
}

/// Generate PDF content for a single line with font tag support
fn generate_line_with_font_tags(line: &str, x: f32, y: f32, font_size: f32) -> String {
    let segments = parse_font_tags(line);
    let mut content = String::new();
    let mut current_x = x;

    for segment in segments {
        if segment.text.is_empty() {
            continue;
        }

        content.push_str("BT\n");
        content.push_str(&format!("/F1 {} Tf\n", font_size));

        // Apply transformations for style
        if segment.style.italic && segment.style.bold {
            // Bold italic: shear + thicker stroke
            // Matrix: [1 0 tan(12°) 1 x y] for italic shear
            let shear = 0.21; // tan(12°) ≈ 0.21
            content.push_str(&format!("1 0 {} 1 {} {} Tm\n", shear, current_x, y));
            content.push_str("2 Tr\n"); // Stroke + fill for bold effect
            content.push_str(&format!("{} w\n", font_size * 0.03)); // Stroke width
        } else if segment.style.italic {
            // Italic: apply shear transformation
            let shear = 0.21; // tan(12°) ≈ 0.21
            content.push_str(&format!("1 0 {} 1 {} {} Tm\n", shear, current_x, y));
            content.push_str("0 Tr\n"); // Fill only
        } else if segment.style.bold {
            // Bold: use stroke + fill rendering mode
            content.push_str(&format!("1 0 0 1 {} {} Tm\n", current_x, y));
            content.push_str("2 Tr\n"); // Stroke + fill for bold effect
            content.push_str(&format!("{} w\n", font_size * 0.03)); // Stroke width
        } else {
            // Normal text
            content.push_str(&format!("1 0 0 1 {} {} Tm\n", current_x, y));
            content.push_str("0 Tr\n"); // Fill only
        }

        content.push_str(&format!("({}) Tj\n", escape_pdf_string(&segment.text)));
        content.push_str("ET\n");

        // Advance x position for next segment
        current_x += estimate_text_width(&segment.text, font_size);
    }

    content
}

/// Estimate text width excluding font tags
fn estimate_text_width_with_tags(text: &str, font_size: f32) -> f32 {
    let segments = parse_font_tags(text);
    segments
        .iter()
        .map(|s| estimate_text_width(&s.text, font_size))
        .sum()
}

/// Parse text with line break markers
fn parse_multiline_text(text: &str) -> Vec<String> {
    text.split('\n')
        .flat_map(|line| line.split('|'))
        .flat_map(|part| part.split("[br]"))
        .flat_map(|part| part.split("[BR]"))
        .flat_map(|part| part.split("<br>"))
        .flat_map(|part| part.split("<BR>"))
        .flat_map(|part| part.split("<br/>"))
        .flat_map(|part| part.split("<BR/>"))
        .flat_map(|part| part.split("<br />"))
        .flat_map(|part| part.split("<BR />"))
        .map(|s| s.to_string())
        .collect()
}

/// Escape special characters in PDF strings
fn escape_pdf_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

/// Estimate text width for Liberation Serif
fn estimate_text_width(text: &str, font_size: f32) -> f32 {
    // Use average character width from the widths table
    // Average width is approximately 480/1000 = 0.48 em
    text.len() as f32 * font_size * 0.48
}

/// Create a Form XObject for headers/footers
///
/// The Form XObject has its own coordinate system defined by BBox.
/// Since we wrap the original page content in q/Q before invoking this XObject,
/// it renders in standard page coordinates (identity CTM).
fn create_form_xobject(doc: &mut Document, content: String, font_id: ObjectId) -> Result<ObjectId> {
    // Create Resources dictionary for the Form XObject
    let mut resources = Dictionary::new();
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    resources.set("Font", Object::Dictionary(fonts));

    // Create the Form XObject dictionary
    let mut xobject_dict = Dictionary::new();
    xobject_dict.set("Type", Object::Name(b"XObject".to_vec()));
    xobject_dict.set("Subtype", Object::Name(b"Form".to_vec()));
    xobject_dict.set("FormType", Object::Integer(1));

    // BBox defines the Form's coordinate system - use standard Letter size
    xobject_dict.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(612),
            Object::Integer(792),
        ]),
    );

    // Identity matrix - our XObject uses standard page coordinates
    xobject_dict.set(
        "Matrix",
        Object::Array(vec![
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(0),
        ]),
    );

    xobject_dict.set("Resources", Object::Dictionary(resources));

    // Create the stream with the content
    // Note: Must use Stream::new() which automatically sets the Length entry in the
    // stream dictionary. The struct constructor doesn't set Length, causing the
    // stream content to be lost when saving the PDF.
    let xobject_stream = Stream::new(xobject_dict, content.into_bytes());

    let xobject_id = doc.add_object(Object::Stream(xobject_stream));
    Ok(xobject_id)
}

/// Count the q/Q imbalance in a content stream
///
/// Returns a positive number if there are more q than Q operators,
/// meaning there are unclosed graphics states.
fn count_graphics_state_imbalance(content: &[u8]) -> i32 {
    let content_str = String::from_utf8_lossy(content);
    let mut depth: i32 = 0;

    // Simple tokenization - look for standalone 'q' and 'Q' operators
    // They should be preceded by whitespace/start and followed by whitespace/end
    let bytes = content_str.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        let c = bytes[i];

        // Check for 'q' or 'Q'
        if c == b'q' || c == b'Q' {
            // Check it's not part of a larger token (like /Fq or BQ)
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let next_ok = i + 1 >= len || !bytes[i + 1].is_ascii_alphanumeric();

            if prev_ok && next_ok {
                if c == b'q' {
                    depth += 1;
                } else {
                    depth -= 1;
                }
            }
        }
    }

    depth
}

/// Wrap page content in q/Q and append XObject invocation
///
/// This is the key to making headers/footers work with any PDF:
/// 1. Prepend "q\n" to save graphics state before original content
/// 2. Append enough Q operators to close all unclosed states in original content
/// 3. Append one more Q to close our initial q
/// 4. Append XObject invocation in clean coordinate space
///
/// The structure becomes:
/// ```text
/// Stream 1: q
/// Stream 2: [original content]
/// Stream 3: Q Q Q... (enough to balance)
///           q 1 0 0 1 0 0 cm /HeaderFooter Do Q
/// ```
fn wrap_content_and_append_xobject(
    doc: &mut Document,
    page_id: ObjectId,
    fit_transform: Option<Matrix>,
    overlay_transform: Matrix,
) -> Result<()> {
    // First, read existing content to count q/Q imbalance
    let imbalance = {
        let page_obj = doc.get_object(page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            if let Ok(contents) = page_dict.get(b"Contents") {
                let content_ids: Vec<ObjectId> = match contents {
                    Object::Reference(id) => vec![*id],
                    Object::Array(arr) => arr
                        .iter()
                        .filter_map(|o| {
                            if let Object::Reference(id) = o {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .collect(),
                    _ => vec![],
                };

                let mut total_imbalance: i32 = 0;
                for content_id in content_ids {
                    if let Ok(Object::Stream(stream)) = doc.get_object(content_id) {
                        total_imbalance += count_graphics_state_imbalance(&stream.content);
                    }
                }
                total_imbalance
            } else {
                0
            }
        } else {
            0
        }
    };

    // Create the opening stream: save the graphics state, then apply the fit
    // transform (if any) so it affects the source content and nothing else.
    let mut q_content = String::from("q\n");
    if let Some(m) = fit_transform {
        q_content.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} cm\n",
            m[0], m[1], m[2], m[3], m[4], m[5]
        ));
    }
    let q_stream_id = doc.add_object(Stream::new(Dictionary::new(), q_content.into_bytes()));

    // Build the closing stream:
    // - First, close any unclosed graphics states from original content
    // - Then close our initial q
    // - Then draw our XObject
    let mut qx_content = String::new();

    // Close unclosed states from original content (if any)
    // imbalance > 0 means there are unclosed q's
    for _ in 0..imbalance.max(0) {
        qx_content.push_str(" Q\n");
    }

    // Close our initial q
    qx_content.push_str(" Q\n");

    // Draw our XObject in clean coordinate space. On a landscape page the
    // overlay transform is a quarter turn, which puts the title and footer
    // along the short edges instead of the long ones.
    qx_content.push_str(&format!(
        "q {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} cm /HeaderFooter Do Q\n",
        overlay_transform[0],
        overlay_transform[1],
        overlay_transform[2],
        overlay_transform[3],
        overlay_transform[4],
        overlay_transform[5]
    ));

    let qx_stream_id = doc.add_object(Stream::new(Dictionary::new(), qx_content.into_bytes()));

    // Get the page and modify its Contents
    let page_obj = doc.get_object_mut(page_id)?;

    if let Object::Dictionary(ref mut page_dict) = page_obj {
        let existing_content = page_dict.get(b"Contents").ok().cloned();

        let new_contents = match existing_content {
            Some(Object::Reference(content_id)) => {
                // Single content stream -> wrap it
                vec![
                    Object::Reference(q_stream_id),
                    Object::Reference(content_id),
                    Object::Reference(qx_stream_id),
                ]
            }
            Some(Object::Array(content_array)) => {
                // Multiple content streams -> wrap them all
                let mut new_array = vec![Object::Reference(q_stream_id)];
                new_array.extend(content_array);
                new_array.push(Object::Reference(qx_stream_id));
                new_array
            }
            _ => {
                // No existing content - just add our XObject invocation
                vec![
                    Object::Reference(q_stream_id),
                    Object::Reference(qx_stream_id),
                ]
            }
        };

        page_dict.set("Contents", Object::Array(new_contents));
    }

    Ok(())
}

/// Add XObject reference to page's Resources dictionary
fn add_xobject_to_page_resources(
    doc: &mut Document,
    page_id: ObjectId,
    xobject_id: ObjectId,
) -> Result<()> {
    // First, get the resources dictionary and XObject subdictionary
    // We need to dereference both if they are references
    let (resources_dict, xobjects_dict) = {
        let page_obj = doc.get_object(page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            // Try to get Resources from page directly
            let res_dict = if let Ok(res) = page_dict.get(b"Resources") {
                match res {
                    Object::Dictionary(dict) => dict.clone(),
                    Object::Reference(res_id) => {
                        // Dereference to get the actual Resources dictionary
                        if let Ok(Object::Dictionary(dict)) = doc.get_object(*res_id) {
                            dict.clone()
                        } else {
                            Dictionary::new()
                        }
                    }
                    _ => Dictionary::new(),
                }
            } else {
                // No Resources on page - check parent for inherited Resources
                if let Ok(Object::Reference(parent_id)) = page_dict.get(b"Parent") {
                    get_inherited_resources(doc, *parent_id)
                } else {
                    Dictionary::new()
                }
            };

            // Now get the XObject subdictionary, dereferencing if needed
            let xo_dict = if let Ok(xo) = res_dict.get(b"XObject") {
                match xo {
                    Object::Dictionary(dict) => dict.clone(),
                    Object::Reference(xo_id) => {
                        // Dereference to get the actual XObject dictionary
                        if let Ok(Object::Dictionary(dict)) = doc.get_object(*xo_id) {
                            dict.clone()
                        } else {
                            Dictionary::new()
                        }
                    }
                    _ => Dictionary::new(),
                }
            } else {
                Dictionary::new()
            };

            (res_dict, xo_dict)
        } else {
            (Dictionary::new(), Dictionary::new())
        }
    };

    // Now modify the page with the updated resources
    let page_obj = doc.get_object_mut(page_id)?;

    if let Object::Dictionary(ref mut page_dict) = page_obj {
        let mut new_resources = resources_dict;

        // Use the dereferenced XObject subdictionary and add our header/footer
        let mut xobjects = xobjects_dict;
        xobjects.set("HeaderFooter", Object::Reference(xobject_id));

        new_resources.set("XObject", Object::Dictionary(xobjects));

        // Set the Resources directly on the page (not as a reference)
        // This ensures the page has its own copy with our XObject
        page_dict.set("Resources", Object::Dictionary(new_resources));
    }

    Ok(())
}

/// Get Resources from page tree parent (handles inheritance)
fn get_inherited_resources(doc: &Document, parent_id: ObjectId) -> Dictionary {
    if let Ok(Object::Dictionary(parent_dict)) = doc.get_object(parent_id) {
        // Check for Resources on this parent
        if let Ok(res) = parent_dict.get(b"Resources") {
            match res {
                Object::Dictionary(dict) => return dict.clone(),
                Object::Reference(res_id) => {
                    if let Ok(Object::Dictionary(dict)) = doc.get_object(*res_id) {
                        return dict.clone();
                    }
                }
                _ => {}
            }
        }

        // Not found here, check grandparent
        if let Ok(Object::Reference(grandparent_id)) = parent_dict.get(b"Parent") {
            return get_inherited_resources(doc, *grandparent_id);
        }
    }

    Dictionary::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_spec_parse_size_only() {
        let spec = FontSpec::parse("14pt");
        assert!(!spec.bold);
        assert!(!spec.italic);
        assert_eq!(spec.size, Some(14.0));
        assert!(spec.family.is_none());
        assert!(spec.color.is_none());
    }

    #[test]
    fn test_font_spec_parse_size_no_unit() {
        let spec = FontSpec::parse("12");
        assert_eq!(spec.size, Some(12.0));
    }

    #[test]
    fn test_font_spec_parse_bold() {
        let spec = FontSpec::parse("bold 16pt");
        assert!(spec.bold);
        assert!(!spec.italic);
        assert_eq!(spec.size, Some(16.0));
    }

    #[test]
    fn test_font_spec_parse_italic() {
        let spec = FontSpec::parse("italic 14pt");
        assert!(!spec.bold);
        assert!(spec.italic);
        assert_eq!(spec.size, Some(14.0));
    }

    #[test]
    fn test_font_spec_parse_bold_italic() {
        let spec = FontSpec::parse("bold italic 18pt");
        assert!(spec.bold);
        assert!(spec.italic);
        assert_eq!(spec.size, Some(18.0));
    }

    #[test]
    fn test_font_spec_parse_family_with_underscores() {
        let spec = FontSpec::parse("14pt Liberation_Serif");
        assert_eq!(spec.size, Some(14.0));
        assert_eq!(spec.family, Some("Liberation Serif".to_string()));
    }

    #[test]
    fn test_font_spec_parse_hex_color_full() {
        let spec = FontSpec::parse("14pt #ff0000");
        assert_eq!(spec.size, Some(14.0));
        let color = spec.color.unwrap();
        assert!((color.0 - 1.0).abs() < 0.01); // Red
        assert!(color.1.abs() < 0.01); // Green
        assert!(color.2.abs() < 0.01); // Blue
    }

    #[test]
    fn test_font_spec_parse_hex_color_short() {
        let spec = FontSpec::parse("14pt #f00");
        let color = spec.color.unwrap();
        assert!((color.0 - 1.0).abs() < 0.01); // Red
        assert!(color.1.abs() < 0.01); // Green
        assert!(color.2.abs() < 0.01); // Blue
    }

    #[test]
    fn test_font_spec_parse_hex_color_gray() {
        let spec = FontSpec::parse("#333333");
        let color = spec.color.unwrap();
        let expected = 0x33 as f32 / 255.0;
        assert!((color.0 - expected).abs() < 0.01);
        assert!((color.1 - expected).abs() < 0.01);
        assert!((color.2 - expected).abs() < 0.01);
    }

    #[test]
    fn test_font_spec_parse_full_spec() {
        let spec = FontSpec::parse("bold italic 24pt Times_New_Roman #0000ff");
        assert!(spec.bold);
        assert!(spec.italic);
        assert_eq!(spec.size, Some(24.0));
        assert_eq!(spec.family, Some("Times New Roman".to_string()));
        let color = spec.color.unwrap();
        assert!(color.0.abs() < 0.01); // Red
        assert!(color.1.abs() < 0.01); // Green
        assert!((color.2 - 1.0).abs() < 0.01); // Blue
    }

    #[test]
    fn test_font_spec_parse_empty() {
        let spec = FontSpec::parse("");
        assert!(!spec.bold);
        assert!(!spec.italic);
        assert!(spec.size.is_none());
        assert!(spec.family.is_none());
        assert!(spec.color.is_none());
    }

    #[test]
    fn test_font_spec_case_insensitive() {
        let spec = FontSpec::parse("BOLD ITALIC 14PT");
        assert!(spec.bold);
        assert!(spec.italic);
        assert_eq!(spec.size, Some(14.0));
    }

    #[test]
    fn test_parse_hex_color() {
        // Full hex
        let color = parse_hex_color("#ff8800").unwrap();
        assert!((color.0 - 1.0).abs() < 0.01);
        assert!((color.1 - 0.533).abs() < 0.01);
        assert!(color.2.abs() < 0.01);

        // Short hex
        let color = parse_hex_color("#f80").unwrap();
        assert!((color.0 - 1.0).abs() < 0.01);
        assert!((color.1 - 0.533).abs() < 0.01);
        assert!(color.2.abs() < 0.01);

        // Invalid lengths
        assert!(parse_hex_color("#12345").is_none()); // Wrong length
        assert!(parse_hex_color("#1234567").is_none()); // Too long
        assert!(parse_hex_color("#12").is_none()); // Too short
    }
}
