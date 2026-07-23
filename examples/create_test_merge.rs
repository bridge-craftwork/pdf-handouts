//! Create a merged PDF for manual inspection

use pdf_handouts::pdf::{merge_pdfs, MergeOptions};
use std::path::PathBuf;

fn main() {
    let input_files = [
        "tests/fixtures/real-world/1. NT Ladder - Google Docs.pdf",
        "tests/fixtures/real-world/2. NT Ladder Practice Sheet.pdf",
        "tests/fixtures/real-world/3. ABS4-2 Jacoby Transfers Handouts.pdf",
        "tests/fixtures/real-world/4. thinking-bridge-Responding to 1NT 1-6.pdf",
    ];

    let output_path = PathBuf::from("test_merged_output.pdf");

    let options = MergeOptions {
        input_paths: input_files.iter().map(PathBuf::from).collect(),
        output_path: output_path.clone(),
    };

    println!("Merging {} PDFs...", input_files.len());
    for (i, file) in input_files.iter().enumerate() {
        println!("  {}. {}", i + 1, file);
    }

    match merge_pdfs(&options) {
        Ok(_) => {
            println!("\n✓ Successfully created: {}", output_path.display());
            println!("  You can now open this file to inspect the merged result.");
        }
        Err(e) => {
            eprintln!("✗ Failed to merge PDFs: {}", e);
        }
    }
}
