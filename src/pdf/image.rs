//! Image input support: converting raster images into single-page PDFs.
//!
//! Handout source folders often mix PDFs with screenshots (PNG, JPEG, ...).
//! Rather than requiring a separate conversion step, inputs are classified by
//! their content and images are rendered to a one-page PDF on the fly, so they
//! can be merged alongside real PDFs.
//!
//! Image pages are laid out to match the rest of the handout: US Letter, with
//! the page rotated to landscape when the image is wider than it is tall, and
//! the image scaled to fit inside the margins and centred. Keeping the page
//! size consistent means headers and footers land in the same place on every
//! page of the merged output.

use crate::error::{Error, Result};
use std::io::Read;
use std::path::Path;

/// US Letter width in points (8.5in x 72).
const LETTER_SHORT_EDGE: f32 = 612.0;
/// US Letter height in points (11in x 72).
const LETTER_LONG_EDGE: f32 = 792.0;
/// Margin along the layout frame's left and right edges, in points (0.5in).
const IMAGE_PAGE_SIDE_MARGIN: f32 = 36.0;
/// Margin along the layout frame's top and bottom edges, in points (1in).
///
/// Deeper than the side margin so the image clears the bands that
/// [`crate::pdf::headers`] draws into:
///
/// - Footer: the lowest baseline sits 30pt up from the bottom edge, and each
///   extra line adds `font_size * 1.2`. A two-line 14pt footer — what the
///   handout workflow uses — tops out near 30 + 16.8 + ascender ≈ 57pt.
/// - Title: baseline 50pt down from the top, ascending to roughly 26pt from
///   the top for the default 24pt size.
///
/// 72pt clears both with margin to spare. A footer of four or more lines, or
/// an unusually large footer font, can still reach into the image.
const IMAGE_PAGE_EDGE_MARGIN: f32 = 72.0;

/// A raster image format that can be converted to PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// Portable Network Graphics
    Png,
    /// JPEG / JFIF
    Jpeg,
    /// Graphics Interchange Format
    Gif,
    /// WebP
    WebP,
}

impl ImageFormat {
    /// Human-readable name of the format, for error messages.
    pub fn name(self) -> &'static str {
        match self {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Gif => "GIF",
            ImageFormat::WebP => "WebP",
        }
    }
}

/// What kind of file an input path holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// An existing PDF document.
    Pdf,
    /// A raster image to be converted into a single PDF page.
    Image(ImageFormat),
}

/// Formats accepted as merge inputs, for use in error messages and help text.
pub const SUPPORTED_INPUT_FORMATS: &str = "PDF, PNG, JPEG, GIF, WebP";

/// Identify a file's type from its leading bytes.
///
/// Returns `None` for content that is neither a PDF nor a supported image.
fn sniff(header: &[u8]) -> Option<InputKind> {
    if header.starts_with(b"%PDF") {
        return Some(InputKind::Pdf);
    }
    if header.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(InputKind::Image(ImageFormat::Png));
    }
    if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(InputKind::Image(ImageFormat::Jpeg));
    }
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return Some(InputKind::Image(ImageFormat::Gif));
    }
    // RIFF container: bytes 0..4 are "RIFF", 8..12 identify the payload.
    if header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP" {
        return Some(InputKind::Image(ImageFormat::WebP));
    }
    None
}

/// Determine whether an input file is a PDF or a supported image.
///
/// Detection is based on the file's contents rather than its extension, so a
/// mislabelled file is still handled correctly. Files that are neither produce
/// [`Error::UnsupportedInput`], which is what turns an unusable input into a
/// visible failure instead of a silently skipped file.
pub fn detect_input_kind(path: &Path) -> Result<InputKind> {
    if !path.exists() {
        return Err(Error::FileNotFound(path.to_path_buf()));
    }

    // Only the header is needed; inputs can be large.
    let mut header = [0u8; 12];
    let mut file = std::fs::File::open(path)?;
    let mut filled = 0;
    while filled < header.len() {
        match file.read(&mut header[filled..])? {
            0 => break,
            n => filled += n,
        }
    }

    sniff(&header[..filled]).ok_or_else(|| Error::UnsupportedInput(path.to_path_buf()))
}

/// Render a raster image into a single-page PDF and return the PDF bytes.
///
/// The page is US Letter, rotated to landscape when the image is wider than it
/// is tall. The image is scaled to fit within the page margins (0.5in at the
/// sides, 1in top and bottom to leave room for headers and footers),
/// preserving its aspect ratio, and centred on the page.
pub fn image_to_pdf_bytes(path: &Path) -> Result<Vec<u8>> {
    use krilla::image::Image;
    use krilla::{Document, PageSettings};
    use tiny_skia_path::{Size, Transform};

    let format = match detect_input_kind(path)? {
        InputKind::Image(format) => format,
        InputKind::Pdf => {
            return Err(Error::General(format!(
                "{} is already a PDF; no image conversion needed",
                path.display()
            )))
        }
    };

    let data = std::fs::read(path)?;

    let image = match format {
        ImageFormat::Png => Image::from_png(&data),
        ImageFormat::Jpeg => Image::from_jpeg(&data),
        ImageFormat::Gif => Image::from_gif(&data),
        ImageFormat::WebP => Image::from_webp(&data),
    }
    .ok_or_else(|| Error::ImageDecode {
        path: path.to_path_buf(),
        format: format.name().to_string(),
    })?;

    let image_size = image.size();
    let (image_width, image_height) = (image_size.width(), image_size.height());

    // Auto-rotate: a landscape image gets a landscape page so it stays large.
    let (page_width, page_height) = if image_width > image_height {
        (LETTER_LONG_EDGE, LETTER_SHORT_EDGE)
    } else {
        (LETTER_SHORT_EDGE, LETTER_LONG_EDGE)
    };

    // Margins follow the header/footer layout frame, not the page. On a
    // landscape page that frame is turned a quarter turn — the title and footer
    // run along the left and right short edges — so the deeper margin goes on
    // the page's horizontal axis instead of its vertical one.
    let landscape = page_width > page_height;
    let (margin_x, margin_y) = if landscape {
        (IMAGE_PAGE_EDGE_MARGIN, IMAGE_PAGE_SIDE_MARGIN)
    } else {
        (IMAGE_PAGE_SIDE_MARGIN, IMAGE_PAGE_EDGE_MARGIN)
    };

    let content_width = page_width - 2.0 * margin_x;
    let content_height = page_height - 2.0 * margin_y;

    // Scale to fit the content box, preserving aspect ratio.
    let scale = (content_width / image_width).min(content_height / image_height);
    let drawn_width = image_width * scale;
    let drawn_height = image_height * scale;

    // Centre within the full page (equal margins on each axis).
    let offset_x = (page_width - drawn_width) / 2.0;
    let offset_y = (page_height - drawn_height) / 2.0;

    let drawn_size =
        Size::from_wh(drawn_width, drawn_height).ok_or_else(|| Error::ImageDecode {
            path: path.to_path_buf(),
            format: format.name().to_string(),
        })?;

    let mut document = Document::new();
    {
        let mut page = document.start_page_with(PageSettings::new(page_width, page_height));
        let mut surface = page.surface();

        // draw_image places the image at the origin, so translate to centre it.
        surface.push_transform(&Transform::from_translate(offset_x, offset_y));
        surface.draw_image(image, drawn_size);
        surface.pop();

        surface.finish();
        page.finish();
    }

    document
        .finish()
        .map_err(|e| Error::General(format!("Failed to generate PDF for image: {:?}", e)))
}

/// Convert a raster image file into a single-page PDF written to `output`.
///
/// See [`image_to_pdf_bytes`] for the page layout rules.
pub fn image_to_pdf(input: &Path, output: &Path) -> Result<()> {
    let pdf = image_to_pdf_bytes(input)?;
    std::fs::write(output, pdf)?;
    Ok(())
}

/// Load an input file as an lopdf `Document`, converting images to PDF first.
///
/// This is the single entry point used by the merge and header/footer paths so
/// that every command accepts the same set of input formats, and so that an
/// unsupported file produces a clear error naming the file.
pub fn load_input_document(path: &Path) -> Result<lopdf::Document> {
    match detect_input_kind(path)? {
        InputKind::Pdf => Ok(lopdf::Document::load(path)?),
        InputKind::Image(_) => {
            let pdf = image_to_pdf_bytes(path)?;
            Ok(lopdf::Document::load_mem(&pdf)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_pdf() {
        assert_eq!(sniff(b"%PDF-1.7\n%\xE2\xE3"), Some(InputKind::Pdf));
    }

    #[test]
    fn sniffs_png() {
        let header = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        assert_eq!(sniff(&header), Some(InputKind::Image(ImageFormat::Png)));
    }

    #[test]
    fn sniffs_jpeg() {
        let header = [0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];
        assert_eq!(sniff(&header), Some(InputKind::Image(ImageFormat::Jpeg)));
    }

    #[test]
    fn sniffs_gif() {
        assert_eq!(
            sniff(b"GIF89a\x01\x00\x01\x00\x00\x00"),
            Some(InputKind::Image(ImageFormat::Gif))
        );
    }

    #[test]
    fn sniffs_webp() {
        assert_eq!(
            sniff(b"RIFF\x24\x00\x00\x00WEBP"),
            Some(InputKind::Image(ImageFormat::WebP))
        );
    }

    #[test]
    fn rejects_unknown_content() {
        assert_eq!(sniff(b"not a document"), None);
        // A RIFF container that is not WebP (e.g. a WAV file) is not an image.
        assert_eq!(sniff(b"RIFF\x24\x00\x00\x00WAVE"), None);
        // Too short to classify.
        assert_eq!(sniff(b"%P"), None);
    }

    #[test]
    fn detect_reports_missing_file() {
        let result = detect_input_kind(Path::new("nonexistent-input.png"));
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }
}
