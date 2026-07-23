//! Check Resources in detail

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: check_resources <pdf>");
    let doc = Document::load(Path::new(&path))?;

    let pages = doc.get_pages();
    for (page_num, page_id) in pages.iter().take(1) {
        println!("=== Page {} (ID {:?}) ===", page_num, page_id);

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(*page_id) {
            println!(
                "Page dict keys: {:?}",
                page_dict
                    .iter()
                    .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                    .collect::<Vec<_>>()
            );

            // Check for Resources
            if let Ok(resources) = page_dict.get(b"Resources") {
                println!("Resources (direct): {:?}", resources);

                // If it's a reference, dereference it
                if let Object::Reference(res_id) = resources {
                    if let Ok(res_obj) = doc.get_object(*res_id) {
                        println!("Resources (dereferenced {:?}): {:?}", res_id, res_obj);
                    }
                }
            } else {
                println!("No Resources on page - checking parent");

                // Check parent for inherited Resources
                if let Ok(parent) = page_dict.get(b"Parent") {
                    println!("Parent: {:?}", parent);
                    if let Object::Reference(parent_id) = parent {
                        if let Ok(Object::Dictionary(parent_dict)) = doc.get_object(*parent_id) {
                            if let Ok(resources) = parent_dict.get(b"Resources") {
                                println!("Inherited Resources: {:?}", resources);
                                if let Object::Reference(res_id) = resources {
                                    if let Ok(res_obj) = doc.get_object(*res_id) {
                                        println!(
                                            "Inherited Resources (dereferenced {:?}): {:?}",
                                            res_id, res_obj
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
