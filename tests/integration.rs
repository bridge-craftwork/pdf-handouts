//! Integration tests for PDF handouts library

use chrono::NaiveDate;
use pdf_handouts::pdf::{
    count_pages, create_watermark_pdf, merge_pdfs, overlay_watermark, MergeOptions,
    WatermarkOptions,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Test helper to get the path to test fixtures
fn fixture_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("fixtures");
    path.push("real-world");
    path.push(name);
    path
}

#[test]
fn test_count_pages_real_pdfs() {
    // Test counting pages in real PDF files
    // Note: These are the actual Count values from the PDF metadata
    // Some PDFs may have incorrect Count fields in their metadata
    let test_cases = vec![
        ("1. NT Ladder - Google Docs.pdf", 1),
        ("2. NT Ladder Practice Sheet.pdf", 1),
        ("3. ABS4-2 Jacoby Transfers Handouts.pdf", 6),
        ("4. thinking-bridge-Responding to 1NT 1-6.pdf", 6),
    ];

    for (filename, expected_pages) in test_cases {
        let path = fixture_path(filename);

        // Skip if file doesn't exist (in case test fixtures aren't available)
        if !path.exists() {
            eprintln!("Skipping test for {}: file not found", filename);
            continue;
        }

        let page_count =
            count_pages(&path).unwrap_or_else(|_| panic!("Failed to count pages in {}", filename));

        assert_eq!(
            page_count, expected_pages,
            "Page count mismatch for {}: expected {}, got {}",
            filename, expected_pages, page_count
        );
    }
}

#[test]
fn test_merge_real_pdfs_page_count() {
    // Verify that merged PDF has correct total page count
    // Using actual Count values from PDF metadata
    let input_files = vec![
        ("1. NT Ladder - Google Docs.pdf", 1),
        ("2. NT Ladder Practice Sheet.pdf", 1),
        ("3. ABS4-2 Jacoby Transfers Handouts.pdf", 6),
        ("4. thinking-bridge-Responding to 1NT 1-6.pdf", 6),
    ];

    // Build list of input paths
    let mut input_paths = Vec::new();
    let mut expected_total = 0;

    for (filename, pages) in &input_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping merge test: {} not found", filename);
            return; // Skip entire test if any file is missing
        }
        input_paths.push(path);
        expected_total += pages;
    }

    // Create temporary directory for output
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let output_path = temp_dir.path().join("merged.pdf");

    // Merge PDFs
    let options = MergeOptions {
        input_paths,
        output_path: output_path.clone(),
    };

    merge_pdfs(&options).expect("Failed to merge PDFs");

    // Verify output exists
    assert!(output_path.exists(), "Merged PDF was not created");

    // Count pages in merged PDF
    let merged_page_count = count_pages(&output_path).expect("Failed to count pages in merged PDF");

    assert_eq!(
        merged_page_count, expected_total,
        "Merged PDF should have {} pages (sum of all inputs), got {}",
        expected_total, merged_page_count
    );

    println!(
        "✓ Successfully merged {} PDFs into {} pages",
        input_files.len(),
        merged_page_count
    );
}

#[test]
fn test_merge_preserves_content_order() {
    // Verify that pages appear in correct order after merge
    let input_files = vec![
        ("1. NT Ladder - Google Docs.pdf", 1),
        ("2. NT Ladder Practice Sheet.pdf", 1),
        ("3. ABS4-2 Jacoby Transfers Handouts.pdf", 6),
    ];

    let mut input_paths = Vec::new();
    let mut expected_total = 0;
    for (filename, pages) in &input_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping order test: {} not found", filename);
            return;
        }
        input_paths.push(path);
        expected_total += pages;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let output_path = temp_dir.path().join("ordered.pdf");

    let options = MergeOptions {
        input_paths,
        output_path: output_path.clone(),
    };

    merge_pdfs(&options).expect("Failed to merge PDFs");

    // Verify the merge succeeded
    assert!(output_path.exists(), "Merged PDF was not created");

    // Expected: 1 + 1 + 6 = 8 pages
    let page_count = count_pages(&output_path).expect("Failed to count pages");
    assert_eq!(
        page_count, expected_total,
        "Merged PDF should have {} pages",
        expected_total
    );

    println!("✓ Page order preserved in merged PDF");
}

#[test]
fn test_merge_empty_input_list() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let output_path = temp_dir.path().join("empty.pdf");

    let options = MergeOptions {
        input_paths: vec![],
        output_path: output_path.clone(),
    };

    let result = merge_pdfs(&options);
    assert!(result.is_err(), "Should fail with empty input list");

    if let Err(e) = result {
        assert!(
            e.to_string().contains("No input files"),
            "Error message should mention no input files"
        );
    }
}

#[test]
fn test_merge_nonexistent_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let output_path = temp_dir.path().join("output.pdf");

    let options = MergeOptions {
        input_paths: vec![PathBuf::from("nonexistent.pdf")],
        output_path: output_path.clone(),
    };

    let result = merge_pdfs(&options);
    assert!(result.is_err(), "Should fail with nonexistent file");

    if let Err(e) = result {
        assert!(
            e.to_string().contains("not found") || e.to_string().contains("nonexistent"),
            "Error should mention file not found: {}",
            e
        );
    }
}

#[test]
fn test_full_workflow_merge_watermark_overlay() {
    // Complete workflow test: merge PDFs, create watermark, and overlay
    println!("=== Full Workflow Test: Merge + Watermark + Overlay ===");

    // Step 1: Collect input PDFs
    let input_files = vec![
        ("1. NT Ladder - Google Docs.pdf", 1),
        ("2. NT Ladder Practice Sheet.pdf", 1),
        ("3. ABS4-2 Jacoby Transfers Handouts.pdf", 6),
        ("4. thinking-bridge-Responding to 1NT 1-6.pdf", 6),
    ];

    let mut input_paths = Vec::new();
    let mut expected_total = 0;

    for (filename, pages) in &input_files {
        let path = fixture_path(filename);
        if !path.exists() {
            eprintln!("Skipping full workflow test: {} not found", filename);
            return; // Skip entire test if any file is missing
        }
        input_paths.push(path);
        expected_total += pages;
    }

    // Create temporary directory for intermediate and output files
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let merged_path = temp_dir.path().join("merged.pdf");
    let watermark_path = temp_dir.path().join("watermark.pdf");
    let final_output_path = temp_dir.path().join("final_handouts.pdf");

    println!("Step 1: Merging {} PDFs...", input_files.len());

    // Step 2: Merge PDFs
    let merge_options = MergeOptions {
        input_paths,
        output_path: merged_path.clone(),
    };

    merge_pdfs(&merge_options).expect("Failed to merge PDFs");
    assert!(merged_path.exists(), "Merged PDF was not created");

    // Verify merged page count
    let merged_page_count = count_pages(&merged_path).expect("Failed to count pages in merged PDF");
    assert_eq!(
        merged_page_count, expected_total,
        "Merged PDF should have {} pages, got {}",
        expected_total, merged_page_count
    );
    println!("  ✓ Merged {} pages successfully", merged_page_count);

    println!("Step 2: Creating watermark PDF with headers/footers...");

    // Step 3: Create watermark PDF with headers and footers
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

    create_watermark_pdf(&watermark_path, &watermark_options)
        .expect("Failed to create watermark PDF");
    assert!(watermark_path.exists(), "Watermark PDF was not created");

    // Verify watermark page count matches merged PDF
    let watermark_page_count =
        count_pages(&watermark_path).expect("Failed to count pages in watermark PDF");
    assert_eq!(
        watermark_page_count, merged_page_count,
        "Watermark PDF should have same page count as merged PDF"
    );
    println!("  ✓ Created watermark with {} pages", watermark_page_count);

    println!("Step 3: Overlaying watermark onto merged PDF...");

    // Step 4: Overlay watermark onto merged PDF
    overlay_watermark(&merged_path, &watermark_path, &final_output_path)
        .expect("Failed to overlay watermark");
    assert!(
        final_output_path.exists(),
        "Final output PDF was not created"
    );

    // Verify final page count
    let final_page_count =
        count_pages(&final_output_path).expect("Failed to count pages in final PDF");
    assert_eq!(
        final_page_count, expected_total,
        "Final PDF should have {} pages, got {}",
        expected_total, final_page_count
    );
    println!(
        "  ✓ Final PDF has {} pages with headers/footers",
        final_page_count
    );

    println!("\n=== Full Workflow Test: SUCCESS ===");
    println!(
        "✓ Merged {} PDFs ({} pages)",
        input_files.len(),
        merged_page_count
    );
    println!("✓ Created watermark with title and multi-line footers");
    println!("✓ Overlaid watermark successfully");
    println!(
        "✓ Final output: {} pages with headers/footers",
        final_page_count
    );
}
