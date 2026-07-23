//! Test inherited resources

use lopdf::{Dictionary, Document, Object};
use std::path::Path;

fn get_inherited_resources(doc: &Document, parent_id: (u32, u16)) -> Dictionary {
    if let Ok(Object::Dictionary(parent_dict)) = doc.get_object(parent_id) {
        // Check for Resources on this parent
        if let Ok(res) = parent_dict.get(b"Resources") {
            match res {
                Object::Dictionary(dict) => return dict.clone(),
                Object::Reference(res_id) => {
                    if let Ok(Object::Dictionary(dict)) = doc.get_object(*res_id) {
                        return dict.clone();
                    }
                }
                _ => {}
            }
        }

        // Not found here, check grandparent
        if let Ok(Object::Reference(grandparent_id)) = parent_dict.get(b"Parent") {
            return get_inherited_resources(doc, *grandparent_id);
        }
    }

    Dictionary::new()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("Usage: test_inherit <pdf>");
    let doc = Document::load(Path::new(&path))?;

    let pages = doc.get_pages();
    for (page_num, page_id) in pages.iter().take(1) {
        println!("=== Page {} ===", page_num);

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(*page_id) {
            // Check direct Resources
            if let Ok(res) = page_dict.get(b"Resources") {
                println!("Has direct Resources: {:?}", res);
            } else {
                println!("No direct Resources");

                // Try inherited
                if let Ok(Object::Reference(parent_id)) = page_dict.get(b"Parent") {
                    let inherited = get_inherited_resources(&doc, *parent_id);
                    println!(
                        "Inherited Resources keys: {:?}",
                        inherited
                            .iter()
                            .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                            .collect::<Vec<_>>()
                    );

                    // Check for Font
                    if let Ok(font) = inherited.get(b"Font") {
                        println!("Fonts: {:?}", font);
                    }
                }
            }
        }
    }

    Ok(())
}
