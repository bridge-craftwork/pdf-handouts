use lopdf::{Document, Object};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: inspect_resources <pdf_file>");
        return;
    }

    let doc = Document::load(&args[1]).expect("Failed to load PDF");
    let pages = doc.get_pages();

    println!("Total pages: {}", pages.len());

    for (page_num, page_id) in pages {
        println!("\n=== Page {} (ID: {:?}) ===", page_num, page_id);

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(page_id) {
            // Check Resources
            if let Ok(resources) = page_dict.get(b"Resources") {
                match resources {
                    Object::Reference(id) => {
                        println!("Resources: reference to {:?}", id);
                        if let Ok(Object::Dictionary(res_dict)) = doc.get_object(*id) {
                            print_resources(res_dict);
                        }
                    }
                    Object::Dictionary(res_dict) => {
                        println!("Resources: inline dictionary");
                        print_resources(res_dict);
                    }
                    _ => println!("Resources: {:?}", resources),
                }
            } else {
                println!("No direct Resources");
            }
        }
    }
}

fn print_resources(res_dict: &lopdf::Dictionary) {
    for (key, val) in res_dict.iter() {
        let key_str = String::from_utf8_lossy(key);
        match val {
            Object::Dictionary(d) => {
                println!("  {}: dict with {} entries", key_str, d.len());
                for (k, _v) in d.iter() {
                    println!("    - {}", String::from_utf8_lossy(k));
                }
            }
            Object::Reference(id) => println!("  {}: ref {:?}", key_str, id),
            _ => println!("  {}: {:?}", key_str, val),
        }
    }
}
