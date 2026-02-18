//! Integration tests for merge-agent subprocess invocation and exit code handling.
//!
//! Responsibilities:
//! - Test merge-agent command line interface.
//! - Verify exit code classification for retry decisions.
//! - Test task finalization in coordinator queue context.
//!
//! Not handled here:
//! - Full parallel orchestration (see parallel_e2e_test.rs).
//! - State persistence (see parallel_state_recovery_test.rs).
//!
//! Invariants/assumptions:
//! - Tests use fake gh CLI to simulate PR operations.
//! - Tests run with env_lock to prevent PATH race conditions.
//! - Merge-agent always runs in coordinator repo context (CWD = repo root).

use anyhow::Result;

mod test_support;

// =============================================================================
// Test: Merge-Agent Success Path
// =============================================================================

/// Verify that merge-agent successfully finalizes a task when:
/// - PR exists and is merged
/// - Task is in doing state
/// - Coordinator queue/done are writable
#[test]
fn merge_agent_success_moves_task_to_done() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first (creates queue.json)
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0001",
        "Task to finalize",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Create fake gh that returns a merged PR
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0001", "--pr", "42"],
        )
    });

    // Merge-agent should succeed (exit 0 or 6 for already merged)
    let combined = format!("{}{}", stdout, stderr);
    eprintln!("Merge-agent output:\n{}", combined);

    // The exit code should be 0 (success) or 6 (already finalized - idempotent)
    let exit_code = status.code().unwrap_or(-1);
    assert!(
        exit_code == 0 || exit_code == 6,
        "Expected exit code 0 or 6, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    // Task should be moved to done
    let done = test_support::read_done(temp.path())?;
    assert!(
        done.tasks.iter().any(|t| t.id == "RQ-0001"),
        "Task RQ-0001 should be in done.json"
    );

    Ok(())
}

// =============================================================================
// Test: Merge-Agent Conflict Exit Code
// =============================================================================

/// Verify that merge-agent returns exit code 3 (MERGE_CONFLICT) when PR has conflicts.
#[test]
fn merge_agent_conflict_returns_exit_code_3() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first (creates queue.json)
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0002",
        "Conflict task",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Create fake gh that reports PR has conflicts
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    # Return open PR with mergeable = CONFLICTING
    echo '{"number":42,"state":"OPEN","merged":false,"mergeStateStatus":"DIRTY","headRefName":"ralph/RQ-0002","baseRefName":"main","isDraft":false}'
    exit 0
fi
if [[ "$1" == "pr" ]] && [[ "$2" == "merge" ]]; then
    echo "Merge failed: conflicts" >&2
    exit 1
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0002", "--pr", "42"],
        )
    });

    // Exit code should be 3 (MERGE_CONFLICT)
    let exit_code = status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 3,
        "Expected exit code 3 for conflict, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    // Task should NOT be in done (conflict means retryable, not finalized)
    let done = test_support::read_done(temp.path())?;
    assert!(
        done.tasks.is_empty() || !done.tasks.iter().any(|t| t.id == "RQ-0002"),
        "Task RQ-0002 should NOT be in done.json on conflict"
    );

    Ok(())
}

// =============================================================================
// Test: Merge-Agent Draft PR Exit Code
// =============================================================================

/// Verify that merge-agent returns exit code 5 (PR_IS_DRAFT) for draft PRs.
#[test]
fn merge_agent_draft_pr_returns_exit_code_5() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first (creates queue.json)
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0003",
        "Draft PR task",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Create fake gh that reports PR is draft
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"OPEN","merged":false,"mergeStateStatus":"CLEAN","headRefName":"ralph/RQ-0003","baseRefName":"main","isDraft":true}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0003", "--pr", "42"],
        )
    });

    // Exit code should be 5 (PR_IS_DRAFT)
    let exit_code = status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 5,
        "Expected exit code 5 for draft PR, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    // Task should NOT be in done
    let done = test_support::read_done(temp.path())?;
    assert!(
        done.tasks.is_empty() || !done.tasks.iter().any(|t| t.id == "RQ-0003"),
        "Task RQ-0003 should NOT be in done.json for draft PR"
    );

    Ok(())
}

// =============================================================================
// Test: Merge-Agent Already Finalized (Idempotent)
// =============================================================================

/// Verify that merge-agent returns exit code 6 (ALREADY_FINALIZED) for already-done tasks.
#[test]
fn merge_agent_already_finalized_returns_exit_code_6() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first (creates queue.json)
    test_support::ralph_init(temp.path())?;

    // Create task already in done.json
    let done_task = test_support::make_test_task(
        "RQ-0004",
        "Already done task",
        ralph::contracts::TaskStatus::Done,
    );
    test_support::write_done(temp.path(), std::slice::from_ref(&done_task))?;

    // Empty queue (write after ralph_init to ensure it's clean)
    test_support::write_queue(temp.path(), &[])?;

    // Create fake gh that reports PR is merged
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent on already-finalized task with PATH prepended
    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0004", "--pr", "42"],
        )
    });

    // Exit code should be 6 (ALREADY_FINALIZED)
    let exit_code = status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 6,
        "Expected exit code 6 for already finalized, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    // Should have exactly one entry in done (no duplicates)
    let done = test_support::read_done(temp.path())?;
    let count = done.tasks.iter().filter(|t| t.id == "RQ-0004").count();
    assert_eq!(
        count, 1,
        "Should have exactly one entry for RQ-0004 in done.json, got {}",
        count
    );

    Ok(())
}

// =============================================================================
// Test: Merge-Agent Validation Error Exit Code
// =============================================================================

/// Verify that merge-agent returns exit code 2 for validation errors (invalid task ID).
#[test]
fn merge_agent_invalid_task_id_returns_exit_code_2() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project
    test_support::ralph_init(temp.path())?;

    // Run merge-agent with invalid task ID (contains special chars)
    let (status, stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &["run", "merge-agent", "--task", "INVALID/TASK", "--pr", "42"],
    );

    // Exit code should be 2 (VALIDATION_FAILURE)
    let exit_code = status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 2,
        "Expected exit code 2 for validation failure, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    Ok(())
}

// =============================================================================
// Test: Merge-Agent Zero PR Number Validation
// =============================================================================

/// Verify that merge-agent returns exit code 2 for PR number 0.
#[test]
fn merge_agent_zero_pr_number_returns_exit_code_2() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project
    test_support::ralph_init(temp.path())?;

    // Run merge-agent with PR number 0
    let (status, stdout, stderr) = test_support::run_in_dir(
        temp.path(),
        &["run", "merge-agent", "--task", "RQ-0001", "--pr", "0"],
    );

    // Exit code should be 2 (VALIDATION_FAILURE)
    let exit_code = status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 2,
        "Expected exit code 2 for PR=0 validation failure, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    Ok(())
}

// =============================================================================
// Test: Merge-Agent PR Not Found Exit Code
// =============================================================================

/// Verify that merge-agent returns exit code 4 (PR_NOT_FOUND) for closed/unmerged PRs.
#[test]
fn merge_agent_closed_pr_returns_exit_code_4() -> Result<()> {
    let _lock = test_support::env_lock().lock().unwrap();
    let temp = test_support::temp_dir_outside_repo();

    // Setup git repo
    test_support::git_init(temp.path())?;

    // Init ralph project first (creates queue.json)
    test_support::ralph_init(temp.path())?;

    // Create task in doing state (write AFTER ralph_init)
    let tasks = vec![test_support::make_test_task(
        "RQ-0005",
        "Closed PR task",
        ralph::contracts::TaskStatus::Doing,
    )];
    test_support::write_queue(temp.path(), &tasks)?;

    // Create fake gh that reports PR is closed (not merged)
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let gh_script = r#"#!/bin/bash
if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    echo '{"number":42,"state":"CLOSED","merged":false,"mergeStateStatus":"CLEAN"}'
    exit 0
fi
exit 0
"#;
    let _gh_path = test_support::create_executable_script(&bin_dir, "gh", gh_script)?;

    // Run merge-agent with PATH prepended
    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        test_support::run_in_dir(
            temp.path(),
            &["run", "merge-agent", "--task", "RQ-0005", "--pr", "42"],
        )
    });

    // Exit code should be 4 (PR_NOT_FOUND)
    let exit_code = status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 4,
        "Expected exit code 4 for closed PR, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        exit_code
    );

    // Task should NOT be in done
    let done = test_support::read_done(temp.path())?;
    assert!(
        done.tasks.is_empty() || !done.tasks.iter().any(|t| t.id == "RQ-0005"),
        "Task RQ-0005 should NOT be in done.json for closed PR"
    );

    Ok(())
}
