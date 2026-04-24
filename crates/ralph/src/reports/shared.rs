//! Shared reporting utilities and types.
//!
//! Purpose:
//! - Shared reporting utilities and types.
//!
//! Responsibilities:
//! - Define common report output formats (text/JSON).
//! - Provide duration formatting helpers used by all reports.
//!
//! Not handled here:
//! - Report-specific business logic.
//! - CLI argument parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Durations are formatted consistently across all reports.

use anyhow::Result;
use serde::Serialize;
use time::Duration;

/// Output format for report commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReportFormat {
    Text,
    Json,
}

/// Print a report as JSON.
pub(crate) fn print_json<T: Serialize>(report: &T) -> Result<()> {
    let rendered = serde_json::to_string_pretty(report)?;
    print!("{rendered}");
    Ok(())
}

/// Format a Duration as a human-readable string (e.g., "2h 30m", "1d 4h").
pub(crate) fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.whole_seconds();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;

    let mut parts = Vec::new();

    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 || (hours == 0 && days == 0) {
        parts.push(format!("{}m", minutes));
    }

    if parts.is_empty() {
        "0m".to_string()
    } else {
        parts.join(" ")
    }
}

/// Format a date as a simple key string (YYYY-MM-DD).
pub(crate) fn format_date_key(dt: time::OffsetDateTime) -> String {
    format!("{:04}-{:02}-{:02}", dt.year(), dt.month() as u8, dt.day())
}

/// Calculate the average duration from a slice of durations.
pub(crate) fn avg_duration(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }

    let total_seconds: i64 = durations.iter().map(|d| d.whole_seconds()).sum();
    Duration::seconds(total_seconds / durations.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct JsonSmokeReport {
        value: &'static str,
    }

    #[test]
    fn test_format_duration_zero() {
        let duration = Duration::ZERO;
        assert_eq!(format_duration(duration), "0m");
    }

    #[test]
    fn test_format_duration_minutes_only() {
        let duration = Duration::minutes(45);
        assert_eq!(format_duration(duration), "45m");
    }

    #[test]
    fn test_format_duration_hours_and_minutes() {
        let duration = Duration::hours(2) + Duration::minutes(30);
        assert_eq!(format_duration(duration), "2h 30m");
    }

    #[test]
    fn test_format_duration_days_and_hours() {
        let duration = Duration::days(1) + Duration::hours(4) + Duration::minutes(15);
        assert_eq!(format_duration(duration), "1d 4h 15m");
    }

    #[test]
    fn test_format_duration_days_only() {
        let duration = Duration::days(3);
        assert_eq!(format_duration(duration), "3d 0h");
    }

    #[test]
    fn test_avg_duration_empty() {
        let durations: Vec<Duration> = vec![];
        assert_eq!(avg_duration(&durations), Duration::ZERO);
    }

    #[test]
    fn test_avg_duration_single() {
        let durations = vec![Duration::hours(2)];
        assert_eq!(avg_duration(&durations), Duration::hours(2));
    }

    #[test]
    fn test_avg_duration_multiple() {
        let durations = vec![Duration::hours(1), Duration::hours(2), Duration::hours(3)];
        assert_eq!(avg_duration(&durations), Duration::hours(2));
    }

    #[test]
    fn test_format_date_key() {
        let dt = time::OffsetDateTime::now_utc()
            .replace_year(2026)
            .unwrap()
            .replace_month(time::Month::January)
            .unwrap()
            .replace_day(19)
            .unwrap()
            .replace_hour(12)
            .unwrap()
            .replace_minute(30)
            .unwrap()
            .replace_second(0)
            .unwrap();
        assert_eq!(format_date_key(dt), "2026-01-19");
    }

    #[test]
    fn test_print_json_smoke() {
        let report = JsonSmokeReport { value: "ok" };
        let result = print_json(&report);
        assert!(result.is_ok());
    }
}
