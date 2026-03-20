//! Purpose: run-loop immediate-abort regression coverage for lock and validation failures.
//!
//! Responsibilities:
//! - Verify queue lock failures abort `run_loop()` immediately.
//! - Verify queue validation failures abort `run_loop()` immediately.
//! - Preserve the regression assertions that ensure the 50-failure retry loop is not hit.
//!
//! Scope:
//! - In-process run-loop error-path tests only.
//!
//! Usage:
//! - Tests use `support::setup_run_loop_fixture()` and `support::run_loop_once()` to share setup.
//!
//! Invariants/Assumptions:
//! - The queue fixture contents and `RunLoopOptions` must remain aligned with the pre-split monolith.
//! - Assertion text and timing thresholds remain unchanged.

use super::*;

#[test]
#[serial]
fn run_loop_aborts_immediately_on_queue_lock_error() -> Result<()> {
    let fixture = support::setup_run_loop_fixture(vec![])?;

    let _lock = queue::acquire_queue_lock(&fixture.repo_root, "test lock holder", false)?;

    let (result, elapsed) = support::run_loop_once(&fixture.resolved);

    let err = result.expect_err("expected run_loop to fail with lock error");
    let err_msg = format!("{:#}", err);

    anyhow::ensure!(
        err_msg.contains("Queue lock already held"),
        "expected 'Queue lock already held' in error: {err_msg}"
    );

    anyhow::ensure!(
        !err_msg.contains("50 consecutive failures"),
        "run loop hit 50-failure abort instead of returning immediately: {err_msg}"
    );

    anyhow::ensure!(
        elapsed < Duration::from_secs(1),
        "run loop took too long ({elapsed:?}), should have failed immediately"
    );

    Ok(())
}

/// Test that run loop aborts immediately on queue validation error without hitting the
/// 50-failure abort loop (regression test for invalid relates_to format).
///
/// This test verifies that when the queue has an invalid relationship reference
/// (e.g., relates_to pointing to a non-existent task), the run loop returns the
/// validation error immediately rather than retrying and eventually hitting
/// "aborting after 50 consecutive failures".
#[test]
#[serial]
fn run_loop_aborts_immediately_on_queue_validation_error() -> Result<()> {
    let fixture = support::setup_run_loop_fixture(vec!["RQ-9999".to_string()])?;

    let (result, elapsed) = support::run_loop_once(&fixture.resolved);

    let err = result.expect_err("expected run_loop to fail with validation error");
    let err_msg = format!("{:#}", err);

    anyhow::ensure!(
        err_msg.contains("relationship"),
        "expected 'relationship' in error: {err_msg}"
    );
    anyhow::ensure!(
        err_msg.contains("non-existent") || err_msg.contains("RQ-9999"),
        "expected 'non-existent' or task ID in error: {err_msg}"
    );

    anyhow::ensure!(
        !err_msg.contains("50 consecutive failures"),
        "run loop hit 50-failure abort instead of returning immediately: {err_msg}"
    );

    anyhow::ensure!(
        elapsed < Duration::from_secs(1),
        "run loop took too long ({elapsed:?}), should have failed immediately"
    );

    Ok(())
}
