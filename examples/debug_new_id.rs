//! Debug new_object_id behavior

use lopdf::{Dictionary, Document, Object};
use std::collections::BTreeMap;

fn main() {
    let mut doc = Document::with_version("1.5");

    println!("Empty document:");
    println!("  max_id: {}", doc.max_id);
    println!("  objects.len(): {}", doc.objects.len());

    let id1 = doc.new_object_id();
    println!("\nAfter new_object_id():");
    println!("  id: {:?}", id1);
    println!("  max_id: {}", doc.max_id);

    // Simulate adding collected objects
    let mut collected = BTreeMap::new();
    for i in 1..=20 {
        collected.insert((i, 0), Object::Dictionary(Dictionary::new()));
    }

    println!(
        "\nAdding {} collected objects via extend...",
        collected.len()
    );
    doc.objects.extend(collected);
    println!("  max_id: {} (should be 20, but is it?)", doc.max_id);
    println!("  objects.len(): {}", doc.objects.len());

    let id2 = doc.new_object_id();
    println!("\nAfter new_object_id() again:");
    println!("  id: {:?} (should be (21, 0) or higher)", id2);
    println!("  max_id: {}", doc.max_id);

    if doc.objects.contains_key(&id2) {
        println!("  ERROR: ID {:?} already exists!", id2);
    } else {
        println!("  OK: ID {:?} is unique", id2);
    }
}
