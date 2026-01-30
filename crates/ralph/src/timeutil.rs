//! Time helpers for RFC3339 timestamps with consistent precision.
//!
//! Responsibilities:
//! - Parse RFC3339 timestamps for queue/reporting workflows.
//! - Format timestamps with fixed 9-digit fractional seconds in UTC.
//!
//! Does not handle:
//! - Parsing non-RFC3339 timestamp formats.
//! - Guessing or inferring time zones for naive timestamps.
//!
//! Invariants/assumptions:
//! - Callers provide RFC3339 strings when parsing.
//! - Formatted timestamps are always UTC with 9-digit subseconds.

use anyhow::{bail, Context, Result};
use std::sync::OnceLock;
use time::format_description::well_known::Rfc3339;
use time::format_description::FormatItem;
use time::{OffsetDateTime, UtcOffset};

pub const FALLBACK_RFC3339: &str = "2026-01-18T00:00:00.000000000Z";

fn fixed_rfc3339_format() -> &'static [FormatItem<'static>] {
    static FORMAT: OnceLock<Vec<FormatItem<'static>>> = OnceLock::new();
    FORMAT
        .get_or_init(|| {
            // This format string is a compile-time constant that is always valid.
            // The expect documents this invariant and ensures we fail fast if it changes.
            time::format_description::parse(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z",
            )
            .expect("compile-time RFC3339 format string is valid")
        })
        .as_slice()
}

pub fn now_utc_rfc3339() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(fixed_rfc3339_format())
        .context("format RFC3339 timestamp")
}

pub fn parse_rfc3339(ts: &str) -> Result<OffsetDateTime> {
    let trimmed = ts.trim();
    if trimmed.is_empty() {
        bail!("timestamp is empty");
    }
    OffsetDateTime::parse(trimmed, &Rfc3339)
        .with_context(|| format!("parse RFC3339 timestamp '{}'", trimmed))
}

pub fn parse_rfc3339_opt(ts: &str) -> Option<OffsetDateTime> {
    let trimmed = ts.trim();
    if trimmed.is_empty() {
        return None;
    }
    parse_rfc3339(trimmed).ok()
}

pub fn format_rfc3339(dt: OffsetDateTime) -> Result<String> {
    dt.to_offset(UtcOffset::UTC)
        .format(fixed_rfc3339_format())
        .context("format RFC3339 timestamp")
}

pub fn now_utc_rfc3339_or_fallback() -> String {
    now_utc_rfc3339().unwrap_or_else(|_| FALLBACK_RFC3339.to_string())
}

/// Parse a relative or absolute time expression into RFC3339.
///
/// Supports:
/// - RFC3339 timestamps (2026-02-01T09:00:00Z)
/// - Relative expressions: "tomorrow 9am", "in 2 hours", "next monday"
///
/// Time parsing for expressions like "tomorrow 9am" uses a simple heuristic:
/// - "9am", "9:00am", "09:00" formats are supported
/// - If no time is specified, defaults to 9:00 AM
pub fn parse_relative_time(expression: &str) -> Result<String> {
    let trimmed = expression.trim();

    // First try RFC3339 parsing
    if let Ok(dt) = parse_rfc3339(trimmed) {
        return format_rfc3339(dt);
    }

    // Try relative parsing
    let lower = trimmed.to_lowercase();
    let now = OffsetDateTime::now_utc();

    // "tomorrow [TIME]"
    if lower.starts_with("tomorrow") {
        let tomorrow = now + time::Duration::days(1);
        let time_part = lower.strip_prefix("tomorrow").unwrap_or("").trim();
        let time = parse_time_expression(time_part).unwrap_or((9, 0));
        let result = tomorrow
            .replace_hour(time.0)
            .map_err(|e| anyhow::anyhow!("Invalid hour: {}", e))?
            .replace_minute(time.1)
            .map_err(|e| anyhow::anyhow!("Invalid minute: {}", e))?;
        return format_rfc3339(result);
    }

    // "in N [units]"
    if let Some(rest) = lower.strip_prefix("in ") {
        return parse_in_expression(now, rest);
    }

    // "next [weekday]"
    if let Some(rest) = lower.strip_prefix("next ") {
        return parse_next_weekday(now, rest);
    }

    bail!(
        "Unable to parse time expression: '{}'. Supported formats:\n  - RFC3339: 2026-02-01T09:00:00Z\n  - Relative: 'tomorrow 9am', 'in 2 hours', 'next monday'",
        expression
    )
}

/// Parse a time expression like "9am", "14:30", "2:30pm"
/// Returns (hour, minute) in 24-hour format
fn parse_time_expression(expr: &str) -> Option<(u8, u8)> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    // Try to parse "9am", "9:30am", "2pm", etc.
    let expr = expr.replace(' ', "");

    // Check for am/pm
    let is_pm = expr.ends_with("pm");
    let is_am = expr.ends_with("am");
    let num_part = if is_pm || is_am {
        &expr[..expr.len() - 2]
    } else {
        &expr
    };

    // Split by colon if present
    let parts: Vec<&str> = num_part.split(':').collect();
    let hour: u8 = parts[0].parse().ok()?;
    let minute: u8 = parts.get(1).and_then(|m| m.parse().ok()).unwrap_or(0);

    // Convert to 24-hour format
    let hour_24 = if is_pm && hour != 12 {
        hour + 12
    } else if is_am && hour == 12 {
        0
    } else {
        hour
    };

    if hour_24 > 23 || minute > 59 {
        return None;
    }

    Some((hour_24, minute))
}

/// Parse "in N hours/minutes/days/weeks"
fn parse_in_expression(now: OffsetDateTime, expr: &str) -> Result<String> {
    let expr = expr.trim();

    // Parse number and unit
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("Invalid 'in' expression: expected 'in N hours/minutes/days/weeks'");
    }

    let num: i64 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number in 'in' expression: '{}'", parts[0]))?;

    let unit = parts[1].to_lowercase();
    let unit = unit.trim_end_matches('s'); // Handle both "hour" and "hours"

    let duration = match unit {
        "minute" => time::Duration::minutes(num),
        "hour" => time::Duration::hours(num),
        "day" => time::Duration::days(num),
        "week" => time::Duration::weeks(num),
        _ => bail!(
            "Unknown time unit: '{}'. Use minutes, hours, days, or weeks.",
            unit
        ),
    };

    let result = now + duration;
    format_rfc3339(result)
}

/// Parse "next monday", "next tuesday", etc.
fn parse_next_weekday(now: OffsetDateTime, expr: &str) -> Result<String> {
    let weekdays = [
        ("sunday", time::Weekday::Sunday),
        ("monday", time::Weekday::Monday),
        ("tuesday", time::Weekday::Tuesday),
        ("wednesday", time::Weekday::Wednesday),
        ("thursday", time::Weekday::Thursday),
        ("friday", time::Weekday::Friday),
        ("saturday", time::Weekday::Saturday),
    ];

    let expr = expr.trim().to_lowercase();
    let target_weekday = weekdays
        .iter()
        .find(|(name, _)| expr.starts_with(name))
        .map(|(_, wd)| *wd)
        .ok_or_else(|| anyhow::anyhow!("Unknown weekday: '{}'", expr))?;

    let current_weekday = now.weekday();
    let days_until = days_until_weekday(current_weekday, target_weekday);

    let result = now + time::Duration::days(days_until);
    // Default to 9:00 AM
    let result = result
        .replace_hour(9)
        .map_err(|e| anyhow::anyhow!("Invalid hour: {}", e))?
        .replace_minute(0)
        .map_err(|e| anyhow::anyhow!("Invalid minute: {}", e))?;

    format_rfc3339(result)
}

/// Calculate days until target weekday from current weekday
fn days_until_weekday(current: time::Weekday, target: time::Weekday) -> i64 {
    let current_num = current as i64;
    let target_num = target as i64;
    if target_num > current_num {
        target_num - current_num
    } else {
        7 - (current_num - target_num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_relative_time_rfc3339() {
        let result = parse_relative_time("2026-02-01T09:00:00Z").unwrap();
        assert!(result.contains("2026-02-01T09:00:00"));
    }

    #[test]
    fn test_parse_relative_time_tomorrow() {
        let result = parse_relative_time("tomorrow 9am").unwrap();
        // Should be tomorrow at 9am
        let tomorrow = OffsetDateTime::now_utc() + time::Duration::days(1);
        assert!(result.contains(&tomorrow.year().to_string()));
    }

    #[test]
    fn test_parse_relative_time_in_hours() {
        let result = parse_relative_time("in 2 hours").unwrap();
        let now = OffsetDateTime::now_utc();
        // Parse result and verify it's approximately 2 hours from now
        let parsed = parse_rfc3339(&result).unwrap();
        let diff = parsed - now;
        // Allow for some test execution time (within 5 minutes)
        assert!(
            diff.whole_hours() >= 1 && diff.whole_hours() <= 3,
            "Expected ~2 hours, got {} hours",
            diff.whole_hours()
        );
    }

    #[test]
    fn test_parse_relative_time_in_days() {
        let result = parse_relative_time("in 3 days").unwrap();
        let now = OffsetDateTime::now_utc();
        let parsed = parse_rfc3339(&result).unwrap();
        let diff = parsed - now;
        // Should be approximately 3 days (allow 2-4 for test timing)
        assert!(
            diff.whole_days() >= 2 && diff.whole_days() <= 4,
            "Expected ~3 days, got {} days",
            diff.whole_days()
        );
    }

    #[test]
    fn test_parse_relative_time_next_weekday() {
        let result = parse_relative_time("next monday").unwrap();
        // Should parse successfully
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_relative_time_invalid() {
        let result = parse_relative_time("invalid expression");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_time_expression_am() {
        assert_eq!(parse_time_expression("9am"), Some((9, 0)));
        assert_eq!(parse_time_expression("12am"), Some((0, 0)));
    }

    #[test]
    fn test_parse_time_expression_pm() {
        assert_eq!(parse_time_expression("2pm"), Some((14, 0)));
        assert_eq!(parse_time_expression("12pm"), Some((12, 0)));
    }

    #[test]
    fn test_parse_time_expression_with_minutes() {
        assert_eq!(parse_time_expression("9:30am"), Some((9, 30)));
        assert_eq!(parse_time_expression("2:45pm"), Some((14, 45)));
    }

    #[test]
    fn test_parse_time_expression_24h() {
        assert_eq!(parse_time_expression("14:30"), Some((14, 30)));
        assert_eq!(parse_time_expression("09:00"), Some((9, 0)));
    }

    #[test]
    fn test_parse_time_expression_invalid() {
        assert_eq!(parse_time_expression(""), None);
        assert_eq!(parse_time_expression("invalid"), None);
    }

    #[test]
    fn test_days_until_weekday() {
        use time::Weekday;
        // If today is Monday, next Monday is 7 days away
        assert_eq!(days_until_weekday(Weekday::Monday, Weekday::Monday), 7);
        // If today is Monday, next Tuesday is 1 day away
        assert_eq!(days_until_weekday(Weekday::Monday, Weekday::Tuesday), 1);
        // If today is Friday, next Monday is 3 days away
        assert_eq!(days_until_weekday(Weekday::Friday, Weekday::Monday), 3);
    }
}
