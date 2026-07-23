//! Inspect the direct headers PDF to see what's wrong

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Inspecting Direct Headers PDF ===\n");

    let final_path = Path::new("target/demo_final_direct.pdf");
    let mut doc = Document::load(final_path)?;

    println!(
        "PDF has {} objects, max_id: {}",
        doc.objects.len(),
        doc.max_id
    );

    // Decompress for easier reading
    doc.decompress();

    let pages = doc.get_pages();
    println!("\nPages: {}", pages.len());

    // Look at first page
    if let Some(page_id) = pages.values().next() {
        println!("\nFirst page ID: {:?}", page_id);

        let page_obj = doc.get_object(*page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            // Check Contents
            if let Ok(contents) = page_dict.get(b"Contents") {
                println!("\nContents: {:?}", contents);

                match contents {
                    Object::Reference(content_id) => {
                        println!("\nSingle content stream:");
                        if let Ok(Object::Stream(stream)) = doc.get_object(*content_id) {
                            let content_str = String::from_utf8_lossy(&stream.content);
                            println!("{}", content_str);
                        }
                    }
                    Object::Array(arr) => {
                        println!("\nMultiple content streams: {} streams", arr.len());
                        for (i, obj) in arr.iter().enumerate() {
                            if let Object::Reference(content_id) = obj {
                                println!("\n--- Stream {} (ID {:?}) ---", i + 1, content_id);
                                if let Ok(Object::Stream(stream)) = doc.get_object(*content_id) {
                                    let content_str = String::from_utf8_lossy(&stream.content);
                                    println!("{}", content_str);
                                }
                            }
                        }
                    }
                    _ => println!("Unexpected Contents format"),
                }
            }

            // Check Resources
            if let Ok(resources) = page_dict.get(b"Resources") {
                println!("\n\nResources: {:?}", resources);

                if let Object::Dictionary(resources_dict) = resources {
                    if let Ok(fonts) = resources_dict.get(b"Font") {
                        println!("\nFonts: {:?}", fonts);
                    } else {
                        println!("\n❌ No Font entry in Resources!");
                    }
                }
            } else {
                println!("\n❌ No Resources!");
            }
        }
    }

    Ok(())
}
