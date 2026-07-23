//! Debug q/Q imbalance detection

use lopdf::{Document, Object};
use std::path::Path;

fn count_graphics_state_imbalance(content: &[u8]) -> i32 {
    let content_str = String::from_utf8_lossy(content);
    let mut depth: i32 = 0;
    let bytes = content_str.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        let c = bytes[i];
        if c == b'q' || c == b'Q' {
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let next_ok = i + 1 >= len || !bytes[i + 1].is_ascii_alphanumeric();
            if prev_ok && next_ok {
                if c == b'q' {
                    depth += 1;
                } else {
                    depth -= 1;
                }
            }
        }
    }
    depth
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: debug_imbalance <pdf>");
    let mut doc = Document::load(Path::new(&path))?;

    println!("Before decompress:");
    analyze(&doc);

    doc.decompress();

    println!("\nAfter decompress:");
    analyze(&doc);

    Ok(())
}

fn analyze(doc: &Document) {
    let pages = doc.get_pages();
    for (page_num, page_id) in pages.iter().take(1) {
        println!("  Page {}", page_num);
        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(*page_id) {
            if let Ok(contents) = page_dict.get(b"Contents") {
                let content_ids: Vec<_> = match contents {
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

                for content_id in content_ids {
                    if let Ok(Object::Stream(stream)) = doc.get_object(content_id) {
                        let imbalance = count_graphics_state_imbalance(&stream.content);
                        println!(
                            "    Stream {:?}: {} bytes, imbalance: {}",
                            content_id,
                            stream.content.len(),
                            imbalance
                        );

                        // Show first 200 chars
                        let preview = String::from_utf8_lossy(&stream.content);
                        let preview: String = preview.chars().take(200).collect();
                        println!("    Preview: {:?}", preview);
                    }
                }
            }
        }
    }
}
