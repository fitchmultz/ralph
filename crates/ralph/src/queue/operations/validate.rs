//! Shared validation helpers for queue operations.

use anyhow::{bail, Context, Result};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub(crate) fn parse_rfc3339_utc(now_rfc3339: &str) -> Result<String> {
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;
    Ok(now.to_string())
}
