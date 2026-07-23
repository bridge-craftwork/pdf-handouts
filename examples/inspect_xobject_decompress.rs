use lopdf::{Document, Object};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: inspect_xobject_decompress <pdf_file>");
        return;
    }

    let mut doc = Document::load(&args[1]).expect("Failed to load PDF");
    doc.decompress();

    let pages = doc.get_pages();

    for (page_num, page_id) in pages {
        println!("\n=== Page {} ===", page_num);

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(page_id) {
            if let Ok(resources) = page_dict.get(b"Resources") {
                let res_dict = match resources {
                    Object::Reference(id) => {
                        if let Ok(Object::Dictionary(d)) = doc.get_object(*id) {
                            d.clone()
                        } else {
                            continue;
                        }
                    }
                    Object::Dictionary(d) => d.clone(),
                    _ => continue,
                };

                if let Ok(Object::Dictionary(xobjects)) = res_dict.get(b"XObject") {
                    if let Ok(Object::Reference(hf_id)) = xobjects.get(b"HeaderFooter") {
                        println!("HeaderFooter XObject ID: {:?}", hf_id);
                        if let Ok(Object::Stream(stream)) = doc.get_object(*hf_id) {
                            let content = String::from_utf8_lossy(&stream.content);
                            println!("Content length: {} bytes", content.len());
                            // Show first 500 chars
                            println!("{}", content.chars().take(500).collect::<String>());
                        }
                    }
                }
            }
        }
    }
}
