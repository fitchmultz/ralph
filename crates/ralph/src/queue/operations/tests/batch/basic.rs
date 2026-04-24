//! Basic batch-operation regression coverage.
//!
//! Purpose:
//! - Basic batch-operation regression coverage.
//!
//! Responsibilities:
//! - Cover tag filtering, task-ID collection, resolution, and result helpers.
//! - Lock down order-preserving deduplication and batch summary behavior.
//!
//! Non-scope:
//! - Mutation-path failure handling for batch edit/status/field commands.
//! - Queue persistence or CLI integration behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Tests reuse the parent module's shared fixtures and imports via `super::*`.
//! - Helper-only behavior should stay deterministic for stable batch orchestration.

use super::*;

// =============================================================================
// Tag filtering tests
// =============================================================================

#[test]
fn filter_tasks_by_tags_matches_case_insensitive() {
    let tasks = vec![
        task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["rust".to_string(), "cli".to_string()],
        ),
        task_with(
            "RQ-0002",
            TaskStatus::Todo,
            vec!["Rust".to_string(), "backend".to_string()],
        ),
        task_with("RQ-0003", TaskStatus::Todo, vec!["python".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["rust".to_string()]);

    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|t| t.id == "RQ-0001"));
    assert!(result.iter().any(|t| t.id == "RQ-0002"));
}

#[test]
fn filter_tasks_by_tags_uses_or_logic() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0002", TaskStatus::Todo, vec!["cli".to_string()]),
        task_with("RQ-0003", TaskStatus::Todo, vec!["python".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["rust".to_string(), "cli".to_string()]);

    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|t| t.id == "RQ-0001"));
    assert!(result.iter().any(|t| t.id == "RQ-0002"));
}

#[test]
fn filter_tasks_by_tags_empty_filter_returns_empty() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &[]);

    assert!(result.is_empty());
}

#[test]
fn filter_tasks_by_tags_no_match_returns_empty() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["python".to_string()]);

    assert!(result.is_empty());
}

#[test]
fn filter_tasks_by_tags_trims_whitespace() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = filter_tasks_by_tags(&queue, &["  rust  ".to_string(), "".to_string()]);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "RQ-0001");
}

// =============================================================================
// Deduplication tests
// =============================================================================

#[test]
fn deduplicate_task_ids_preserves_order() {
    let ids = vec![
        "RQ-0001".to_string(),
        "RQ-0002".to_string(),
        "RQ-0001".to_string(),
        "RQ-0003".to_string(),
        "RQ-0002".to_string(),
    ];

    let result = deduplicate_task_ids(&ids);

    assert_eq!(result, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
}

#[test]
fn deduplicate_task_ids_skips_empty() {
    let ids = vec![
        "RQ-0001".to_string(),
        "".to_string(),
        "RQ-0002".to_string(),
        " ".to_string(),
    ];

    let result = deduplicate_task_ids(&ids);

    assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
}

#[test]
fn deduplicate_task_ids_trims_whitespace() {
    let ids = vec![
        "RQ-0001".to_string(),
        "  RQ-0001  ".to_string(),
        "RQ-0002".to_string(),
    ];

    let result = deduplicate_task_ids(&ids);

    // After trimming, "  RQ-0001  " equals "RQ-0001", so it's a duplicate
    assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
}

// =============================================================================
// Collect task IDs tests
// =============================================================================

#[test]
fn collect_task_ids_gathers_all_ids() {
    let tasks = [
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Doing, vec![]),
    ];
    let task_refs: Vec<&Task> = tasks.iter().collect();

    let result = collect_task_ids(&task_refs);

    assert_eq!(result, vec!["RQ-0001", "RQ-0002"]);
}

// =============================================================================
// resolve_task_ids tests
// =============================================================================

#[test]
fn resolve_task_ids_prefers_tag_filter() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0002", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0003", TaskStatus::Todo, vec!["python".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result = resolve_task_ids(
        &queue,
        &["RQ-0003".to_string()], // Should be ignored
        &["rust".to_string()],
    )
    .expect("should resolve tasks");

    assert_eq!(result.len(), 2);
    assert!(result.contains(&"RQ-0001".to_string()));
    assert!(result.contains(&"RQ-0002".to_string()));
}

#[test]
fn resolve_task_ids_uses_explicit_ids_when_no_tag_filter() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec![]),
        task_with("RQ-0002", TaskStatus::Todo, vec![]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result =
        resolve_task_ids(&queue, &["RQ-0001".to_string()], &[]).expect("should resolve tasks");

    assert_eq!(result, vec!["RQ-0001"]);
}

#[test]
fn resolve_task_ids_errors_on_empty_input() {
    let tasks = vec![task_with("RQ-0001", TaskStatus::Todo, vec![])];
    let queue = QueueFile { version: 1, tasks };

    let result = resolve_task_ids(&queue, &[], &[]);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No tasks specified"));
}

#[test]
fn resolve_task_ids_errors_on_no_matching_tags() {
    let tasks = vec![task_with(
        "RQ-0001",
        TaskStatus::Todo,
        vec!["rust".to_string()],
    )];
    let queue = QueueFile { version: 1, tasks };

    let result = resolve_task_ids(&queue, &[], &["python".to_string()]);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No tasks found with tags"));
}

#[test]
fn resolve_task_ids_deduplicates_results() {
    let tasks = vec![
        task_with("RQ-0001", TaskStatus::Todo, vec!["rust".to_string()]),
        task_with("RQ-0002", TaskStatus::Todo, vec!["rust".to_string()]),
    ];
    let queue = QueueFile { version: 1, tasks };

    let result =
        resolve_task_ids(&queue, &[], &["rust".to_string()]).expect("should resolve tasks");

    // Two different tasks with same tag, should return both
    assert_eq!(result.len(), 2);
}

// =============================================================================
// BatchOperationResult helper tests
// =============================================================================

#[test]
fn batch_operation_result_all_succeeded_true_when_no_failures() {
    let result = BatchOperationResult {
        total: 3,
        succeeded: 3,
        failed: 0,
        results: vec![],
    };

    assert!(result.all_succeeded());
    assert!(!result.has_failures());
}

#[test]
fn batch_operation_result_has_failures_true_when_failures() {
    let result = BatchOperationResult {
        total: 3,
        succeeded: 2,
        failed: 1,
        results: vec![],
    };

    assert!(!result.all_succeeded());
    assert!(result.has_failures());
}
