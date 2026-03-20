//! Purpose: JSON task-field comparison coverage for task command tests.
//!
//! Responsibilities:
//! - Validate unchanged-field detection.
//! - Validate changed-field detection for modified JSON values.
//! - Verify invalid JSON input surfaces errors.
//!
//! Scope:
//! - `task_cmd::compare_task_fields` behavior only.
//!
//! Usage:
//! - Uses `super::*;` to access the shared suite imports.
//!
//! Invariants/assumptions callers must respect:
//! - Tests cover JSON diff contract behavior only, not downstream update reporting.

use super::*;

#[test]
fn test_compare_task_fields_no_changes() {
    let before = r#"{"id":"RQ-0001","status":"todo","title":"Test task"}"#;
    let after = r#"{"id":"RQ-0001","status":"todo","title":"Test task"}"#;

    let result = task_cmd::compare_task_fields(before, after);
    assert!(result.is_ok());
    let changed = result.unwrap();
    assert_eq!(changed.len(), 0);
}

#[test]
fn test_compare_task_fields_some_changes() {
    let before = r#"{"id":"RQ-0001","status":"todo","title":"Test task"}"#;
    let after = r#"{"id":"RQ-0001","status":"doing","title":"Updated task"}"#;

    let result = task_cmd::compare_task_fields(before, after);
    assert!(result.is_ok());
    let changed = result.unwrap();
    assert!(changed.contains(&"status".to_string()));
    assert!(changed.contains(&"title".to_string()));
}

#[test]
fn test_compare_task_fields_invalid_json() {
    let before = "{invalid json}";
    let after = r#"{"id":"RQ-0001"}"#;

    let result = task_cmd::compare_task_fields(before, after);
    assert!(result.is_err());
}
