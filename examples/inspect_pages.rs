use lopdf::{Document, Object};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: inspect_pages <pdf_file>");
        return;
    }

    let doc = Document::load(&args[1]).expect("Failed to load PDF");
    let pages = doc.get_pages();

    println!("Total pages: {}", pages.len());

    for (page_num, page_id) in pages {
        println!("\n=== Page {} (ID: {:?}) ===", page_num, page_id);

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(page_id) {
            // Check Contents
            if let Ok(contents) = page_dict.get(b"Contents") {
                match contents {
                    Object::Reference(id) => println!("Contents: single stream {:?}", id),
                    Object::Array(arr) => {
                        println!("Contents: array with {} items", arr.len());
                        for (i, item) in arr.iter().enumerate() {
                            if let Object::Reference(id) = item {
                                if let Ok(Object::Stream(stream)) = doc.get_object(*id) {
                                    let preview: String = String::from_utf8_lossy(&stream.content)
                                        .chars()
                                        .take(100)
                                        .collect();
                                    println!("  [{}] {:?}: {}...", i, id, preview);
                                }
                            }
                        }
                    }
                    _ => println!("Contents: {:?}", contents),
                }
            }

            // Check Resources
            if let Ok(resources) = page_dict.get(b"Resources") {
                match resources {
                    Object::Reference(id) => {
                        println!("Resources: reference to {:?}", id);
                        if let Ok(Object::Dictionary(res_dict)) = doc.get_object(*id) {
                            if let Ok(xobjects) = res_dict.get(b"XObject") {
                                println!("  XObject: {:?}", xobjects);
                            }
                        }
                    }
                    Object::Dictionary(res_dict) => {
                        println!("Resources: inline dictionary");
                        if let Ok(xobjects) = res_dict.get(b"XObject") {
                            println!("  XObject: {:?}", xobjects);
                        }
                    }
                    _ => println!("Resources: {:?}", resources),
                }
            } else {
                println!("No direct Resources (may be inherited)");
            }
        }
    }
}
