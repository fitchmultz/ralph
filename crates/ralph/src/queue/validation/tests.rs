//! Unit tests for queue validation.

use super::*;
use crate::contracts::{Task, TaskAgent, TaskStatus};
use std::collections::HashMap;

fn task(id: &str) -> Task {
    task_with(id, TaskStatus::Todo, vec!["code".to_string()])
}

fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags,
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: HashMap::new(),
    }
}

#[test]
fn validate_rejects_duplicate_ids() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0001")],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("duplicate"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_allows_missing_request() {
    let mut task = task("RQ-0001");
    task.request = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_allows_empty_lists() {
    let mut task = task("RQ-0001");
    task.tags = vec![];
    task.scope = vec![];
    task.evidence = vec![];
    task.plan = vec![];
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_rejects_missing_created_at() {
    let mut task = task("RQ-0001");
    task.created_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing created_at"));
}

#[test]
fn validate_rejects_missing_updated_at() {
    let mut task = task("RQ-0001");
    task.updated_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing updated_at"));
}

#[test]
fn validate_rejects_invalid_rfc3339() {
    let mut task = task("RQ-0001");
    task.created_at = Some("not a date".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn validate_rejects_zero_agent_iterations() {
    let mut task = task("RQ-0001");
    task.agent = Some(TaskAgent {
        runner: None,
        model: None,
        model_effort: crate::contracts::ModelEffort::Default,
        iterations: Some(0),
        followup_reasoning_effort: None,
    });
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("agent.iterations"));
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
    let err = validate_queue_set(&active, Some(&done), "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Duplicate task ID detected across queue and done"));
}

#[test]
fn validate_queue_allows_duplicate_if_one_is_rejected() {
    let mut t_rejected = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
            t_rejected,
        ],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_rejects_done_without_completed_at() {
    let mut task = task("RQ-0001");
    task.status = TaskStatus::Done;
    task.completed_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
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
    let mut t_rejected = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![t_rejected],
    };
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4).is_ok());

    let mut t_rejected2 = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    t_rejected2.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let active2 = QueueFile {
        version: 1,
        tasks: vec![t_rejected2],
    };
    let mut t_done = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    t_done.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done2 = QueueFile {
        version: 1,
        tasks: vec![t_done],
    };
    assert!(validate_queue_set(&active2, Some(&done2), "RQ", 4).is_ok());
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
    let err = validate_queue_set(&active, Some(&done), "RQ", 4).unwrap_err();
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
    let err = validate_queue_set(&active, Some(&done), "RQ", 4).unwrap_err();
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
    let err = validate_queue_set(&active, Some(&done), "RQ", 4).unwrap_err();
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
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4).is_ok());
}
