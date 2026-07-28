use pdf_handouts::pdf::{FitMode, HeaderFooterOptions, MaskOptions};

fn main() {
    let options = HeaderFooterOptions {
        title: Some("Glob Test".to_string()),
        footer_left: None,
        footer_center: None,
        footer_right: Some("Page [page] of [pages]".to_string()),
        date: None,
        show_page_numbers: false,
        show_total_page_count: false,
        title_font_size: 24.0,
        footer_font_size: 14.0,
        header_font: None,
        footer_font: None,
        mask: MaskOptions::new(),
        fit: FitMode::Auto,
    };

    // Simulate what should happen for each page
    for page in 1..=3 {
        println!("\n=== Simulating Page {} ===", page);
        println!("title: {:?}", options.title);
        println!("footer_right: {:?}", options.footer_right);
    }
}
