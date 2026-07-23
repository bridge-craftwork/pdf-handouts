use lopdf::Document;

fn main() {
    let doc = Document::load("/tmp/merged-only.pdf").expect("Failed to load");

    let pages: Vec<(usize, lopdf::ObjectId)> = doc
        .get_pages()
        .iter()
        .enumerate()
        .map(|(i, (_num, id))| (i, *id))
        .collect();

    println!("Pages vector:");
    for (i, page_id) in pages.iter() {
        println!("  i={}, page_id={:?}", i, page_id);
    }

    println!("\nIteration with iter():");
    for item in pages.iter() {
        println!("  item={:?}", item);
    }
}
