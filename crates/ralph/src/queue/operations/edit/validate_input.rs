//! Input validation helpers for edit operations.
//!
//! Responsibilities:
//! - Wrap validate module functions for use in edit operations.
//!
//! Does not handle:
//! - Full task validation (see `queue::validate_queue_set`).

use crate::queue::operations::validate::parse_rfc3339_utc;
use anyhow::Result;

/// Ensure we have a valid RFC3339 timestamp for `now`.
pub(crate) fn ensure_now(now_rfc3339: &str) -> Result<String> {
    parse_rfc3339_utc(now_rfc3339)
}
