//! Compare old working PDF structure with our output

use lopdf::{Document, Object};
use std::env;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .expect("Usage: compare_old_new <pdf_path>");
    println!("=== Inspecting: {} ===\n", path);

    let mut doc = Document::load(Path::new(&path))?;
    doc.decompress();

    println!("Objects: {}, Max ID: {}", doc.objects.len(), doc.max_id);

    let pages = doc.get_pages();
    println!("Pages: {}\n", pages.len());

    // Inspect first page
    for (page_num, page_id) in pages.iter().take(2) {
        println!("=== Page {} (ID {:?}) ===", page_num, page_id);

        let page_obj = doc.get_object(*page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            // MediaBox
            if let Ok(mediabox) = page_dict.get(b"MediaBox") {
                println!("MediaBox: {:?}", mediabox);
            }

            // Contents structure
            if let Ok(contents) = page_dict.get(b"Contents") {
                match contents {
                    Object::Reference(id) => {
                        println!("Contents: single stream {:?}", id);
                        show_stream(&doc, *id, 1500)?;
                    }
                    Object::Array(arr) => {
                        println!("Contents: {} streams", arr.len());
                        for (i, obj) in arr.iter().enumerate() {
                            if let Object::Reference(id) = obj {
                                println!("\n--- Stream {} ({:?}) ---", i + 1, id);
                                show_stream(
                                    &doc,
                                    *id,
                                    if i == 0 || i == arr.len() - 1 {
                                        1000
                                    } else {
                                        200
                                    },
                                )?;
                            }
                        }
                    }
                    _ => println!("Contents: other"),
                }
            }

            // Resources
            println!("\n--- Resources ---");
            if let Ok(resources) = page_dict.get(b"Resources") {
                show_resources(&doc, resources)?;
            }
        }
        println!("\n");
    }

    Ok(())
}

fn show_stream(
    doc: &Document,
    id: lopdf::ObjectId,
    max_chars: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(Object::Stream(stream)) = doc.get_object(id) {
        let content = String::from_utf8_lossy(&stream.content);
        let display = if content.len() > max_chars {
            format!("{}...[truncated]", &content[..max_chars])
        } else {
            content.to_string()
        };
        println!("{}", display);
    }
    Ok(())
}

fn show_resources(doc: &Document, resources: &Object) -> Result<(), Box<dyn std::error::Error>> {
    let res_dict = match resources {
        Object::Reference(id) => {
            println!("Resources ref: {:?}", id);
            if let Ok(Object::Dictionary(d)) = doc.get_object(*id) {
                d.clone()
            } else {
                return Ok(());
            }
        }
        Object::Dictionary(d) => d.clone(),
        _ => return Ok(()),
    };

    // Fonts
    if let Ok(fonts) = res_dict.get(b"Font") {
        println!("Fonts: {:?}", fonts);
    }

    // XObjects
    if let Ok(xobjects) = res_dict.get(b"XObject") {
        println!("XObjects: {:?}", xobjects);
    }

    Ok(())
}
