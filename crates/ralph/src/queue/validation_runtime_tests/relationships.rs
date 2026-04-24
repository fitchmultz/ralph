//! Relationship validation runtime tests.
//!
//! Purpose:
//! - Relationship validation runtime tests.
//!
//! Responsibilities:
//! - Cover `blocks`, `relates_to`, and `duplicates` validation rules.
//! - Verify self-reference, missing-target, and cycle semantics.
//! - Keep relationship-specific warnings separate from dependency coverage.
//!
//! Not handled here:
//! - Parent chain validation.
//! - Required-field or archive validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Relationship tests operate on active queue state unless done/archive behavior matters.
//! - Duplicate-to-done warnings stay non-fatal.

use crate::contracts::{QueueFile, TaskStatus};

use super::support::{task_with, task_with_relationships};
use crate::queue::validation::validate_queue_set;

#[test]
fn validate_rejects_self_blocking() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["RQ-0001".to_string()],
            vec![],
            None,
        )],
    };

    let err =
        validate_queue_set(&active, None, "RQ", 4, 10).expect_err("Should error on self-blocking");
    assert!(
        format!("{err:#}").contains("Self-blocking"),
        "Error should mention self-blocking: {err:#}"
    );
}

#[test]
fn validate_rejects_self_relates_to() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec![],
            vec!["RQ-0001".to_string()],
            None,
        )],
    };

    let err = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect_err("Should error on self-relates_to");
    assert!(
        format!("{err:#}").contains("Self-reference"),
        "Error should mention self-reference: {err:#}"
    );
}

#[test]
fn validate_rejects_self_duplication() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec![],
            vec![],
            Some("RQ-0001".to_string()),
        )],
    };

    let err = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect_err("Should error on self-duplication");
    assert!(
        format!("{err:#}").contains("Self-duplication"),
        "Error should mention self-duplication: {err:#}"
    );
}

#[test]
fn validate_rejects_blocks_to_nonexistent_task() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["RQ-9999".to_string()],
                vec![],
                None,
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let err = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect_err("Should error on blocks to non-existent task");
    assert!(
        format!("{err:#}").contains("non-existent"),
        "Error should mention non-existent task: {err:#}"
    );
}

#[test]
fn validate_rejects_relates_to_nonexistent_task() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec![],
                vec!["RQ-9999".to_string()],
                None,
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let err = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect_err("Should error on relates_to non-existent task");
    assert!(
        format!("{err:#}").contains("non-existent"),
        "Error should mention non-existent task: {err:#}"
    );
}

#[test]
fn validate_rejects_duplicates_nonexistent_task() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec![],
                vec![],
                Some("RQ-9999".to_string()),
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let err = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect_err("Should error on duplicates non-existent task");
    assert!(
        format!("{err:#}").contains("non-existent"),
        "Error should mention non-existent task: {err:#}"
    );
}

#[test]
fn validate_rejects_circular_blocking() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["RQ-0002".to_string()],
                vec![],
                None,
            ),
            task_with_relationships(
                "RQ-0002",
                TaskStatus::Todo,
                vec!["RQ-0001".to_string()],
                vec![],
                None,
            ),
        ],
    };

    let err = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect_err("Should error on circular blocking");
    assert!(
        format!("{err:#}").contains("Circular blocking"),
        "Error should mention circular blocking: {err:#}"
    );
}

#[test]
fn validate_warns_on_duplicate_of_done_task() {
    let mut done_task = task_with("RQ-0002", TaskStatus::Done, vec![]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec![],
            vec![],
            Some("RQ-0002".to_string()),
        )],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };

    let warnings = validate_queue_set(&active, Some(&done), "RQ", 4, 10)
        .expect("Should not error on duplicate of done task");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("done")),
        "Should warn about duplicate of done task: {:?}",
        warnings
    );
}

#[test]
fn validate_allows_valid_relationships() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["RQ-0002".to_string()],
                vec!["RQ-0003".to_string()],
                None,
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
            task_with_relationships("RQ-0003", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let warnings = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect("Should not error on valid relationships");
    assert!(
        warnings.is_empty(),
        "Should have no warnings for valid relationships: {:?}",
        warnings
    );
}
