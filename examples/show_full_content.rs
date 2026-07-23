//! Show full decompressed content

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: show_full_content <pdf>");
    let mut doc = Document::load(Path::new(&path))?;
    doc.decompress();

    let pages = doc.get_pages();
    for (page_num, page_id) in pages.iter().take(1) {
        println!("=== Page {} ===", page_num);
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
                        let content = String::from_utf8_lossy(&stream.content);
                        println!("{}", content);
                    }
                }
            }
        }
    }

    Ok(())
}
