//! Normalization-focused queue import tests.
//!
//! Purpose:
//! - Normalization-focused queue import tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::super::normalize::normalize_task;
use crate::contracts::{Task, TaskStatus};

#[test]
fn normalize_task_trims_fields() {
    let mut task = Task {
        id: "  RQ-0001  ".to_string(),
        title: "  Test  ".to_string(),
        description: None,
        tags: vec!["  a  ".to_string(), "".to_string(), "  b  ".to_string()],
        ..Default::default()
    };

    normalize_task(&mut task, "2026-01-01T00:00:00.000000000Z");
    assert_eq!(task.id, "RQ-0001");
    assert_eq!(task.title, "Test");
    assert_eq!(task.tags, vec!["a", "b"]);
}

#[test]
fn normalize_task_backfills_timestamps() {
    let mut task = Task {
        id: "RQ-0001".to_string(),
        title: "Test".to_string(),
        description: None,
        status: TaskStatus::Todo,
        ..Default::default()
    };
    let now = "2026-01-01T00:00:00.000000000Z";
    normalize_task(&mut task, now);
    assert_eq!(task.created_at, Some(now.to_string()));
    assert_eq!(task.updated_at, Some(now.to_string()));
    assert_eq!(task.completed_at, None);
}

#[test]
fn normalize_task_backfills_completed_at_for_terminal() {
    let now = "2026-01-01T00:00:00.000000000Z";

    let mut done_task = Task {
        id: "RQ-0001".to_string(),
        title: "Test".to_string(),
        description: None,
        status: TaskStatus::Done,
        ..Default::default()
    };
    normalize_task(&mut done_task, now);
    assert_eq!(done_task.completed_at, Some(now.to_string()));

    let mut rejected_task = Task {
        id: "RQ-0002".to_string(),
        title: "Test".to_string(),
        description: None,
        status: TaskStatus::Rejected,
        ..Default::default()
    };
    normalize_task(&mut rejected_task, now);
    assert_eq!(rejected_task.completed_at, Some(now.to_string()));
}
