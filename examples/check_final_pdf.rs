//! Check what's actually in the final PDF

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Checking Final PDF ===\n");

    let final_path = Path::new("target/demo_final_direct.pdf");
    let mut doc = Document::load(final_path)?;
    doc.decompress();

    let pages = doc.get_pages();

    for (page_num, page_id) in pages.iter() {
        println!("--- Page {} ---", page_num);

        let page_obj = doc.get_object(*page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            // Check Contents
            if let Ok(contents) = page_dict.get(b"Contents") {
                let content_count = match contents {
                    Object::Reference(_) => 1,
                    Object::Array(arr) => arr.len(),
                    _ => 0,
                };
                println!("  Contents: {} stream(s)", content_count);

                // Check if last content stream has HeaderFooter Do
                let content_ids: Vec<(u32, u16)> = match contents {
                    Object::Reference(id) => vec![*id],
                    Object::Array(arr) => arr
                        .iter()
                        .filter_map(|o| {
                            if let Object::Reference(id) = o {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .collect(),
                    _ => vec![],
                };

                if let Some(last_id) = content_ids.last() {
                    if let Ok(Object::Stream(stream)) = doc.get_object(*last_id) {
                        let content_str = String::from_utf8_lossy(&stream.content);
                        if content_str.contains("HeaderFooter") {
                            println!("  Last stream: Has HeaderFooter Do ✓");
                        } else {
                            println!(
                                "  Last stream: NO HeaderFooter (first 100 chars: {})",
                                &content_str.chars().take(100).collect::<String>()
                            );
                        }
                    }
                }
            }

            // Check Resources for HeaderFooter XObject
            if let Ok(resources) = page_dict.get(b"Resources") {
                if let Object::Dictionary(res_dict) = resources {
                    if let Ok(xobjects) = res_dict.get(b"XObject") {
                        if let Object::Dictionary(xobj_dict) = xobjects {
                            if xobj_dict.get(b"HeaderFooter").is_ok() {
                                println!("  Resources: Has HeaderFooter XObject ✓");

                                // Check the matrix
                                if let Ok(Object::Reference(hf_id)) = xobj_dict.get(b"HeaderFooter")
                                {
                                    if let Ok(Object::Stream(stream)) = doc.get_object(*hf_id) {
                                        if let Ok(matrix) = stream.dict.get(b"Matrix") {
                                            println!("  XObject Matrix: {:?}", matrix);
                                        }
                                    }
                                }
                            } else {
                                println!("  Resources: NO HeaderFooter XObject!");
                            }
                        }
                    } else {
                        println!("  Resources: No XObject dict");
                    }
                } else if let Object::Reference(res_id) = resources {
                    println!("  Resources: Reference to {:?} (inherited)", res_id);
                }
            } else {
                println!("  Resources: None (inherited)");
            }
        }
        println!();
    }

    Ok(())
}
