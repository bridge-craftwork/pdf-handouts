//! Validate PDF structure

use lopdf::Document;
use std::path::Path;

fn main() {
    let path = Path::new("test_merged_output.pdf");

    if !path.exists() {
        eprintln!("File not found: test_merged_output.pdf");
        return;
    }

    println!("Loading PDF...");
    match Document::load(path) {
        Ok(doc) => {
            println!("✓ PDF loads successfully with lopdf");

            // Check trailer
            println!("\n=== Trailer ===");
            if let Ok(root_ref) = doc.trailer.get(b"Root") {
                println!("Root: {:?}", root_ref);
            } else {
                println!("ERROR: No Root in trailer!");
            }

            // Check catalog
            if let Ok(lopdf::Object::Reference(catalog_id)) = doc.trailer.get(b"Root") {
                if let Ok(catalog_obj) = doc.get_object(*catalog_id) {
                    println!("\n=== Catalog (Object {:?}) ===", catalog_id);
                    if let lopdf::Object::Dictionary(catalog_dict) = catalog_obj {
                        for (key, value) in catalog_dict.iter() {
                            println!("{}: {:?}", String::from_utf8_lossy(key), value);
                        }

                        // Check Pages
                        if let Ok(pages_ref) = catalog_dict.get(b"Pages") {
                            println!("\n=== Pages Reference ===");
                            println!("Pages: {:?}", pages_ref);

                            if let lopdf::Object::Reference(pages_id) = pages_ref {
                                if let Ok(pages_obj) = doc.get_object(*pages_id) {
                                    println!("\n=== Pages Object (Object {:?}) ===", pages_id);
                                    if let lopdf::Object::Dictionary(pages_dict) = pages_obj {
                                        for (key, value) in pages_dict.iter() {
                                            let val_str = match key.as_slice() {
                                                b"Kids" => format!("{:?}", value),
                                                _ => format!("{:?}", value),
                                            };
                                            println!(
                                                "{}: {}",
                                                String::from_utf8_lossy(key),
                                                val_str
                                            );
                                        }

                                        // Check Kids array
                                        if let Ok(lopdf::Object::Array(kids)) =
                                            pages_dict.get(b"Kids")
                                        {
                                            println!("\n=== Page Kids (Page References) ===");
                                            println!("Number of page references: {}", kids.len());
                                            for (i, kid) in kids.iter().enumerate() {
                                                if let lopdf::Object::Reference(page_id) = kid {
                                                    // Check if page exists
                                                    match doc.get_object(*page_id) {
                                                        Ok(page_obj) => {
                                                            if let lopdf::Object::Dictionary(
                                                                page_dict,
                                                            ) = page_obj
                                                            {
                                                                let page_type = if let Ok(t) =
                                                                    page_dict.get(b"Type")
                                                                {
                                                                    if let Ok(name) = t.as_name() {
                                                                        String::from_utf8_lossy(
                                                                            name,
                                                                        )
                                                                        .to_string()
                                                                    } else {
                                                                        "Unknown".to_string()
                                                                    }
                                                                } else {
                                                                    "Unknown".to_string()
                                                                };
                                                                println!(
                                                                    "  [{}] {:?} -> Type: {}",
                                                                    i, page_id, page_type
                                                                );
                                                            } else {
                                                                println!("  [{}] {:?} -> ERROR: Not a dictionary!", i, page_id);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            println!("  [{}] {:?} -> ERROR: Cannot get object: {}", i, page_id, e);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    println!("ERROR: Cannot get Pages object!");
                                }
                            }
                        }
                    }
                }
            }

            println!("\n=== get_pages() Result ===");
            let pages = doc.get_pages();
            println!("get_pages() returned {} pages", pages.len());
            if pages.is_empty() {
                println!("WARNING: get_pages() returned empty!");
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to load PDF: {}", e);
        }
    }
}
