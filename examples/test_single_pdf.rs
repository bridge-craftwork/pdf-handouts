//! Test adding headers/footers to a single PDF (simpler test case)

use chrono::NaiveDate;
use pdf_handouts::pdf::{add_headers_footers, FitMode, HeaderFooterOptions, MaskOptions};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Single PDF ===\n");

    // Helper to get fixture path
    let fixture_path = |name: &str| -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests");
        path.push("fixtures");
        path.push("real-world");
        path.push(name);
        path
    };

    // Use just the first PDF
    let input_path = fixture_path("1. NT Ladder - Google Docs.pdf");
    let output_path = Path::new("target/test_single_output.pdf");

    if !input_path.exists() {
        eprintln!("Error: Input PDF not found: {}", input_path.display());
        return Ok(());
    }

    println!("Input: {}", input_path.display());
    println!("Output: {}", output_path.display());

    let header_footer_options = HeaderFooterOptions {
        title: Some("Test Title".to_string()),
        footer_left: Some("Left Footer".to_string()),
        footer_center: Some("Center Footer".to_string()),
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
    add_headers_footers(&input_path, output_path, &header_footer_options)?;
    println!("✓ Done!");
    println!("\nOpen with: open {}", output_path.display());

    Ok(())
}
