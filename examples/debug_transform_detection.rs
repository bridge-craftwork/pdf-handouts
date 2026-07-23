//! Debug transformation detection

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Debug Transform Detection ===\n");

    let merged_path = Path::new("target/demo_merged_direct.pdf");
    let mut doc = Document::load(merged_path)?;
    doc.decompress();

    let pages = doc.get_pages();

    for (page_num, page_id) in pages.iter() {
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

                if let Some(content_id) = content_ids.first() {
                    if let Ok(Object::Stream(stream)) = doc.get_object(*content_id) {
                        let content_str = String::from_utf8_lossy(&stream.content);

                        // Find first cm
                        if let Some(cm_pos) = content_str.find(" cm") {
                            let before_cm = &content_str[..cm_pos];
                            let parts: Vec<&str> = before_cm.split_whitespace().collect();

                            if parts.len() >= 6 {
                                let start = parts.len() - 6;
                                let matrix_parts: Vec<&str> = parts[start..].to_vec();
                                println!(
                                    "Page {}: First cm = [{} {} {} {} {} {}]",
                                    page_num,
                                    matrix_parts[0],
                                    matrix_parts[1],
                                    matrix_parts[2],
                                    matrix_parts[3],
                                    matrix_parts[4],
                                    matrix_parts[5]
                                );
                            } else {
                                println!(
                                    "Page {}: cm found but not enough numbers before it",
                                    page_num
                                );
                                println!("  Parts: {:?}", parts);
                            }
                        } else {
                            println!("Page {}: No cm found in first content stream", page_num);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
