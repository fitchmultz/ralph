//! Core queue validation runtime tests.
//!
//! Purpose:
//! - Core queue validation runtime tests.
//!
//! Responsibilities:
//! - Cover queue item required fields and per-task validation rules.
//! - Cover queue/done archive cross-file validation behavior.
//! - Keep baseline validation expectations separate from graph-like relations.
//!
//! Not handled here:
//! - Dependency-chain warning behavior.
//! - Relationship or parent validation semantics.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - These tests exercise `validate_queue` and `validate_queue_set` directly.
//! - Queue/done fixtures use helpers from `support.rs`.

use crate::contracts::{ModelEffort, QueueFile, TaskAgent, TaskStatus};

use super::support::{task, task_with, task_with_agent};
use crate::queue::validation::{validate_queue, validate_queue_set};

#[test]
fn validate_rejects_duplicate_ids() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0002"), task("RQ-0001")],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("duplicate") && msg.contains("RQ-0001"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_allows_missing_request() {
    let mut missing_request = task("RQ-0001");
    missing_request.request = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![missing_request],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_allows_empty_lists() {
    let mut empty_lists = task("RQ-0001");
    empty_lists.tags.clear();
    empty_lists.scope.clear();
    empty_lists.evidence.clear();
    empty_lists.plan.clear();
    let queue = QueueFile {
        version: 1,
        tasks: vec![empty_lists],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_rejects_missing_created_at() {
    let mut missing_created = task("RQ-0001");
    missing_created.created_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![missing_created],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing created_at"));
}

#[test]
fn validate_rejects_missing_updated_at() {
    let mut missing_updated = task("RQ-0001");
    missing_updated.updated_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![missing_updated],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing updated_at"));
}

#[test]
fn validate_rejects_invalid_rfc3339() {
    let mut invalid_timestamp = task("RQ-0001");
    invalid_timestamp.created_at = Some("not a date".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![invalid_timestamp],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn validate_rejects_zero_agent_iterations() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task_with_agent(
            "RQ-0001",
            TaskAgent {
                runner: None,
                model: None,
                model_effort: ModelEffort::Default,
                phases: None,
                iterations: Some(0),
                followup_reasoning_effort: None,
                runner_cli: None,
                phase_overrides: None,
            },
        )],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("agent.iterations"));
}

#[test]
fn validate_rejects_invalid_agent_phases() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task_with_agent(
            "RQ-0001",
            TaskAgent {
                runner: None,
                model: None,
                model_effort: ModelEffort::Default,
                phases: Some(4),
                iterations: None,
                followup_reasoning_effort: None,
                runner_cli: None,
                phase_overrides: None,
            },
        )],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("agent.phases"));
}

#[test]
fn validate_queue_set_rejects_cross_file_duplicates() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    assert!(format!("{err}").contains("Duplicate task ID detected across queue and done"));
}

#[test]
fn validate_queue_allows_duplicate_if_one_is_rejected() {
    let mut rejected = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
            rejected,
        ],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_rejects_done_without_completed_at() {
    let mut done_task = task("RQ-0001");
    done_task.status = TaskStatus::Done;
    done_task.completed_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing completed_at"));
}

#[test]
fn validate_queue_set_allows_duplicate_across_files_if_rejected() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["tag".to_string()],
        )],
    };
    let mut rejected_done = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    rejected_done.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![rejected_done],
    };
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4, 10).is_ok());

    let mut rejected_active = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    rejected_active.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let active = QueueFile {
        version: 1,
        tasks: vec![rejected_active],
    };
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4, 10).is_ok());
}

#[test]
fn validate_queue_set_rejects_todo_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["tag".to_string()],
        )],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("done.json") && msg.contains("RQ-0001") && msg.contains("Todo"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_queue_set_rejects_doing_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Doing,
            vec!["tag".to_string()],
        )],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("done.json") && msg.contains("RQ-0001") && msg.contains("Doing"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_queue_set_rejects_draft_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Draft,
            vec!["tag".to_string()],
        )],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("done.json") && msg.contains("RQ-0001") && msg.contains("Draft"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_queue_set_allows_terminal_statuses_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let mut rejected_task = task_with("RQ-0003", TaskStatus::Rejected, vec!["tag".to_string()]);
    rejected_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task, rejected_task],
    };
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4, 10).is_ok());
}
