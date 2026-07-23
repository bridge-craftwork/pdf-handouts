//! Test prepending headers/footers BEFORE the Google Docs content

use lopdf::{Dictionary, Document, Object, Stream};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Prepend Approach ===\n");

    // Load the merged PDF (before headers are added)
    let merged_path = Path::new("target/demo_merged_direct.pdf");
    let mut doc = Document::load(merged_path)?;
    doc.decompress();

    // Create a simple header content stream
    // This will be drawn BEFORE the Google Docs cm transformation
    let header_content = r#"q
BT
/F1 24 Tf
1 0 0 1 200 742 Tm
(PREPENDED TITLE TEST) Tj
ET
BT
/F1 14 Tf
1 0 0 1 50 30 Tm
(Footer Left) Tj
ET
Q
"#;

    // Create Helvetica font
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font));

    // Get first page
    let pages = doc.get_pages();
    let first_page_id = *pages.values().next().unwrap();

    // Add font to page resources
    {
        let page_obj = doc.get_object_mut(first_page_id)?;
        if let Object::Dictionary(ref mut page_dict) = page_obj {
            let mut resources = if let Ok(res) = page_dict.get(b"Resources") {
                res.clone()
            } else {
                Object::Dictionary(Dictionary::new())
            };

            if let Object::Dictionary(ref mut resources_dict) = resources {
                let mut fonts = if let Ok(Object::Dictionary(f)) = resources_dict.get(b"Font") {
                    f.clone()
                } else {
                    Dictionary::new()
                };
                fonts.set("F1", Object::Reference(font_id));
                resources_dict.set("Font", Object::Dictionary(fonts));
                page_dict.set("Resources", resources);
            }
        }
    }

    // Create the content stream
    let header_stream_id = doc.add_object(Stream::new(
        Dictionary::new(),
        header_content.as_bytes().to_vec(),
    ));

    // PREPEND to page contents
    {
        let page_obj = doc.get_object_mut(first_page_id)?;
        if let Object::Dictionary(ref mut page_dict) = page_obj {
            let existing_content = page_dict.get(b"Contents").ok().cloned();

            match existing_content {
                Some(Object::Reference(content_id)) => {
                    // Prepend our content
                    let new_contents = vec![
                        Object::Reference(header_stream_id),
                        Object::Reference(content_id),
                    ];
                    page_dict.set("Contents", Object::Array(new_contents));
                }
                Some(Object::Array(mut content_array)) => {
                    content_array.insert(0, Object::Reference(header_stream_id));
                    page_dict.set("Contents", Object::Array(content_array));
                }
                _ => {
                    page_dict.set(
                        "Contents",
                        Object::Array(vec![Object::Reference(header_stream_id)]),
                    );
                }
            }
        }
    }

    // Save
    doc.save(Path::new("target/test_prepend.pdf"))?;
    println!("Saved to target/test_prepend.pdf");
    println!("Open with: open target/test_prepend.pdf");

    Ok(())
}
