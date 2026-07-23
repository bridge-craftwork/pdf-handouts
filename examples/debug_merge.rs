//! Debug merged PDF structure

use lopdf::Document;
use pdf_handouts::pdf::{merge_pdfs, MergeOptions};
use std::path::{Path, PathBuf};

fn main() {
    let test_files = vec![
        "tests/fixtures/real-world/1. NT Ladder - Google Docs.pdf",
        "tests/fixtures/real-world/2. NT Ladder Practice Sheet.pdf",
    ];

    // Verify input files exist
    for test_file in &test_files {
        let path = Path::new(test_file);
        if !path.exists() {
            eprintln!("File not found: {}", test_file);
            return;
        }
    }

    let output_path = PathBuf::from("debug_merged.pdf");

    let options = MergeOptions {
        input_paths: test_files.iter().map(PathBuf::from).collect(),
        output_path: output_path.clone(),
    };

    println!("Merging PDFs...");
    match merge_pdfs(&options) {
        Ok(_) => println!("Merge successful!"),
        Err(e) => {
            eprintln!("Merge failed: {}", e);
            return;
        }
    }

    // Now try to load and inspect the merged PDF
    println!("\nInspecting merged PDF...");
    match Document::load(&output_path) {
        Ok(doc) => {
            println!("Document loaded successfully");

            // Check trailer
            println!("\nTrailer keys:");
            for (key, _) in doc.trailer.iter() {
                println!("  {}", String::from_utf8_lossy(key));
            }

            // Get Root reference
            if let Ok(root_ref) = doc.trailer.get(b"Root") {
                println!("\nRoot reference: {:?}", root_ref);

                if let lopdf::Object::Reference(catalog_id) = root_ref {
                    if let Ok(catalog_obj) = doc.get_object(*catalog_id) {
                        println!("Catalog object: {:?}", catalog_obj);

                        if let lopdf::Object::Dictionary(catalog_dict) = catalog_obj {
                            println!("\nCatalog keys:");
                            for (key, value) in catalog_dict.iter() {
                                println!("  {}: {:?}", String::from_utf8_lossy(key), value);
                            }

                            // Try to get Pages
                            if let Ok(pages_ref) = catalog_dict.get(b"Pages") {
                                println!("\nPages reference: {:?}", pages_ref);

                                if let lopdf::Object::Reference(pages_id) = pages_ref {
                                    if let Ok(pages_obj) = doc.get_object(*pages_id) {
                                        println!("Pages object: {:?}", pages_obj);
                                    } else {
                                        println!("ERROR: Could not get Pages object");
                                    }
                                }
                            } else {
                                println!("ERROR: No Pages in catalog dictionary");
                            }
                        }
                    } else {
                        println!("ERROR: Could not get catalog object");
                    }
                }
            } else {
                println!("ERROR: No Root in trailer");
            }

            // Try get_pages()
            println!("\nTrying get_pages()...");
            let pages = doc.get_pages();
            println!("get_pages() returned {} pages", pages.len());
        }
        Err(e) => {
            eprintln!("Failed to load merged PDF: {}", e);
        }
    }
}
