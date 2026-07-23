//! Test renumbering behavior

use lopdf::Document;
use std::path::Path;

fn main() {
    let path1 = Path::new("tests/fixtures/real-world/1. NT Ladder - Google Docs.pdf");
    let path2 = Path::new("tests/fixtures/real-world/2. NT Ladder Practice Sheet.pdf");

    if !path1.exists() || !path2.exists() {
        eprintln!("Test files not found");
        return;
    }

    println!("=== Document 1 (before renumbering) ===");
    let mut doc1 = Document::load(path1).expect("Failed to load doc1");
    let pages1_before = doc1.get_pages();
    println!("Pages: {:?}", pages1_before);
    println!("max_id: {}", doc1.max_id);

    println!("\n=== Document 1 (after renumbering with offset 1) ===");
    doc1.renumber_objects_with(1);
    let pages1_after = doc1.get_pages();
    println!("Pages: {:?}", pages1_after);
    println!("max_id: {}", doc1.max_id);

    println!("\n=== Document 2 (before renumbering) ===");
    let mut doc2 = Document::load(path2).expect("Failed to load doc2");
    let pages2_before = doc2.get_pages();
    println!("Pages: {:?}", pages2_before);
    println!("max_id: {}", doc2.max_id);

    let max_id_after_doc1 = doc1.max_id + 1;
    println!(
        "\n=== Document 2 (after renumbering with offset {}) ===",
        max_id_after_doc1
    );
    doc2.renumber_objects_with(max_id_after_doc1);
    let pages2_after = doc2.get_pages();
    println!("Pages: {:?}", pages2_after);
    println!("max_id: {}", doc2.max_id);

    println!("\n=== Summary ===");
    println!(
        "Doc1 pages: {:?}",
        pages1_after.values().collect::<Vec<_>>()
    );
    println!(
        "Doc2 pages: {:?}",
        pages2_after.values().collect::<Vec<_>>()
    );

    let all_page_ids: Vec<_> = pages1_after
        .iter()
        .chain(pages2_after.iter())
        .map(|(_, id)| id)
        .collect();
    println!("All page IDs: {:?}", all_page_ids);
}
