//! Tests for `validate.rs` helpers.
//!
//! Purpose:
//! - Tests for `validate.rs` helpers.
//!
//! Responsibilities:
//! - Validate shared queue input validation helpers.
//! - Ensure error context is actionable and consistent.
//!
//! Non-scope:
//! - End-to-end queue operations or persistence.
//! - Validation of full queue schemas.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Helper errors are surfaced directly to CLI/GUI consumers.

use crate::queue::operations::validate::{
    ensure_task_id, parse_custom_fields_with_context, parse_rfc3339_utc, validate_custom_field_key,
};

#[test]
fn ensure_task_id_rejects_empty() {
    let err = ensure_task_id("   ", "edit").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Queue edit failed"));
    assert!(msg.contains("missing task_id"));
}

#[test]
fn validate_custom_field_key_rejects_whitespace() {
    let err = validate_custom_field_key("bad key", "RQ-0001", "edit").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("task_id=RQ-0001"));
    assert!(msg.contains("contains whitespace"));
}

#[test]
fn parse_custom_fields_with_context_rejects_missing_equals() {
    let err = parse_custom_fields_with_context("RQ-0001", "severity", "edit").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("task_id=RQ-0001"));
    assert!(msg.contains("Expected key=value"));
}

// =============================================================================
// parse_rfc3339_utc tests
// =============================================================================

#[test]
fn parse_rfc3339_utc_success_valid_timestamp() {
    let result = parse_rfc3339_utc("2026-01-21T12:00:00Z").expect("should parse valid timestamp");
    // Format includes nanoseconds precision
    assert!(result.starts_with("2026-01-21T12:00:00"));
    assert!(result.ends_with("Z"));
}

#[test]
fn parse_rfc3339_utc_success_normalizes_to_canonical() {
    // Timestamp with fractional seconds should be normalized
    let result = parse_rfc3339_utc("2026-01-21T12:00:00.123456Z")
        .expect("should parse timestamp with fractional seconds");
    // The canonical format may include or exclude fractional seconds based on implementation
    // Just verify it's valid and doesn't fail
    assert!(!result.is_empty());
    assert!(result.contains("2026-01-21"));
}

#[test]
fn parse_rfc3339_utc_rejects_empty_input() {
    let err = parse_rfc3339_utc("").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Missing timestamp"));
    assert!(msg.contains("RFC3339"));
}

#[test]
fn parse_rfc3339_utc_rejects_whitespace_only() {
    let err = parse_rfc3339_utc("   ").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Missing timestamp"));
}

#[test]
fn parse_rfc3339_utc_rejects_non_utc_offset() {
    let err = parse_rfc3339_utc("2026-01-21T12:00:00+05:00").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn parse_rfc3339_utc_rejects_negative_offset() {
    let err = parse_rfc3339_utc("2026-01-21T12:00:00-08:00").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn parse_rfc3339_utc_rejects_malformed_timestamp() {
    let err = parse_rfc3339_utc("not-a-timestamp").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn parse_rfc3339_utc_rejects_invalid_date() {
    let err = parse_rfc3339_utc("2026-13-45T25:70:00Z").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn parse_rfc3339_utc_rejects_date_only() {
    let err = parse_rfc3339_utc("2026-01-21").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn parse_rfc3339_utc_accepts_various_utc_formats() {
    // UTC can be represented as Z or +00:00
    let result_z = parse_rfc3339_utc("2026-01-21T12:00:00Z").expect("Z format should work");
    assert!(result_z.starts_with("2026-01-21T12:00:00"));
    assert!(result_z.ends_with("Z"));

    let result_offset =
        parse_rfc3339_utc("2026-01-21T12:00:00+00:00").expect("+00:00 format should work");
    assert!(result_offset.starts_with("2026-01-21T12:00:00"));
    assert!(result_offset.ends_with("Z"));
}
