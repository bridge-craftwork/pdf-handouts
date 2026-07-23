//! Detailed debug of the overlay process

use lopdf::{Document, Object, ObjectId};
use std::collections::HashMap;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Detailed Overlay Debug ===\n");

    let source_path = Path::new("target/debug_merged.pdf");
    let watermark_path = Path::new("target/debug_watermark.pdf");
    let output_path = Path::new("target/debug_final.pdf");

    // Load both documents
    let mut source_doc = Document::load(source_path)?;
    let watermark_doc = Document::load(watermark_path)?;

    println!(
        "Source doc: {} objects, max_id: {}",
        source_doc.objects.len(),
        source_doc.max_id
    );
    println!(
        "Watermark doc: {} objects, max_id: {}",
        watermark_doc.objects.len(),
        watermark_doc.max_id
    );

    let source_pages = source_doc.get_pages();
    let watermark_pages = watermark_doc.get_pages();

    println!("\nSource pages: {}", source_pages.len());
    println!("Watermark pages: {}", watermark_pages.len());

    // Import all objects from watermark
    let watermark_max_id = watermark_doc.max_id;
    let source_max_id = source_doc.max_id;
    let id_offset = source_max_id + 1;

    println!("\nID offset: {}", id_offset);

    // Build complete ID map first
    let mut id_map: HashMap<ObjectId, ObjectId> = HashMap::new();
    for old_id in watermark_doc.objects.keys() {
        let new_id = (old_id.0 + id_offset, old_id.1);
        id_map.insert(*old_id, new_id);
    }

    println!("Built ID map with {} entries", id_map.len());

    // Copy all objects
    for (old_id, object) in watermark_doc.objects.iter() {
        let new_id = id_map[old_id];
        let new_object = renumber_object_references(object, &id_map);
        source_doc.objects.insert(new_id, new_object);
    }

    println!("Copied {} objects from watermark to source", id_map.len());
    println!("Source doc now has {} objects", source_doc.objects.len());

    // Update max_id
    source_doc.max_id = watermark_max_id + id_offset;
    println!("Updated source max_id to {}", source_doc.max_id);

    // Now overlay the content
    println!("\n=== Overlaying content streams ===");

    for (i, (_page_num, source_page_id)) in source_pages.iter().enumerate() {
        let watermark_page_num = (i + 1) as u32;

        println!(
            "\nProcessing page {} (source page ID: {:?})",
            watermark_page_num, source_page_id
        );

        // Get watermark content refs
        if let Some(watermark_content_refs) =
            get_page_content_refs(&watermark_doc, watermark_page_num, &id_map)?
        {
            println!("  Watermark content refs: {:?}", watermark_content_refs);

            // Get source page
            let page_obj = source_doc.get_object_mut(*source_page_id)?;

            if let Object::Dictionary(ref mut page_dict) = page_obj {
                let existing_content = page_dict.get(b"Contents").ok().cloned();
                println!("  Existing content: {:?}", existing_content);

                match existing_content {
                    Some(Object::Reference(content_id)) => {
                        let mut new_content = vec![Object::Reference(content_id)];
                        new_content.extend(watermark_content_refs.clone());
                        println!("  Setting new content array: {:?}", new_content);
                        page_dict.set("Contents", Object::Array(new_content));
                    }
                    Some(Object::Array(mut content_array)) => {
                        content_array.extend(watermark_content_refs.clone());
                        println!("  Extending content array to: {:?}", content_array);
                        page_dict.set("Contents", Object::Array(content_array));
                    }
                    _ => {
                        println!("  No existing content, using watermark content only");
                        page_dict.set("Contents", Object::Array(watermark_content_refs));
                    }
                }

                // Verify it was set
                let updated_content = page_dict.get(b"Contents").ok();
                println!("  Updated content: {:?}", updated_content);
            }
        } else {
            println!(
                "  WARNING: No watermark content found for page {}",
                watermark_page_num
            );
        }
    }

    // Save
    println!("\n=== Saving ===");
    source_doc.save(output_path)?;
    println!("Saved to {}", output_path.display());

    // Verify the saved document
    println!("\n=== Verifying saved document ===");
    let final_doc = Document::load(output_path)?;
    let final_pages = final_doc.get_pages();

    for (page_num, page_id) in final_pages.iter().take(1) {
        println!("Page {} (ID: {:?}):", page_num, page_id);
        let page_obj = final_doc.get_object(*page_id)?;
        if let Object::Dictionary(ref page_dict) = page_obj {
            if let Ok(contents) = page_dict.get(b"Contents") {
                println!("  Contents: {:?}", contents);
            }
        }
    }

    Ok(())
}

fn renumber_object_references(object: &Object, id_map: &HashMap<ObjectId, ObjectId>) -> Object {
    match object {
        Object::Reference(old_id) => {
            if let Some(new_id) = id_map.get(old_id) {
                Object::Reference(*new_id)
            } else {
                Object::Reference(*old_id)
            }
        }
        Object::Array(arr) => Object::Array(
            arr.iter()
                .map(|obj| renumber_object_references(obj, id_map))
                .collect(),
        ),
        Object::Dictionary(dict) => {
            let mut new_dict = lopdf::Dictionary::new();
            for (key, value) in dict.iter() {
                new_dict.set(key.clone(), renumber_object_references(value, id_map));
            }
            Object::Dictionary(new_dict)
        }
        Object::Stream(stream) => {
            let mut new_dict = lopdf::Dictionary::new();
            for (key, value) in stream.dict.iter() {
                new_dict.set(key.clone(), renumber_object_references(value, id_map));
            }
            Object::Stream(lopdf::Stream {
                dict: new_dict,
                content: stream.content.clone(),
                allows_compression: stream.allows_compression,
                start_position: stream.start_position,
            })
        }
        _ => object.clone(),
    }
}

fn get_page_content_refs(
    doc: &Document,
    page_num: u32,
    id_map: &HashMap<ObjectId, ObjectId>,
) -> Result<Option<Vec<Object>>, Box<dyn std::error::Error>> {
    let pages = doc.get_pages();

    for (pg_num, page_id) in pages {
        if pg_num == page_num {
            let page_obj = doc.get_object(page_id)?;

            if let Object::Dictionary(ref page_dict) = page_obj {
                if let Ok(content) = page_dict.get(b"Contents") {
                    let remapped_content = renumber_object_references(content, id_map);

                    let content_refs = match remapped_content {
                        Object::Reference(id) => vec![Object::Reference(id)],
                        Object::Array(arr) => arr,
                        _ => vec![remapped_content],
                    };

                    return Ok(Some(content_refs));
                }
            }
        }
    }

    Ok(None)
}
