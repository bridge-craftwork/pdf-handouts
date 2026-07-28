//! Test adding headers/footers to the already-merged PDF

use chrono::NaiveDate;
use pdf_handouts::pdf::{add_headers_footers, FitMode, HeaderFooterOptions, MaskOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing with Merged PDF ===\n");

    let input_path = Path::new("target/demo_merged_direct.pdf");
    let output_path = Path::new("target/test_merged_output.pdf");

    if !input_path.exists() {
        eprintln!("Error: Merged PDF not found. Run demo_direct_headers first.");
        return Ok(());
    }

    println!("Input: {}", input_path.display());
    println!("Output: {}", output_path.display());

    let header_footer_options = HeaderFooterOptions {
        title: Some("Bridge Class Handout".to_string()),
        footer_left: Some("Stoneridge Creek|Community Center".to_string()),
        footer_center: Some("Presented by:[br]Rick Wilson".to_string()),
        footer_right: None,
        date: Some(NaiveDate::from_ymd_opt(2026, 1, 14).unwrap()),
        show_page_numbers: true,
        show_total_page_count: true,
        title_font_size: 24.0,
        footer_font_size: 14.0,
        header_font: None,
        footer_font: None,
        mask: MaskOptions::new(),
        fit: FitMode::Auto,
    };

    println!("\nAdding headers/footers...");
    add_headers_footers(input_path, output_path, &header_footer_options)?;
    println!("✓ Done!");
    println!("\nOpen with: open {}", output_path.display());

    Ok(())
}
