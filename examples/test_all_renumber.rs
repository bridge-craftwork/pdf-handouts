//! Test renumbering all 4 PDFs

use lopdf::Document;
use std::path::Path;

fn main() {
    let files = [
        "tests/fixtures/real-world/1. NT Ladder - Google Docs.pdf",
        "tests/fixtures/real-world/2. NT Ladder Practice Sheet.pdf",
        "tests/fixtures/real-world/3. ABS4-2 Jacoby Transfers Handouts.pdf",
        "tests/fixtures/real-world/4. thinking-bridge-Responding to 1NT 1-6.pdf",
    ];

    let mut all_page_ids = Vec::new();
    let mut max_id = 1;

    for (i, file) in files.iter().enumerate() {
        let path = Path::new(file);
        if !path.exists() {
            eprintln!("File not found: {}", file);
            continue;
        }

        let mut doc = Document::load(path).expect("Failed to load");
        println!("\n=== Doc {} ({}) ===", i + 1, file);
        println!("Before renumber:");
        println!("  max_id: {}", doc.max_id);
        let pages_before = doc.get_pages();
        println!("  pages: {:?}", pages_before.values().collect::<Vec<_>>());

        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        println!(
            "After renumber (offset {}):",
            max_id - 1 - doc.max_id + doc.max_id
        );
        println!("  max_id: {}", doc.max_id);
        let pages_after = doc.get_pages();
        let page_ids: Vec<_> = pages_after.values().copied().collect();
        println!("  pages: {:?}", page_ids);

        all_page_ids.extend(page_ids);
    }

    println!("\n=== All collected page IDs ===");
    for (i, id) in all_page_ids.iter().enumerate() {
        println!("[{}] {:?}", i, id);
    }
    println!("Total: {} page references", all_page_ids.len());
}
