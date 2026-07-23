//! Debug overlay function to inspect what's happening

use chrono::NaiveDate;
use lopdf::{Document, Object};
use pdf_handouts::pdf::{create_watermark_pdf, merge_pdfs, MergeOptions, WatermarkOptions};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Debug Overlay Investigation ===\n");

    // Helper to get fixture path
    let fixture_path = |name: &str| -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests");
        path.push("fixtures");
        path.push("real-world");
        path.push(name);
        path
    };

    // Step 1: Create a simple merged PDF
    let input_files = vec![
        "1. NT Ladder - Google Docs.pdf",
        "2. NT Ladder Practice Sheet.pdf",
    ];

    let mut input_paths = Vec::new();
    for filename in &input_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Error: {} not found", filename);
            return Ok(());
        }
        input_paths.push(path);
    }

    let merged_path = Path::new("target/debug_merged.pdf");
    let watermark_path = Path::new("target/debug_watermark.pdf");

    println!("Step 1: Merging 2 PDFs...");
    let merge_options = MergeOptions {
        input_paths,
        output_path: merged_path.to_path_buf(),
    };
    merge_pdfs(&merge_options)?;
    println!("  ✓ Merged to {}", merged_path.display());

    println!("\nStep 2: Creating watermark PDF...");
    let watermark_options = WatermarkOptions {
        title: Some("Test Title".to_string()),
        footer_left: Some("Left Footer".to_string()),
        footer_center: Some("Center Footer".to_string()),
        footer_right: None,
        date: Some(NaiveDate::from_ymd_opt(2026, 1, 14).unwrap()),
        show_page_numbers: true,
        show_total_page_count: true,
        page_count: 2,
        title_font_size: 24.0,
        footer_font_size: 14.0,
        ..Default::default()
    };
    create_watermark_pdf(watermark_path, &watermark_options)?;
    println!("  ✓ Created watermark at {}", watermark_path.display());

    println!("\nStep 3: Inspecting watermark PDF structure...");
    let watermark_doc = Document::load(watermark_path)?;

    println!(
        "  Watermark PDF has {} objects",
        watermark_doc.objects.len()
    );
    println!("  Max ID: {}", watermark_doc.max_id);

    let watermark_pages = watermark_doc.get_pages();
    println!("  Pages: {}", watermark_pages.len());

    for (page_num, page_id) in &watermark_pages {
        println!("\n  Page {} (ID: {:?}):", page_num, page_id);

        let page_obj = watermark_doc.get_object(*page_id)?;
        if let Object::Dictionary(ref page_dict) = page_obj {
            // Check for Contents
            if let Ok(contents) = page_dict.get(b"Contents") {
                println!("    Contents: {:?}", contents);

                // If it's a reference, look up the actual content stream
                if let Object::Reference(content_id) = contents {
                    if let Ok(content_obj) = watermark_doc.get_object(*content_id) {
                        match content_obj {
                            Object::Stream(stream) => {
                                println!(
                                    "    Content stream length: {} bytes",
                                    stream.content.len()
                                );
                                println!(
                                    "    First 100 bytes: {:?}",
                                    String::from_utf8_lossy(
                                        &stream.content[..stream.content.len().min(100)]
                                    )
                                );
                            }
                            _ => println!("    Content object is not a stream: {:?}", content_obj),
                        }
                    }
                }
            } else {
                println!("    No Contents field!");
            }

            // Check for Resources
            if let Ok(resources) = page_dict.get(b"Resources") {
                println!("    Resources: {:?}", resources);
            } else {
                println!("    No Resources field!");
            }
        }
    }

    println!("\n\nStep 4: Inspecting merged PDF structure...");
    let merged_doc = Document::load(merged_path)?;

    println!("  Merged PDF has {} objects", merged_doc.objects.len());
    println!("  Max ID: {}", merged_doc.max_id);

    let merged_pages = merged_doc.get_pages();
    println!("  Pages: {}", merged_pages.len());

    for (page_num, page_id) in merged_pages.iter().take(1) {
        println!("\n  Page {} (ID: {:?}):", page_num, page_id);

        let page_obj = merged_doc.get_object(*page_id)?;
        if let Object::Dictionary(ref page_dict) = page_obj {
            if let Ok(contents) = page_dict.get(b"Contents") {
                println!("    Contents: {:?}", contents);
            }
        }
    }

    Ok(())
}
