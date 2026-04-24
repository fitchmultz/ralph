//! Regression coverage for machine task command parsing helpers.
//!
//! Purpose:
//! - Regression coverage for machine task command parsing helpers.
//!
//! Responsibilities:
//! - Validate supported status parsing.
//! - Validate supported child-policy parsing.
//!
//! Not handled here:
//! - Machine task write workflows.
//! - Queue mutation/decomposition integration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Parsing remains case-insensitive for supported values.
//! - Unsupported values continue to fail fast.

use super::{parse_child_policy, parse_task_status};
use crate::commands::task::DecompositionChildPolicy;
use crate::contracts::TaskStatus;

#[test]
fn parse_task_status_accepts_supported_values_case_insensitively() {
    assert_eq!(
        parse_task_status("ToDo").expect("todo status"),
        TaskStatus::Todo
    );
    assert_eq!(
        parse_task_status("done").expect("done status"),
        TaskStatus::Done
    );
}

#[test]
fn parse_task_status_rejects_unknown_values() {
    assert!(parse_task_status("later").is_err());
}

#[test]
fn parse_child_policy_accepts_supported_values_case_insensitively() {
    assert_eq!(
        parse_child_policy("Append").expect("append child policy"),
        DecompositionChildPolicy::Append
    );
}

#[test]
fn parse_child_policy_rejects_unknown_values() {
    assert!(parse_child_policy("merge").is_err());
}
