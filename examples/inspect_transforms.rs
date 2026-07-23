//! Inspect detected transformations for all pages

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Detected Transformations ===\n");

    let final_path = Path::new("target/demo_final_direct.pdf");
    let mut doc = Document::load(final_path)?;
    doc.decompress();

    let pages = doc.get_pages();
    println!("Total pages: {}\n", pages.len());

    for (page_num, page_id) in pages.iter() {
        // Find the HeaderFooter XObject for this page
        let page_obj = doc.get_object(*page_id)?;
        if let Object::Dictionary(page_dict) = page_obj {
            if let Ok(Object::Dictionary(resources)) = page_dict.get(b"Resources") {
                if let Ok(Object::Dictionary(xobjects)) = resources.get(b"XObject") {
                    if let Ok(Object::Reference(hf_id)) = xobjects.get(b"HeaderFooter") {
                        if let Ok(Object::Stream(stream)) = doc.get_object(*hf_id) {
                            // Get the Matrix from the XObject
                            if let Ok(Object::Array(matrix)) = stream.dict.get(b"Matrix") {
                                let nums: Vec<String> =
                                    matrix.iter().map(|o| format!("{:?}", o)).collect();
                                println!("Page {}: Matrix = [{}]", page_num, nums.join(", "));
                            } else {
                                println!("Page {}: No Matrix (identity)", page_num);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
