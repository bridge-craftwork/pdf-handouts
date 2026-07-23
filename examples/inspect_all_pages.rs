//! Inspect all pages to understand their transformation matrices

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Inspecting All Pages ===\n");

    let merged_path = Path::new("target/demo_merged_direct.pdf");
    let mut doc = Document::load(merged_path)?;
    doc.decompress();

    let pages = doc.get_pages();
    println!("Total pages: {}\n", pages.len());

    for (page_num, page_id) in pages.iter() {
        println!("--- Page {} (ID {:?}) ---", page_num, page_id);

        let page_obj = doc.get_object(*page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            if let Ok(contents) = page_dict.get(b"Contents") {
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

                for content_id in content_ids.iter().take(1) {
                    // Just first content stream
                    if let Ok(Object::Stream(stream)) = doc.get_object(*content_id) {
                        let content_str = String::from_utf8_lossy(&stream.content);
                        // Get first 200 chars to see the transformation
                        let preview: String = content_str.chars().take(200).collect();
                        println!("First content stream start:\n{}", preview);

                        // Check for cm transformation
                        if let Some(cm_pos) = content_str.find(" cm") {
                            let start = cm_pos.saturating_sub(50);
                            let snippet: String = content_str[start..cm_pos + 3].chars().collect();
                            println!("\nTransformation found: ...{}", snippet);
                        }
                    }
                }
            }
        }
        println!();
    }

    Ok(())
}
