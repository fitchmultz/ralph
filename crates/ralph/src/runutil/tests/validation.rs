//! Queue-validation error classification tests for runutil.
//!
//! Purpose:
//! - Queue-validation error classification tests for runutil.
//!
//! Responsibilities:
//! - Verify user-facing validation failures are detected, including through error context layers.
//! - Guard against false positives for unrelated runtime failures.
//!
//! Non-scope:
//! - Revert-mode behavior.
//! - Runner backend error handling.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Validation checks are exercised through the public `is_queue_validation_error` surface.

#[test]
fn is_queue_validation_error_detects_relationship_errors() {
    let err = anyhow::anyhow!(
        "Invalid relates_to relationship: task RQ-0016 relates to non-existent task RQ-0029"
    );
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!(
        "Invalid blocks relationship: task RQ-0010 blocks non-existent task RQ-0099"
    );
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!(
        "Invalid duplicates relationship: task RQ-0005 duplicates non-existent task RQ-0001"
    );
    assert!(crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_detects_duplicate_id_errors() {
    let err =
        anyhow::anyhow!("Duplicate task ID detected: RQ-0001. Ensure each task has a unique ID.");
    assert!(crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_detects_self_reference_errors() {
    let err = anyhow::anyhow!("Self-reference in relates_to: task RQ-0001 relates to itself");
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Self-blocking detected: task RQ-0001 blocks itself");
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Self-duplication detected: task RQ-0001 duplicates itself");
    assert!(crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_detects_circular_blocking() {
    let err = anyhow::anyhow!("Circular blocking detected involving task RQ-0005");
    assert!(crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_detects_missing_field_errors() {
    let err = anyhow::anyhow!("Missing task ID: task at index 0 is missing an 'id' field");
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Missing task title: task RQ-0001 is missing a 'title' field");
    assert!(crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_detects_invalid_prefix_errors() {
    let err = anyhow::anyhow!("Invalid id_width: width must be greater than 0");
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Empty id_prefix: prefix is required");
    assert!(crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Unsupported queue.json version: 2. Ralph requires version 1");
    assert!(crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_rejects_non_validation_errors() {
    let err = anyhow::anyhow!("Runner exited with non-zero status");
    assert!(!crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Network timeout occurred");
    assert!(!crate::runutil::is_queue_validation_error(&err));

    let err = anyhow::anyhow!("Failed to acquire queue lock");
    assert!(!crate::runutil::is_queue_validation_error(&err));
}

#[test]
fn is_queue_validation_error_detects_through_context_layers() {
    let err = anyhow::anyhow!(
        "Invalid relates_to relationship: task RQ-0016 relates to non-existent task RQ-0029"
    )
    .context("loading phase 3 snapshot")
    .context("running task");

    assert!(crate::runutil::is_queue_validation_error(&err));
}
