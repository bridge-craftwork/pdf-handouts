//! Demo of direct header/footer writing (new approach)

use chrono::NaiveDate;
use pdf_handouts::pdf::{
    add_headers_footers, merge_pdfs, FontSpec, HeaderFooterOptions, MaskOptions, MergeOptions,
};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Direct Header/Footer Demo ===\n");

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
    let merged_path = Path::new("target/demo_merged_direct.pdf");
    let final_output_path = Path::new("target/demo_final_direct.pdf");

    println!("Step 1: Merging {} PDFs...", input_files.len());

    // Step 2: Merge PDFs
    let merge_options = MergeOptions {
        input_paths,
        output_path: merged_path.to_path_buf(),
    };

    merge_pdfs(&merge_options)?;
    println!("  ✓ Merged to {}", merged_path.display());

    println!("\nStep 2: Adding headers/footers directly to merged PDF...");

    // Step 3: Add headers/footers directly
    // Using the new placeholder and font tag syntax:
    // - [page] = current page number
    // - [pages] = total page count
    // - [date] = formatted date
    // - | or [br] = line break
    // - [font italic]...[/font] = italic text
    // - [font bold]...[/font] = bold text
    //
    // FontSpec allows specifying: bold, italic, size, family, and color
    // Format: "[bold] [italic] [size[pt]] [family_name] [#rrggbb]"
    // Example: "bold 24pt Liberation_Serif #333333"
    let header_footer_options = HeaderFooterOptions {
        title: Some("Bridge Class Handout".to_string()),
        footer_left: Some("Stoneridge Creek|[font italic]Community Center[/font]".to_string()),
        footer_center: Some("Presented by:|Rick Wilson".to_string()),
        footer_right: Some("Page [page] of [pages]|[date]".to_string()),
        date: Some(NaiveDate::from_ymd_opt(2026, 1, 14).unwrap()),
        show_page_numbers: false,     // Using [page] placeholder instead
        show_total_page_count: false, // Using [pages] placeholder instead
        title_font_size: 24.0,
        footer_font_size: 14.0,
        // Use FontSpec to set header color to dark gray
        header_font: Some(FontSpec::parse("24pt #333333")),
        // Use FontSpec to set footer color to a slightly lighter gray
        footer_font: Some(FontSpec::parse("14pt #555555")),
        mask: MaskOptions::new(),
    };

    add_headers_footers(merged_path, final_output_path, &header_footer_options)?;
    println!("  ✓ Added headers/footers");
    println!("  → Saved to: {}", final_output_path.display());

    println!("\n=== Demo Complete ===");
    println!("\nGenerated files:");
    println!("  1. Merged (no headers): {}", merged_path.display());
    println!("  2. Final with headers:  {}", final_output_path.display());
    println!("\nOpen the final result with:");
    println!("  open {}", final_output_path.display());

    Ok(())
}
