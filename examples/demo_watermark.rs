//! Demonstrate watermark PDF creation

use chrono::NaiveDate;
use pdf_handouts::pdf::create::{create_watermark_pdf, WatermarkOptions};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Watermark PDF Creation Demo ===\n");

    // Create a simple watermark PDF
    let output = PathBuf::from("target/demo_watermark.pdf");

    let options = WatermarkOptions {
        title: Some("Bridge Class Handout".to_string()),
        footer_left: Some("Stoneridge Creek".to_string()),
        footer_center: Some("Presented by: Rick Wilson".to_string()),
        footer_right: None,
        date: Some(NaiveDate::from_ymd_opt(2026, 1, 14).unwrap()),
        show_page_numbers: true,
        show_total_page_count: true,
        page_count: 3,
        title_font_size: 24.0,
        footer_font_size: 14.0,
        ..Default::default()
    };

    create_watermark_pdf(&output, &options)?;

    println!("Created watermark PDF: {}", output.display());
    println!("\nOpen it with:");
    println!("  open {}", output.display());

    Ok(())
}
