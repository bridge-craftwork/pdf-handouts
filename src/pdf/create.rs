//! PDF creation for headers/footers using krilla

use crate::date::format_date;
use crate::error::{Error, Result};
use chrono::NaiveDate;
use krilla::font::Font;
use std::path::Path;

/// Options for creating a watermark PDF with headers and footers
#[derive(Debug, Clone)]
pub struct WatermarkOptions {
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
    /// Number of pages to generate
    pub page_count: usize,
    /// Font family name (note: must have corresponding font file)
    pub font: String,
    /// Title font size in points
    pub title_font_size: f32,
    /// Footer font size in points
    pub footer_font_size: f32,
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            title: None,
            footer_left: None,
            footer_center: None,
            footer_right: None,
            date: None,
            show_page_numbers: true,
            show_total_page_count: false,
            page_count: 1,
            font: "Times".to_string(),
            title_font_size: 24.0,
            footer_font_size: 16.0,
        }
    }
}

/// Create a watermark PDF with headers and footers
///
/// This generates a multi-page PDF where:
/// - Page 1 has the title (if specified) centered at the top
/// - All pages have footers with three sections (left/center/right)
/// - Footer layout: 25% left, 50% center, 25% right
///
/// The resulting PDF can be overlaid onto another PDF using merge functionality.
///
/// Note: This is a Phase 2 implementation that creates overlay PDFs. It does not
/// scale the source content, so headers/footers may overlap existing content.
pub fn create_watermark_pdf(output: &Path, options: &WatermarkOptions) -> Result<()> {
    use krilla::color::rgb;
    use krilla::path::Fill;
    use krilla::surface::TextDirection;
    use krilla::{Document, PageSettings};
    use tiny_skia_path::Point;

    // Create document
    let mut document = Document::new();

    // US Letter dimensions: 8.5" × 11" = 612pt × 792pt
    let page_width = 612.0;
    let page_height = 792.0;

    // Load font - for now, we'll use a bundled font
    // TODO: Support system fonts or embedded fonts
    let font = load_default_font()?;

    // Generate pages
    for page_num in 1..=options.page_count {
        let mut page = document.start_page_with(PageSettings::new(page_width, page_height));
        let mut surface = page.surface();

        // Add title on first page (centered)
        if page_num == 1 {
            if let Some(ref title) = options.title {
                // Title at top of page - Y=0 is top, increases downward
                let title_y = 50.0; // 50pt from top
                let title_width = measure_text_width(title, options.title_font_size);
                let title_x = (page_width / 2.0) - (title_width / 2.0); // Center the title

                surface.fill_text(
                    Point::from_xy(title_x, title_y),
                    Fill {
                        paint: rgb::Color::new(0, 0, 0).into(),
                        ..Default::default()
                    },
                    font.clone(),
                    options.title_font_size,
                    &[],
                    title,
                    false,
                    TextDirection::Auto,
                );
            }
        }

        // Build footer text
        // Footer at bottom of page - page_height is the bottom
        let footer_y = page_height - 30.0; // 30pt from bottom

        // Footer left (25% width)
        if let Some(ref left_text) = options.footer_left {
            let left_x = 50.0;
            let line_height = options.footer_font_size * 1.2; // 120% line height

            draw_multiline_text(
                &mut surface,
                left_x,
                footer_y,
                left_text,
                font.clone(),
                options.footer_font_size,
                line_height,
            );
        }

        // Footer center (50% width) - only center text, no page numbers or dates
        // Center section occupies 25% to 75% of page width (50% total width)
        // Text should be center-justified within this region
        if let Some(ref center_text) = options.footer_center {
            let lines = parse_multiline_text(center_text);
            let line_height = options.footer_font_size * 1.2; // 120% line height

            // Draw each line centered
            for (i, line) in lines.iter().enumerate() {
                let line_y = footer_y + (i as f32 * line_height);
                let text_width = measure_text_width(line, options.footer_font_size);
                let center_x = (page_width / 2.0) - (text_width / 2.0);

                surface.fill_text(
                    Point::from_xy(center_x, line_y),
                    Fill {
                        paint: rgb::Color::new(0, 0, 0).into(),
                        ..Default::default()
                    },
                    font.clone(),
                    options.footer_font_size,
                    &[],
                    line,
                    false,
                    TextDirection::Auto,
                );
            }
        }

        // Footer right (25% width) - includes page numbers and date
        // Each element on its own line, right-justified
        let mut right_parts = Vec::new();

        if let Some(ref right_text) = options.footer_right {
            right_parts.push(right_text.clone());
        }

        if options.show_page_numbers {
            let page_text = if options.show_total_page_count {
                format!("Page {} of {}", page_num, options.page_count)
            } else {
                format!("Page {}", page_num)
            };
            right_parts.push(page_text);
        }

        if let Some(ref date) = options.date {
            right_parts.push(format_date(date));
        }

        if !right_parts.is_empty() {
            let lines = right_parts;
            let line_height = options.footer_font_size * 1.2; // 120% line height
            let right_edge = page_width - 50.0; // Right edge with margin

            // Draw each line right-justified
            for (i, line) in lines.iter().enumerate() {
                let line_y = footer_y + (i as f32 * line_height);
                let text_width = measure_text_width(line, options.footer_font_size);
                let right_x = right_edge - text_width;

                surface.fill_text(
                    Point::from_xy(right_x, line_y),
                    Fill {
                        paint: rgb::Color::new(0, 0, 0).into(),
                        ..Default::default()
                    },
                    font.clone(),
                    options.footer_font_size,
                    &[],
                    line,
                    false,
                    TextDirection::Auto,
                );
            }
        }

        surface.finish();
        page.finish();
    }

    // Write PDF
    let pdf_data = document
        .finish()
        .map_err(|e| Error::General(format!("Failed to generate PDF: {:?}", e)))?;

    std::fs::write(output, pdf_data)?;

    Ok(())
}

/// Parse text with line break markers and return lines
///
/// Supports multiple line break syntaxes:
/// - `\n` (newline character)
/// - `|` (pipe character)
/// - `[br]` (bracket tag)
/// - `<br>`, `<BR>`, `<br/>`, `<BR/>`, `<br />`, `<BR />` (HTML-style tags for backward compatibility)
fn parse_multiline_text(text: &str) -> Vec<String> {
    // Split on various line break markers
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

/// Draw multi-line text at a given position
///
/// Handles text with line breaks, drawing each line with the specified line height.
fn draw_multiline_text(
    surface: &mut krilla::surface::Surface,
    x: f32,
    y: f32,
    text: &str,
    font: Font,
    font_size: f32,
    line_height: f32,
) {
    use krilla::color::rgb;
    use krilla::path::Fill;
    use krilla::surface::TextDirection;
    use tiny_skia_path::Point;

    let lines = parse_multiline_text(text);

    for (i, line) in lines.iter().enumerate() {
        let line_y = y + (i as f32 * line_height);

        surface.fill_text(
            Point::from_xy(x, line_y),
            Fill {
                paint: rgb::Color::new(0, 0, 0).into(),
                ..Default::default()
            },
            font.clone(),
            font_size,
            &[],
            line,
            false,
            TextDirection::Auto,
        );
    }
}

/// Measure the width of text in points using rustybuzz for accurate text shaping
fn measure_text_width(text: &str, font_size: f32) -> f32 {
    use rustybuzz::{Face, UnicodeBuffer};

    // Use the embedded font data
    const LIBERATION_SERIF: &[u8] =
        include_bytes!("../../assets/fonts/LiberationSerif-Regular.ttf");

    // Parse the font face
    let face = match Face::from_slice(LIBERATION_SERIF, 0) {
        Some(f) => f,
        None => return text.len() as f32 * font_size * 0.5, // Fallback to approximation
    };

    // Create a buffer with the text
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);

    // Shape the text
    let output = rustybuzz::shape(&face, &[], buffer);

    // Calculate total advance width
    let units_per_em = face.units_per_em() as f32;
    let scale = font_size / units_per_em;

    let total_advance: i32 = output
        .glyph_positions()
        .iter()
        .map(|pos| pos.x_advance)
        .sum();

    total_advance as f32 * scale
}

/// Load a default font for PDF generation
///
/// Embeds Liberation Serif (a free/open-source font metrically compatible with Times New Roman).
/// The font is compiled directly into the binary for cross-platform compatibility.
fn load_default_font() -> Result<Font> {
    // Embed Liberation Serif font at compile time
    const LIBERATION_SERIF: &[u8] =
        include_bytes!("../../assets/fonts/LiberationSerif-Regular.ttf");

    use std::sync::Arc;

    // Create font from embedded bytes
    // Parameters: font_data, face_index (0 for single-face fonts), variation_coordinates
    Font::new(Arc::new(LIBERATION_SERIF.to_vec()), 0, vec![])
        .ok_or_else(|| Error::Font("Failed to load embedded font".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tempfile::TempDir;

    #[test]
    fn test_parse_multiline_text() {
        // Single line
        let lines = parse_multiline_text("Simple text");
        assert_eq!(lines, vec!["Simple text"]);

        // Multiple lines with newline
        let lines = parse_multiline_text("Line 1\nLine 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Multiple lines with pipe
        let lines = parse_multiline_text("Line 1|Line 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Multiple lines with [br]
        let lines = parse_multiline_text("Line 1[br]Line 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Multiple lines with [BR]
        let lines = parse_multiline_text("Line 1[BR]Line 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Multiple lines with <br>
        let lines = parse_multiline_text("Line 1<br>Line 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Multiple lines with <BR>
        let lines = parse_multiline_text("Line 1<BR>Line 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Multiple lines with <br/>
        let lines = parse_multiline_text("Line 1<br/>Line 2<br/>Line 3");
        assert_eq!(lines, vec!["Line 1", "Line 2", "Line 3"]);

        // Multiple lines with <br />
        let lines = parse_multiline_text("Line 1<br />Line 2");
        assert_eq!(lines, vec!["Line 1", "Line 2"]);

        // Mixed syntaxes
        let lines = parse_multiline_text("Line 1\nLine 2|Line 3[br]Line 4<br>Line 5");
        assert_eq!(
            lines,
            vec!["Line 1", "Line 2", "Line 3", "Line 4", "Line 5"]
        );
    }

    #[test]
    fn test_create_basic_watermark() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("watermark.pdf");

        let options = WatermarkOptions {
            page_count: 3,
            show_page_numbers: true,
            ..Default::default()
        };

        let result = create_watermark_pdf(&output_path, &options);
        if let Err(ref e) = result {
            eprintln!("Error creating watermark: {:?}", e);
        }
        assert!(result.is_ok());
        assert!(output_path.exists());
    }

    #[test]
    fn test_create_watermark_with_all_options() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("watermark_full.pdf");

        let options = WatermarkOptions {
            title: Some("Test Title".to_string()),
            footer_left: Some("Left Footer".to_string()),
            footer_center: Some("Center Footer".to_string()),
            footer_right: Some("Right Footer".to_string()),
            date: Some(NaiveDate::from_ymd_opt(2024, 11, 20).unwrap()),
            show_page_numbers: true,
            show_total_page_count: true,
            page_count: 5,
            title_font_size: 30.0,
            footer_font_size: 12.0,
            ..Default::default()
        };

        let result = create_watermark_pdf(&output_path, &options);
        if let Err(ref e) = result {
            eprintln!("Error creating watermark: {:?}", e);
        }
        assert!(result.is_ok());
        assert!(output_path.exists());
    }
}
