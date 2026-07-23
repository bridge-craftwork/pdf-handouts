//! Demonstrates the full workflow: merge PDFs, create watermark, and overlay

use chrono::NaiveDate;
use pdf_handouts::pdf::{
    count_pages, create_watermark_pdf, merge_pdfs, overlay_watermark, MergeOptions,
    WatermarkOptions,
};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Full Workflow Demo ===\n");

    // Helper to get fixture path
    let fixture_path = |name: &str| -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests");
        path.push("fixtures");
        path.push("real-world");
        path.push(name);
        path
    };

    // Step 1: Collect input PDFs
    let input_files = vec![
        "1. NT Ladder - Google Docs.pdf",
        "2. NT Ladder Practice Sheet.pdf",
        "3. ABS4-2 Jacoby Transfers Handouts.pdf",
        "4. thinking-bridge-Responding to 1NT 1-6.pdf",
    ];

    let mut input_paths = Vec::new();
    for filename in &input_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Error: {} not found", filename);
            eprintln!("Please ensure test fixtures are available.");
            return Ok(());
        }
        input_paths.push(path);
    }

    // Output paths
    let merged_path = Path::new("target/demo_merged.pdf");
    let watermark_path = Path::new("target/demo_watermark.pdf");
    let final_output_path = Path::new("target/demo_final_with_headers.pdf");

    println!("Step 1: Merging {} PDFs...", input_files.len());

    // Step 2: Merge PDFs
    let merge_options = MergeOptions {
        input_paths,
        output_path: merged_path.to_path_buf(),
    };

    merge_pdfs(&merge_options)?;
    let merged_page_count = count_pages(merged_path)?;
    println!("  ✓ Merged {} pages", merged_page_count);
    println!("  → Saved to: {}", merged_path.display());

    println!("\nStep 2: Creating watermark PDF with headers/footers...");

    // Step 3: Create watermark PDF
    let watermark_options = WatermarkOptions {
        title: Some("Bridge Class Handout".to_string()),
        footer_left: Some("Stoneridge Creek|Community Center".to_string()),
        footer_center: Some("Presented by:[br]Rick Wilson".to_string()),
        footer_right: None, // Page numbers and date will appear here
        date: Some(NaiveDate::from_ymd_opt(2026, 1, 14).unwrap()),
        show_page_numbers: true,
        show_total_page_count: true,
        page_count: merged_page_count,
        title_font_size: 24.0,
        footer_font_size: 14.0,
        ..Default::default()
    };

    create_watermark_pdf(watermark_path, &watermark_options)?;
    println!("  ✓ Created watermark with {} pages", merged_page_count);
    println!("  → Saved to: {}", watermark_path.display());

    println!("\nStep 3: Overlaying watermark onto merged PDF...");

    // Step 4: Overlay watermark
    overlay_watermark(merged_path, watermark_path, final_output_path)?;
    let final_page_count = count_pages(final_output_path)?;
    println!(
        "  ✓ Final PDF has {} pages with headers/footers",
        final_page_count
    );
    println!("  → Saved to: {}", final_output_path.display());

    println!("\n=== Demo Complete ===");
    println!("\nGenerated files:");
    println!("  1. Merged (no headers): {}", merged_path.display());
    println!("  2. Watermark only:      {}", watermark_path.display());
    println!("  3. Final with headers:  {}", final_output_path.display());
    println!("\nOpen the final result with:");
    println!("  open {}", final_output_path.display());

    Ok(())
}
