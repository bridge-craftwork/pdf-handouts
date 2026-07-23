//! Demonstrate date parsing functionality

use pdf_handouts::date::{format_date, parse_date_expression, resolve_date};

fn main() {
    println!("=== Date Parsing Demo ===\n");

    let test_cases = vec![
        "",
        "today",
        "2024-11-20",
        "11/20/2024",
        "Tuesday",
        "Friday+2",
        "Mon",
    ];

    for expr in test_cases {
        println!("Input: {:?}", expr);

        match parse_date_expression(expr) {
            Ok(date_expr) => {
                println!("  Parsed: {:?}", date_expr);

                if let Some(date) = resolve_date(&date_expr) {
                    let formatted = format_date(&date);
                    println!("  Resolved: {}", formatted);
                } else {
                    println!("  Resolved: (no date)");
                }
            }
            Err(e) => {
                println!("  Error: {}", e);
            }
        }
        println!();
    }
}
