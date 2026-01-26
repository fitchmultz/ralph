//! Tests for `validate.rs` helpers.
//!
//! Responsibilities:
//! - Validate shared queue input validation helpers.
//! - Ensure error context is actionable and consistent.
//!
//! Does not handle:
//! - End-to-end queue operations or persistence.
//! - Validation of full queue schemas.
//!
//! Assumptions/invariants:
//! - Helper errors are surfaced directly to CLI/TUI consumers.
//! - `parse_rfc3339_utc()` is exercised indirectly via operations that validate
//!   timestamps (e.g., set_status/archive_terminal_tasks_in_memory error paths).

use crate::queue::operations::validate::{
    ensure_task_id, parse_custom_fields_with_context, validate_custom_field_key,
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
