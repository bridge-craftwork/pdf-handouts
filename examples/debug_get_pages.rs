//! Debug get_pages() behavior

use lopdf::Document;
use std::path::Path;

fn main() {
    let test_files = vec![
        "tests/fixtures/real-world/1. NT Ladder - Google Docs.pdf",
        "tests/fixtures/real-world/2. NT Ladder Practice Sheet.pdf",
    ];

    for test_file in test_files {
        let path = Path::new(test_file);
        if !path.exists() {
            eprintln!("File not found: {}", test_file);
            continue;
        }

        println!("\n=== {} ===", test_file);
        match Document::load(path) {
            Ok(doc) => {
                let pages = doc.get_pages();
                println!("get_pages() returned {} entries", pages.len());
                for (i, (page_num, page_id)) in pages.iter().enumerate() {
                    println!("  [{}] page_num={}, page_id={:?}", i, page_num, page_id);

                    // Try to get the object
                    match doc.get_object(*page_id) {
                        Ok(obj) => {
                            if let lopdf::Object::Dictionary(dict) = obj {
                                let obj_type = if let Ok(t) = dict.get(b"Type") {
                                    if let Ok(name) = t.as_name() {
                                        String::from_utf8_lossy(name).to_string()
                                    } else {
                                        "No Type".to_string()
                                    }
                                } else {
                                    "No Type".to_string()
                                };
                                println!("       Type: {}", obj_type);
                            }
                        }
                        Err(e) => {
                            println!("       ERROR: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to load: {}", e);
            }
        }
    }
}
