//! Test decompressing watermark content

use lopdf::{Document, Object};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Decompressing Watermark Content ===\n");

    let watermark_path = Path::new("target/debug_watermark.pdf");
    let mut watermark_doc = Document::load(watermark_path)?;

    // Try to decompress the entire document
    watermark_doc.decompress();

    // Save decompressed version
    let decompressed_path = Path::new("target/debug_watermark_decompressed.pdf");
    watermark_doc.save(decompressed_path)?;
    println!(
        "Saved decompressed watermark to: {}",
        decompressed_path.display()
    );

    // Now inspect the content
    let pages = watermark_doc.get_pages();

    for (page_num, page_id) in &pages {
        println!("\nPage {}:", page_num);

        let page_obj = watermark_doc.get_object(*page_id)?;
        if let Object::Dictionary(ref page_dict) = page_obj {
            if let Ok(Object::Reference(content_id)) = page_dict.get(b"Contents") {
                if let Ok(Object::Stream(stream)) = watermark_doc.get_object(*content_id) {
                    let content_str = String::from_utf8_lossy(&stream.content);
                    println!("  Content stream ({} bytes):", stream.content.len());
                    println!("  {}", "=".repeat(60));
                    println!("{}", content_str);
                    println!("  {}", "=".repeat(60));
                }
            }
        }
    }

    Ok(())
}
