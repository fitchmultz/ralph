//! Input validation helpers for edit operations.
//!
//! Purpose:
//! - Input validation helpers for edit operations.
//!
//! Responsibilities:
//! - Wrap validate module functions for use in edit operations.
//!
//! Non-scope:
//! - Full task validation (see `queue::validate_queue_set`).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::queue::operations::validate::parse_rfc3339_utc;
use anyhow::Result;

/// Ensure we have a valid RFC3339 timestamp for `now`.
pub(crate) fn ensure_now(now_rfc3339: &str) -> Result<String> {
    parse_rfc3339_utc(now_rfc3339)
}
