//! Inspect XObject overlays in old PDF

use lopdf::{Document, Object};
use std::env;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .expect("Usage: inspect_xobject_detail <pdf_path>");
    println!("=== Inspecting XObjects in: {} ===\n", path);

    let mut doc = Document::load(Path::new(&path))?;
    doc.decompress();

    // Look for Xi0, Xi1, etc. XObjects
    for (id, obj) in &doc.objects {
        if let Object::Stream(stream) = obj {
            if let Ok(Object::Name(name)) = stream.dict.get(b"Subtype") {
                if name == b"Form" {
                    println!("=== Form XObject {:?} ===", id);

                    // Show BBox
                    if let Ok(bbox) = stream.dict.get(b"BBox") {
                        println!("BBox: {:?}", bbox);
                    }

                    // Show Matrix if present
                    if let Ok(matrix) = stream.dict.get(b"Matrix") {
                        println!("Matrix: {:?}", matrix);
                    }

                    // Show Resources
                    if let Ok(resources) = stream.dict.get(b"Resources") {
                        println!("Resources: {:?}", resources);
                    }

                    // Show content
                    let content = String::from_utf8_lossy(&stream.content);
                    if content.len() > 3000 {
                        println!(
                            "Content ({} bytes):\n{}...[truncated]",
                            content.len(),
                            &content[..3000]
                        );
                    } else {
                        println!("Content ({} bytes):\n{}", content.len(), content);
                    }
                    println!("\n");
                }
            }
        }
    }

    Ok(())
}
