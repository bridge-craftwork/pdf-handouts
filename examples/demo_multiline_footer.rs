//! Demonstrates multi-line footer support with various line break syntaxes

use chrono::NaiveDate;
use pdf_handouts::pdf::{create_watermark_pdf, WatermarkOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-line Footer Demo ===\n");

    let output = Path::new("target/demo_multiline_footer.pdf");

    // Demonstrates different line break syntaxes:
    // - footer_left uses pipe separator |
    // - footer_center uses bracket tag [br]
    // - footer_right shows automatic page numbers and dates
    //   Note: Page numbers and dates are placed in the right section automatically,
    //   and shown on separate lines when both are present
    let options = WatermarkOptions {
        title: Some("Bridge Class Handout".to_string()),
        footer_left: Some("Stoneridge Creek|Community Center".to_string()),
        footer_center: Some("Presented by:[br]Rick Wilson".to_string()),
        footer_right: None, // Page numbers and date will appear here automatically
        date: Some(NaiveDate::from_ymd_opt(2026, 1, 14).unwrap()),
        show_page_numbers: true,
        show_total_page_count: true,
        page_count: 3,
        title_font_size: 24.0,
        footer_font_size: 14.0,
        ..Default::default()
    };

    create_watermark_pdf(output, &options)?;

    println!("Created multi-line footer PDF: {}", output.display());
    println!("\nSupported line break syntaxes:");
    println!("  - \\n (newline character)");
    println!("  - | (pipe character)");
    println!("  - [br] (bracket tag)");
    println!("  - <br>, <BR>, <br/>, etc. (HTML-style tags)");
    println!("\nFooter layout:");
    println!("  - Left: Multi-line text (left-aligned)");
    println!("  - Center: Multi-line text (center-aligned)");
    println!("  - Right: Page numbers and date (each on separate lines, right-aligned)");
    println!("\nOpen it with:");
    println!("  open {}", output.display());

    Ok(())
}
