use anyhow::{Context, Result};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const FALLBACK_RFC3339: &str = "2026-01-18T00:00:00Z";

pub fn now_utc_rfc3339() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format RFC3339 timestamp")
}

pub fn now_utc_rfc3339_or_fallback() -> String {
    now_utc_rfc3339().unwrap_or_else(|_| FALLBACK_RFC3339.to_string())
}
