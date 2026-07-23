fn main() {
    // Test options
    let footer_right = Some("Page [page] of [pages]".to_string());
    let title = Some("Glob Test".to_string());

    for page_num in 1..=3 {
        let is_first_page = page_num == 1;
        let total_pages = 14;

        let mut content = String::new();

        // Add title on first page
        if is_first_page {
            if let Some(ref t) = title {
                content.push_str(&format!("TITLE: {}\n", t));
            }
        }

        // Set footer color (should happen for ALL pages)
        content.push_str("FOOTER_COLOR\n");

        // Add footers (should happen for ALL pages)
        if let Some(ref right_text) = footer_right {
            let expanded = right_text
                .replace("[page]", &page_num.to_string())
                .replace("[pages]", &total_pages.to_string());
            content.push_str(&format!("FOOTER_RIGHT: {}\n", expanded));
        }

        println!("=== Page {} ===", page_num);
        println!("Content length: {} bytes", content.len());
        println!("{}", content);
    }
}
