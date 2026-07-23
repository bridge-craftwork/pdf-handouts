//! Trace page tree for Resources

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("Usage: trace_tree <pdf>");
    let doc = Document::load(Path::new(&path))?;

    let pages = doc.get_pages();
    for (page_num, page_id) in pages.iter().take(1) {
        println!("=== Page {} (ID {:?}) ===", page_num, page_id);

        // Trace up the tree
        let mut current_id = Some(*page_id);
        let mut level = 0;

        while let Some(id) = current_id {
            if let Ok(Object::Dictionary(dict)) = doc.get_object(id) {
                let indent = "  ".repeat(level);
                println!("{}Node {:?}:", indent, id);
                println!(
                    "{}  Keys: {:?}",
                    indent,
                    dict.iter()
                        .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                        .collect::<Vec<_>>()
                );

                // Check for Resources
                if let Ok(res) = dict.get(b"Resources") {
                    println!("{}  Has Resources: {:?}", indent, res);
                }

                // Get parent
                current_id = if let Ok(Object::Reference(parent_id)) = dict.get(b"Parent") {
                    Some(*parent_id)
                } else {
                    None
                };
            } else {
                break;
            }
            level += 1;
        }
    }

    Ok(())
}
