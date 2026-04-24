//! Date key utilities for productivity tracking.
//!
//! Purpose:
//! - Date key utilities for productivity tracking.
//!
//! Responsibilities:
//! - Parse and format date keys (YYYY-MM-DD format).
//! - Calculate date offsets and previous/next days.
//!
//! Not handled here:
//! - Persistence or business logic (see other modules).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Date keys are always in YYYY-MM-DD format.
//! - Uses the `time` crate for calendar math (handles leap years, month boundaries).

use time::macros::format_description;
use time::{Date, Duration};

/// Parse a date key (YYYY-MM-DD) into a `time::Date`.
pub fn parse_date_key(date_key: &str) -> Option<Date> {
    let trimmed = date_key.trim();
    if trimmed.is_empty() {
        return None;
    }
    Date::parse(trimmed, &format_description!("[year]-[month]-[day]")).ok()
}

/// Format a `time::Date` as a date key (YYYY-MM-DD).
pub fn format_date_key(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

/// Return `date_key` offset by `delta_days`.
///
/// `delta_days = -1` means previous day.
pub fn date_key_add_days(date_key: &str, delta_days: i64) -> Option<String> {
    let date = parse_date_key(date_key)?;
    let date = date.checked_add(Duration::days(delta_days))?;
    Some(format_date_key(date))
}

/// Return the previous day's date key.
pub fn previous_date_key(date_key: &str) -> Option<String> {
    date_key_add_days(date_key, -1)
}
