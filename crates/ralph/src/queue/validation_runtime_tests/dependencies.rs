//! Dependency validation runtime tests.
//!
//! Purpose:
//! - Dependency validation runtime tests.
//!
//! Responsibilities:
//! - Cover dependency warning semantics and chain-depth limits.
//! - Verify rejected/done dependency handling across queue and done sets.
//! - Keep dependency-chain expectations isolated from relationship tests.
//!
//! Not handled here:
//! - Core required-field validation.
//! - `blocks`, `relates_to`, `duplicates`, or parent validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Dependency warnings do not escalate to validation errors in these cases.
//! - Task builders encode only the dependency edges under test.

use crate::contracts::{QueueFile, TaskStatus};

use super::support::{task_with, task_with_deps};
use crate::queue::validation::validate_queue_set;

#[test]
fn validate_warns_on_dependency_to_rejected_task() {
    let mut rejected = task_with("RQ-0002", TaskStatus::Rejected, vec![]);
    rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            rejected,
        ],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![],
    };

    let warnings = validate_queue_set(&active, Some(&done), "RQ", 4, 10)
        .expect("Should not error on rejected dependency");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.task_id == "RQ-0001" && warning.message.contains("rejected")),
        "Should warn about dependency on rejected task"
    );
}

#[test]
fn validate_warns_on_deep_dependency_chain() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            task_with_deps("RQ-0003", TaskStatus::Todo, vec!["RQ-0004".to_string()]),
            task_with_deps("RQ-0004", TaskStatus::Todo, vec!["RQ-0005".to_string()]),
            task_with_deps("RQ-0005", TaskStatus::Todo, vec!["RQ-0006".to_string()]),
            task_with_deps("RQ-0006", TaskStatus::Todo, vec!["RQ-0007".to_string()]),
            task_with_deps("RQ-0007", TaskStatus::Todo, vec!["RQ-0008".to_string()]),
            task_with_deps("RQ-0008", TaskStatus::Todo, vec!["RQ-0009".to_string()]),
            task_with_deps("RQ-0009", TaskStatus::Todo, vec!["RQ-0010".to_string()]),
            task_with_deps("RQ-0010", TaskStatus::Todo, vec!["RQ-0011".to_string()]),
            task_with_deps("RQ-0011", TaskStatus::Todo, vec!["RQ-0012".to_string()]),
            task_with_deps("RQ-0012", TaskStatus::Todo, vec![]),
        ],
    };

    let warnings =
        validate_queue_set(&active, None, "RQ", 4, 10).expect("Should not error on deep chain");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("depth")),
        "Should warn about deep dependency chain: {:?}",
        warnings
    );
}

#[test]
fn validate_allows_shallow_dependency_chain() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            task_with_deps("RQ-0003", TaskStatus::Todo, vec![]),
        ],
    };

    let warnings =
        validate_queue_set(&active, None, "RQ", 4, 10).expect("Should not error on shallow chain");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.message.contains("depth")),
        "Should not warn about shallow dependency chain"
    );
}

#[test]
fn validate_warns_on_blocked_dependency_chain() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            task_with_deps("RQ-0003", TaskStatus::Todo, vec![]),
        ],
    };

    let warnings =
        validate_queue_set(&active, None, "RQ", 4, 10).expect("Should not error on blocked chain");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("blocked")),
        "Should warn about blocked dependency chain: {:?}",
        warnings
    );
}

#[test]
fn validate_allows_unblocked_chain_with_done_task() {
    let mut done_task = task_with("RQ-0003", TaskStatus::Done, vec![]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
        ],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };

    let warnings = validate_queue_set(&active, Some(&done), "RQ", 4, 10)
        .expect("Should not error on unblocked chain");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.message.contains("blocked")),
        "Should not warn about unblocked dependency chain: {:?}",
        warnings
    );
}

#[test]
fn validate_detects_transitive_rejected_dependency() {
    let mut rejected = task_with("RQ-0003", TaskStatus::Rejected, vec![]);
    rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            rejected,
        ],
    };

    let warnings = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect("Should not error on rejected dependency");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("rejected")
                || warning.message.contains("blocked")),
        "Should warn about rejected or blocked dependency: {:?}",
        warnings
    );
}

#[test]
fn validate_no_warnings_for_valid_dependencies() {
    let mut done_task = task_with("RQ-0002", TaskStatus::Done, vec![]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_deps(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["RQ-0002".to_string()],
        )],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };

    let warnings = validate_queue_set(&active, Some(&done), "RQ", 4, 10)
        .expect("Should not error on valid dependencies");
    assert!(
        warnings.is_empty(),
        "Should have no warnings for valid dependencies: {:?}",
        warnings
    );
}
