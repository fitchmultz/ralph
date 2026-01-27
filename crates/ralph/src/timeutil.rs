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
            time::format_description::parse(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z",
            )
            .expect("valid RFC3339 format")
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
