//! Debug object collection during merge

use lopdf::Document;
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let files = [
        "tests/fixtures/real-world/1. NT Ladder - Google Docs.pdf",
        "tests/fixtures/real-world/2. NT Ladder Practice Sheet.pdf",
    ];

    let mut max_id = 1;
    let mut all_objects = BTreeMap::new();
    let mut all_page_ids = Vec::new();

    for (i, file) in files.iter().enumerate() {
        let path = Path::new(file);
        let mut doc = Document::load(path).expect("Failed to load");

        println!("\n=== Doc {} ({}) ===", i + 1, file);
        println!("Before renumber: {} objects", doc.objects.len());

        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let pages = doc.get_pages();
        let page_ids: Vec<_> = pages.values().copied().collect();
        println!("Page IDs after renumber: {:?}", page_ids);

        // Check if page objects exist
        for page_id in &page_ids {
            if let Some(obj) = doc.objects.get(page_id) {
                println!(
                    "  Page {:?} exists in doc.objects: {:?}",
                    page_id,
                    matches!(obj, lopdf::Object::Dictionary(_))
                );
            } else {
                println!("  Page {:?} NOT FOUND in doc.objects!", page_id);
            }
        }

        println!("Collecting {} objects...", doc.objects.len());
        all_objects.extend(doc.objects);
        all_page_ids.extend(page_ids);
    }

    println!("\n=== After collection ===");
    println!("Total objects collected: {}", all_objects.len());
    println!("Total page IDs: {}", all_page_ids.len());

    println!("\n=== Checking page objects in collected set ===");
    for (i, page_id) in all_page_ids.iter().enumerate() {
        if let Some(_obj) = all_objects.get(page_id) {
            println!("[{}] Page {:?} EXISTS in collected objects", i, page_id);
        } else {
            println!("[{}] Page {:?} NOT FOUND in collected objects!", i, page_id);
        }
    }
}
