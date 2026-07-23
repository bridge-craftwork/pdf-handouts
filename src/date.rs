//! Date parsing module
//!
//! Provides flexible date expression parsing compatible with the original PowerShell
//! Get-ReportDate function.

use crate::error::{Error, Result};
use chrono::{Datelike, Local, NaiveDate, Weekday};

/// Date expression types
#[derive(Debug, Clone, PartialEq)]
pub enum DateExpression {
    /// Use today's date
    Today,
    /// Use an explicit date
    Explicit(NaiveDate),
    /// Use next occurrence of a day of week with optional offset
    DayOfWeek { day: Weekday, offset: u32 },
    /// No date (empty/null)
    None,
}

/// Parse a date expression string into a DateExpression
///
/// Supported formats:
/// - `""` (empty) → None
/// - `"today"` → Today
/// - `"2024-11-20"` → Explicit date (ISO format)
/// - `"11/20/2024"` → Explicit date (US format)
/// - `"Tuesday"` → Next Tuesday (or today if today is Tuesday)
/// - `"Tuesday+3"` → 4th upcoming Tuesday (next + 3 weeks)
pub fn parse_date_expression(expr: &str) -> Result<DateExpression> {
    let expr = expr.trim();

    // Empty string → None
    if expr.is_empty() {
        return Ok(DateExpression::None);
    }

    // "today" → Today
    if expr.eq_ignore_ascii_case("today") {
        return Ok(DateExpression::Today);
    }

    // Try parsing as explicit date formats
    // ISO format: 2024-11-20
    if let Ok(date) = NaiveDate::parse_from_str(expr, "%Y-%m-%d") {
        return Ok(DateExpression::Explicit(date));
    }

    // US format: 11/20/2024
    if let Ok(date) = NaiveDate::parse_from_str(expr, "%m/%d/%Y") {
        return Ok(DateExpression::Explicit(date));
    }

    // Check for day of week with optional offset
    // Format: "Tuesday" or "Tuesday+3"
    if let Some((day_str, offset_str)) = expr.split_once('+') {
        // Has offset: "Tuesday+3"
        let day = parse_weekday(day_str.trim())?;
        let offset: u32 = offset_str
            .trim()
            .parse()
            .map_err(|_| Error::InvalidDateExpression(format!("Invalid offset: {}", offset_str)))?;
        return Ok(DateExpression::DayOfWeek { day, offset });
    } else {
        // No offset: just "Tuesday"
        if let Ok(day) = parse_weekday(expr) {
            return Ok(DateExpression::DayOfWeek { day, offset: 0 });
        }
    }

    Err(Error::InvalidDateExpression(format!(
        "Unable to parse date expression: {}",
        expr
    )))
}

/// Parse a weekday name (case-insensitive)
fn parse_weekday(s: &str) -> Result<Weekday> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "monday" | "mon" => Ok(Weekday::Mon),
        "tuesday" | "tue" => Ok(Weekday::Tue),
        "wednesday" | "wed" => Ok(Weekday::Wed),
        "thursday" | "thu" => Ok(Weekday::Thu),
        "friday" | "fri" => Ok(Weekday::Fri),
        "saturday" | "sat" => Ok(Weekday::Sat),
        "sunday" | "sun" => Ok(Weekday::Sun),
        _ => Err(Error::InvalidDateExpression(format!(
            "Unknown weekday: {}",
            s
        ))),
    }
}

/// Resolve a DateExpression to an actual date (if applicable)
pub fn resolve_date(expr: &DateExpression) -> Option<NaiveDate> {
    match expr {
        DateExpression::None => None,
        DateExpression::Today => Some(Local::now().date_naive()),
        DateExpression::Explicit(date) => Some(*date),
        DateExpression::DayOfWeek { day, offset } => Some(find_next_weekday(*day, *offset)),
    }
}

/// Find the next occurrence of a given weekday, with optional offset
///
/// If offset is 0, returns the next occurrence (or today if today is that day)
/// If offset is n, returns the (n+1)th occurrence (next + n more weeks)
fn find_next_weekday(target_day: Weekday, offset: u32) -> NaiveDate {
    let today = Local::now().date_naive();
    let today_weekday = today.weekday();

    // Calculate days until target weekday
    let days_until = if today_weekday == target_day {
        // Today is the target day
        0
    } else {
        // Days until next occurrence
        let today_num = today_weekday.num_days_from_monday();
        let target_num = target_day.num_days_from_monday();

        if target_num > today_num {
            target_num - today_num
        } else {
            7 - (today_num - target_num)
        }
    };

    // Add the base days plus offset weeks
    let total_days = days_until + (offset * 7);
    today + chrono::Duration::days(total_days as i64)
}

/// Format a date in the standard format: "Month day, year"
/// Example: "November 20, 2024"
pub fn format_date(date: &NaiveDate) -> String {
    date.format("%B %-d, %Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let expr = parse_date_expression("").unwrap();
        assert_eq!(expr, DateExpression::None);

        let expr = parse_date_expression("   ").unwrap();
        assert_eq!(expr, DateExpression::None);
    }

    #[test]
    fn test_parse_today() {
        let expr = parse_date_expression("today").unwrap();
        assert_eq!(expr, DateExpression::Today);

        let expr = parse_date_expression("Today").unwrap();
        assert_eq!(expr, DateExpression::Today);

        let expr = parse_date_expression("TODAY").unwrap();
        assert_eq!(expr, DateExpression::Today);
    }

    #[test]
    fn test_parse_iso_date() {
        let expr = parse_date_expression("2024-11-20").unwrap();
        match expr {
            DateExpression::Explicit(date) => {
                assert_eq!(date.year(), 2024);
                assert_eq!(date.month(), 11);
                assert_eq!(date.day(), 20);
            }
            _ => panic!("Expected Explicit date"),
        }
    }

    #[test]
    fn test_parse_us_date() {
        let expr = parse_date_expression("11/20/2024").unwrap();
        match expr {
            DateExpression::Explicit(date) => {
                assert_eq!(date.year(), 2024);
                assert_eq!(date.month(), 11);
                assert_eq!(date.day(), 20);
            }
            _ => panic!("Expected Explicit date"),
        }
    }

    #[test]
    fn test_parse_weekday() {
        let expr = parse_date_expression("Tuesday").unwrap();
        match expr {
            DateExpression::DayOfWeek { day, offset } => {
                assert_eq!(day, Weekday::Tue);
                assert_eq!(offset, 0);
            }
            _ => panic!("Expected DayOfWeek"),
        }

        let expr = parse_date_expression("friday").unwrap();
        match expr {
            DateExpression::DayOfWeek { day, offset } => {
                assert_eq!(day, Weekday::Fri);
                assert_eq!(offset, 0);
            }
            _ => panic!("Expected DayOfWeek"),
        }
    }

    #[test]
    fn test_parse_weekday_with_offset() {
        let expr = parse_date_expression("Tuesday+3").unwrap();
        match expr {
            DateExpression::DayOfWeek { day, offset } => {
                assert_eq!(day, Weekday::Tue);
                assert_eq!(offset, 3);
            }
            _ => panic!("Expected DayOfWeek with offset"),
        }

        let expr = parse_date_expression("Monday + 1").unwrap();
        match expr {
            DateExpression::DayOfWeek { day, offset } => {
                assert_eq!(day, Weekday::Mon);
                assert_eq!(offset, 1);
            }
            _ => panic!("Expected DayOfWeek with offset"),
        }
    }

    #[test]
    fn test_parse_weekday_abbreviations() {
        let test_cases = vec![
            ("Mon", Weekday::Mon),
            ("Tue", Weekday::Tue),
            ("Wed", Weekday::Wed),
            ("Thu", Weekday::Thu),
            ("Fri", Weekday::Fri),
            ("Sat", Weekday::Sat),
            ("Sun", Weekday::Sun),
        ];

        for (input, expected_day) in test_cases {
            let expr = parse_date_expression(input).unwrap();
            match expr {
                DateExpression::DayOfWeek { day, offset } => {
                    assert_eq!(day, expected_day);
                    assert_eq!(offset, 0);
                }
                _ => panic!("Expected DayOfWeek for {}", input),
            }
        }
    }

    #[test]
    fn test_parse_invalid() {
        assert!(parse_date_expression("NotADay").is_err());
        assert!(parse_date_expression("2024-13-01").is_err()); // Invalid month
        assert!(parse_date_expression("Tuesday+abc").is_err()); // Invalid offset
    }

    #[test]
    fn test_resolve_none() {
        let expr = DateExpression::None;
        assert_eq!(resolve_date(&expr), None);
    }

    #[test]
    fn test_resolve_today() {
        let expr = DateExpression::Today;
        let resolved = resolve_date(&expr).unwrap();
        let today = Local::now().date_naive();
        assert_eq!(resolved, today);
    }

    #[test]
    fn test_resolve_explicit() {
        let date = NaiveDate::from_ymd_opt(2024, 11, 20).unwrap();
        let expr = DateExpression::Explicit(date);
        let resolved = resolve_date(&expr).unwrap();
        assert_eq!(resolved, date);
    }

    #[test]
    fn test_resolve_weekday() {
        // Test that we can resolve a weekday (result will depend on current date)
        let expr = DateExpression::DayOfWeek {
            day: Weekday::Tue,
            offset: 0,
        };
        let resolved = resolve_date(&expr).unwrap();

        // The resolved date should be a Tuesday
        assert_eq!(resolved.weekday(), Weekday::Tue);

        // The resolved date should be today or in the future
        let today = Local::now().date_naive();
        assert!(resolved >= today);
    }

    #[test]
    fn test_resolve_weekday_with_offset() {
        let expr = DateExpression::DayOfWeek {
            day: Weekday::Tue,
            offset: 2,
        };
        let resolved = resolve_date(&expr).unwrap();

        // Should be a Tuesday
        assert_eq!(resolved.weekday(), Weekday::Tue);

        // Should be at least 14 days in the future (2 weeks offset)
        let today = Local::now().date_naive();
        let days_diff = (resolved - today).num_days();
        assert!(days_diff >= 14);
    }

    #[test]
    fn test_format_date() {
        let date = NaiveDate::from_ymd_opt(2024, 11, 20).unwrap();
        let formatted = format_date(&date);
        assert_eq!(formatted, "November 20, 2024");

        let date = NaiveDate::from_ymd_opt(2026, 1, 7).unwrap();
        let formatted = format_date(&date);
        assert_eq!(formatted, "January 7, 2026");
    }

    #[test]
    fn test_find_next_weekday_same_day() {
        // If today is Tuesday and we ask for Tuesday, we should get today
        let today = Local::now().date_naive();
        let today_weekday = today.weekday();

        let next = find_next_weekday(today_weekday, 0);
        assert_eq!(next, today);
    }

    #[test]
    fn test_find_next_weekday_with_offset_weeks() {
        let today = Local::now().date_naive();
        let today_weekday = today.weekday();

        // Ask for same weekday with offset 1 (next week)
        let next_week = find_next_weekday(today_weekday, 1);
        let days_diff = (next_week - today).num_days();
        assert_eq!(days_diff, 7);

        // Ask for same weekday with offset 2 (two weeks from now)
        let two_weeks = find_next_weekday(today_weekday, 2);
        let days_diff = (two_weeks - today).num_days();
        assert_eq!(days_diff, 14);
    }
}
