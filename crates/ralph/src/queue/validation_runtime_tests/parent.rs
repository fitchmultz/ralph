//! Parent relationship validation runtime tests.
//!
//! Purpose:
//! - Parent relationship validation runtime tests.
//!
//! Responsibilities:
//! - Cover `parent_id` warnings and cycle errors.
//! - Verify parent lookup across active queue and done archive.
//! - Keep parent-chain semantics isolated from other relationships.
//!
//! Not handled here:
//! - Dependency graph validation.
//! - JSON deserialization coverage.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing or self-parent cases are warnings, not hard errors.
//! - True parent cycles remain validation errors.

use crate::contracts::{QueueFile, TaskStatus};

use super::support::task_with_parent;
use crate::queue::validation::validate_queue_set;

#[test]
fn validate_warns_on_missing_parent() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_parent("RQ-0001", None),
            task_with_parent("RQ-0002", Some("RQ-9999")),
        ],
    };

    let warnings =
        validate_queue_set(&active, None, "RQ", 4, 10).expect("Should not error on missing parent");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.task_id == "RQ-0002"
                && warning.message.contains("does not exist")),
        "Should warn about missing parent: {:?}",
        warnings
    );
}

#[test]
fn validate_warns_on_self_parent() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_parent("RQ-0001", Some("RQ-0001"))],
    };

    let warnings =
        validate_queue_set(&active, None, "RQ", 4, 10).expect("Should not error on self-parent");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.task_id == "RQ-0001" && warning.message.contains("itself")),
        "Should warn about self-parent: {:?}",
        warnings
    );
}

#[test]
fn validate_errors_on_parent_cycle() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_parent("RQ-0001", Some("RQ-0002")),
            task_with_parent("RQ-0002", Some("RQ-0001")),
        ],
    };

    let err =
        validate_queue_set(&active, None, "RQ", 4, 10).expect_err("Should error on parent cycle");
    assert!(
        err.to_string().contains("Circular parent chain"),
        "Error should mention circular parent chain: {err}"
    );
}

#[test]
fn validate_ignores_whitespace_parent_id() {
    let mut child = task_with_parent("RQ-0001", None);
    child.parent_id = Some("   ".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![child],
    };

    let warnings = validate_queue_set(&active, None, "RQ", 4, 10)
        .expect("Should not error on whitespace parent_id");
    assert!(
        warnings.is_empty(),
        "Should not warn about whitespace parent_id: {:?}",
        warnings
    );
}

#[test]
fn validate_accepts_valid_parent() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_parent("RQ-0001", None),
            task_with_parent("RQ-0002", Some("RQ-0001")),
        ],
    };

    let warnings =
        validate_queue_set(&active, None, "RQ", 4, 10).expect("Should not error on valid parent");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.message.contains("parent")),
        "Should not warn about valid parent: {:?}",
        warnings
    );
}

#[test]
fn validate_finds_parent_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_parent("RQ-0002", Some("RQ-0001"))],
    };
    let mut parent = task_with_parent("RQ-0001", None);
    parent.status = TaskStatus::Done;
    parent.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![parent],
    };

    let warnings = validate_queue_set(&active, Some(&done), "RQ", 4, 10)
        .expect("Should not error when parent is in done");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.message.contains("does not exist")),
        "Should not warn when parent exists in done: {:?}",
        warnings
    );
}
