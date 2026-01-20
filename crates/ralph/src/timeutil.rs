//! Time helpers for RFC3339 timestamps with consistent precision.

use anyhow::{Context, Result};
use std::sync::OnceLock;
use time::format_description::FormatItem;
use time::OffsetDateTime;

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

pub fn now_utc_rfc3339_or_fallback() -> String {
    now_utc_rfc3339().unwrap_or_else(|_| FALLBACK_RFC3339.to_string())
}
