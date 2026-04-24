//! Tests for task edit operations.
//!
//! Purpose:
//! - Tests for task edit operations.
//!
//! Responsibilities:
//! - Test apply_task_edit and preview_task_edit functionality.
//! - Verify TaskEditKey formatting and parsing.
//! - Ensure preview and apply behavior is consistent.
//!
//! Non-scope:
//! - Queue persistence testing (integration tests).
//! - Cross-module queue validation (see queue validation tests).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::super::{TaskEditKey, apply_task_edit, format_field_value, preview_task_edit};
use crate::contracts::{QueueFile, Task, TaskAgent, TaskPriority, TaskStatus};
use std::collections::HashMap;

fn test_task() -> Task {
    Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        tags: vec!["rust".to_string(), "cli".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["step 1".to_string()],
        notes: vec!["note".to_string()],
        request: Some("test request".to_string()),
        created_at: Some("2026-01-20T12:00:00Z".to_string()),
        updated_at: Some("2026-01-20T12:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        estimated_minutes: None,
        actual_minutes: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        agent: None,
        parent_id: None,
    }
}

fn test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![test_task()],
    }
}

#[test]
fn preview_task_edit_shows_title_change() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Title,
        "New title",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.task_id, "RQ-0001");
    assert_eq!(preview.field, "title");
    assert_eq!(preview.old_value, "Test task");
    assert_eq!(preview.new_value, "New title");
}

#[test]
fn preview_task_edit_shows_status_change() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Status,
        "doing",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.field, "status");
    assert_eq!(preview.old_value, "todo");
    assert_eq!(preview.new_value, "doing");
}

#[test]
fn preview_task_edit_shows_priority_change() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Priority,
        "high",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.field, "priority");
    assert_eq!(preview.old_value, "medium");
    assert_eq!(preview.new_value, "high");
}

#[test]
fn preview_task_edit_shows_tags_change() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Tags,
        "bug, urgent",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.field, "tags");
    assert_eq!(preview.old_value, "rust, cli");
    assert_eq!(preview.new_value, "bug, urgent");
}

#[test]
fn preview_task_edit_validates_empty_title() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let result = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Title,
        "",
        &now,
        "RQ",
        4,
        10,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("title cannot be empty"));
}

#[test]
fn preview_task_edit_fails_for_missing_task() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let result = preview_task_edit(
        &queue,
        None,
        "RQ-9999",
        TaskEditKey::Title,
        "New title",
        &now,
        "RQ",
        4,
        10,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));
}

#[test]
fn preview_task_edit_validates_invalid_status() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let result = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Status,
        "invalid_status",
        &now,
        "RQ",
        4,
        10,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // The error message is wrapped in context, so we check for the context message
    assert!(
        err.contains("field=status"),
        "error should mention field=status: {}",
        err
    );
}

#[test]
fn preview_task_edit_clears_request_with_empty_string() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Request,
        "",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.field, "request");
    assert_eq!(preview.old_value, "test request");
    assert_eq!(preview.new_value, "");
}

#[test]
fn preview_task_edit_shows_custom_fields_change() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::CustomFields,
        "severity=high, owner=ralph",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.field, "custom_fields");
    assert_eq!(preview.old_value, "");
    // HashMap iteration order is not deterministic, so check for content not exact order
    assert!(
        preview.new_value.contains("severity=high"),
        "new_value should contain severity=high: {}",
        preview.new_value
    );
    assert!(
        preview.new_value.contains("owner=ralph"),
        "new_value should contain owner=ralph: {}",
        preview.new_value
    );
}

#[test]
fn preview_task_edit_shows_agent_override_change() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();
    let input = r#"{"runner":"codex","model":"gpt-5.3-codex","phases":2,"iterations":1}"#;

    let preview = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Agent,
        input,
        &now,
        "RQ",
        4,
        10,
    )
    .expect("preview should succeed");

    assert_eq!(preview.field, "agent");
    assert_eq!(preview.old_value, "");
    assert!(preview.new_value.contains("\"runner\":\"codex\""));
    assert!(preview.new_value.contains("\"phases\":2"));
}

#[test]
fn apply_task_edit_clears_agent_override_with_empty_value() {
    let mut queue = test_queue();
    queue.tasks[0].agent = Some(TaskAgent {
        runner: Some(crate::contracts::Runner::Codex),
        model: Some(crate::contracts::Model::Gpt53Codex),
        phases: Some(2),
        iterations: Some(1),
        ..Default::default()
    });
    let now = "2026-01-21T12:00:00Z".to_string();

    apply_task_edit(
        &mut queue,
        None,
        "RQ-0001",
        TaskEditKey::Agent,
        "",
        &now,
        "RQ",
        4,
        10,
    )
    .expect("apply should succeed");

    assert!(queue.tasks[0].agent.is_none());
}

#[test]
fn task_edit_key_format_value_with_newline_separator() {
    let task = test_task();

    assert_eq!(TaskEditKey::Tags.format_value(&task, "\n"), "rust\ncli");
    assert_eq!(TaskEditKey::Scope.format_value(&task, "\n"), "crates/ralph");
    assert_eq!(TaskEditKey::Title.format_value(&task, "\n"), "Test task");
}

#[test]
fn task_edit_key_format_value_with_comma_separator() {
    let task = test_task();

    assert_eq!(TaskEditKey::Tags.format_value(&task, ", "), "rust, cli");
    assert_eq!(TaskEditKey::DependsOn.format_value(&task, ", "), "");
}

#[test]
fn task_edit_key_is_list_field_identifies_lists_correctly() {
    assert!(TaskEditKey::Tags.is_list_field());
    assert!(TaskEditKey::Scope.is_list_field());
    assert!(TaskEditKey::Evidence.is_list_field());
    assert!(TaskEditKey::Plan.is_list_field());
    assert!(TaskEditKey::Notes.is_list_field());
    assert!(TaskEditKey::DependsOn.is_list_field());
    assert!(TaskEditKey::Blocks.is_list_field());
    assert!(TaskEditKey::RelatesTo.is_list_field());

    assert!(!TaskEditKey::Title.is_list_field());
    assert!(!TaskEditKey::Status.is_list_field());
    assert!(!TaskEditKey::Priority.is_list_field());
    assert!(!TaskEditKey::Request.is_list_field());
    assert!(!TaskEditKey::Duplicates.is_list_field());
    assert!(!TaskEditKey::ScheduledStart.is_list_field());
}

#[test]
fn preview_task_edit_invalid_priority_includes_canonical_parser_error() {
    let queue = test_queue();
    let now = "2026-01-21T12:00:00Z".to_string();

    let err = preview_task_edit(
        &queue,
        None,
        "RQ-0001",
        TaskEditKey::Priority,
        "nope",
        &now,
        "RQ",
        4,
        10,
    )
    .unwrap_err();

    let msg = err.to_string();

    // The outer context message should contain field=priority
    assert!(msg.contains("field=priority"), "err was: {msg}");

    // The canonical parser error should be in the error chain (source)
    let expected = "nope".parse::<TaskPriority>().unwrap_err().to_string();
    let found_canonical = err.chain().any(|e| e.to_string().contains(&expected));
    assert!(
        found_canonical,
        "canonical error not in chain. err was: {msg}, expected: {expected}"
    );
}

#[test]
fn format_field_value_uses_contextual_separators() {
    let mut task = test_task();
    task.evidence = vec!["item1".to_string(), "item2".to_string()];
    task.plan = vec!["step1".to_string(), "step2".to_string()];

    // Evidence uses "; " separator
    assert_eq!(
        format_field_value(&task, TaskEditKey::Evidence),
        "item1; item2"
    );

    // Plan uses "; " separator
    assert_eq!(format_field_value(&task, TaskEditKey::Plan), "step1; step2");

    // Tags uses ", " separator
    assert_eq!(format_field_value(&task, TaskEditKey::Tags), "rust, cli");
}

#[test]
fn preview_and_apply_cycle_status_in_the_same_order() {
    let now = "2026-01-21T12:00:00Z".to_string();

    // Start from the module's default test task status (currently Todo).
    let mut apply_queue = test_queue();

    // Cycle through all statuses once, comparing preview's computed next value
    // to apply's real mutation at each step.
    for _ in 0..5 {
        let preview = preview_task_edit(
            &apply_queue,
            None,
            "RQ-0001",
            TaskEditKey::Status,
            "", // empty => cycle
            &now,
            "RQ",
            4,
            10,
        )
        .expect("preview should succeed");

        apply_task_edit(
            &mut apply_queue,
            None,
            "RQ-0001",
            TaskEditKey::Status,
            "", // empty => cycle
            &now,
            "RQ",
            4,
            10,
        )
        .expect("apply should succeed");

        let applied = apply_queue.tasks[0].status.to_string();
        assert_eq!(preview.new_value, applied);
    }
}

#[test]
fn preview_and_apply_invalid_status_share_canonical_parse_error() {
    let now = "2026-01-21T12:00:00Z".to_string();

    let preview_err = preview_task_edit(
        &test_queue(),
        None,
        "RQ-0001",
        TaskEditKey::Status,
        "paused",
        &now,
        "RQ",
        4,
        10,
    )
    .unwrap_err();

    let apply_err = {
        let mut q = test_queue();
        apply_task_edit(
            &mut q,
            None,
            "RQ-0001",
            TaskEditKey::Status,
            "paused",
            &now,
            "RQ",
            4,
            10,
        )
        .unwrap_err()
    };

    let expected = "Invalid status: 'paused'. Expected one of: draft, todo, doing, done, rejected.";

    assert!(
        preview_err.chain().any(|e| e.to_string() == expected),
        "preview should include canonical parser error in chain: {}",
        preview_err
    );
    assert!(
        apply_err.chain().any(|e| e.to_string() == expected),
        "apply should include canonical parser error in chain: {}",
        apply_err
    );
}
