//! Integration tests for `ralph task edit` auto-archive behavior.
//!
//! Purpose:
//! - Integration tests for `ralph task edit` auto-archive behavior.
//!
//! Responsibilities:
//! - Verify that task edit auto-archives terminal tasks when configured.
//! - Verify that archived task IDs are listed in output.
//! - Verify that --no-auto-archive flag prevents archiving.
//!
//! Not handled here:
//! - Unit testing of archive internals (covered by module/unit tests).
//! - Testing of other edit functionality.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `ralph init --force --non-interactive` creates a usable `.ralph/` structure.
//! - Auto-archive respects auto_archive_terminal_after_days config setting.

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

#[test]
fn task_edit_lists_archived_task_ids_in_output() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create a config with auto_archive_terminal_after_days = 0 (immediate)
    let config = r#"{
        "version": 2,
        "queue": {
            "auto_archive_terminal_after_days": 0
        }
    }"#;
    std::fs::write(dir.path().join(".ralph/config.jsonc"), config)?;

    // Create tasks: one todo (to edit), two terminal (to be archived)
    let todo_task = test_support::make_test_task("RQ-0001", "Todo task", TaskStatus::Todo);
    let mut done_task = test_support::make_test_task("RQ-0002", "Done task", TaskStatus::Done);
    let mut rejected_task =
        test_support::make_test_task("RQ-0003", "Rejected task", TaskStatus::Rejected);

    // Ensure terminal tasks have completed_at timestamps
    done_task.completed_at = Some("2026-01-01T00:00:00Z".to_string());
    rejected_task.completed_at = Some("2026-01-02T00:00:00Z".to_string());

    test_support::write_queue(dir.path(), &[todo_task, done_task, rejected_task])?;
    test_support::write_done(dir.path(), &[])?;

    // Run edit command
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "edit", "title", "Updated title", "RQ-0001"],
    );
    anyhow::ensure!(
        status.success(),
        "task edit failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify output contains archived task IDs
    let output = format!("{}\n{}", stdout, stderr);
    anyhow::ensure!(
        output.contains("Auto-archived"),
        "expected 'Auto-archived' in output, got:\n{output}"
    );
    anyhow::ensure!(
        output.contains("RQ-0002"),
        "expected 'RQ-0002' in output, got:\n{output}"
    );
    anyhow::ensure!(
        output.contains("RQ-0003"),
        "expected 'RQ-0003' in output, got:\n{output}"
    );

    // Verify tasks were actually archived
    let queue = test_support::read_queue(dir.path())?;
    let done = test_support::read_done(dir.path())?;

    // Only the edited task should remain in queue
    anyhow::ensure!(
        queue.tasks.len() == 1 && queue.tasks[0].id == "RQ-0001",
        "expected only RQ-0001 in queue, got: {:?}",
        queue.tasks.iter().map(|t| &t.id).collect::<Vec<_>>()
    );

    // Terminal tasks should be in done
    anyhow::ensure!(
        done.tasks.iter().any(|t| t.id == "RQ-0002"),
        "RQ-0002 should be in done.json"
    );
    anyhow::ensure!(
        done.tasks.iter().any(|t| t.id == "RQ-0003"),
        "RQ-0003 should be in done.json"
    );

    Ok(())
}

#[test]
fn task_edit_no_auto_archive_flag_prevents_archiving() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create a config with auto_archive_terminal_after_days = 0 (immediate)
    let config = r#"{
        "version": 2,
        "queue": {
            "auto_archive_terminal_after_days": 0
        }
    }"#;
    std::fs::write(dir.path().join(".ralph/config.jsonc"), config)?;

    // Create tasks: one todo (to edit), two terminal (should NOT be archived)
    let todo_task = test_support::make_test_task("RQ-0001", "Todo task", TaskStatus::Todo);
    let mut done_task = test_support::make_test_task("RQ-0002", "Done task", TaskStatus::Done);
    let mut rejected_task =
        test_support::make_test_task("RQ-0003", "Rejected task", TaskStatus::Rejected);

    // Ensure terminal tasks have completed_at timestamps
    done_task.completed_at = Some("2026-01-01T00:00:00Z".to_string());
    rejected_task.completed_at = Some("2026-01-02T00:00:00Z".to_string());

    test_support::write_queue(dir.path(), &[todo_task, done_task, rejected_task])?;
    test_support::write_done(dir.path(), &[])?;

    // Run edit command with --no-auto-archive flag
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "edit",
            "--no-auto-archive",
            "title",
            "Updated title",
            "RQ-0001",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "task edit with --no-auto-archive failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify output does NOT contain auto-archive message
    let output = format!("{}\n{}", stdout, stderr);
    anyhow::ensure!(
        !output.contains("Auto-archived"),
        "expected no 'Auto-archived' in output when using --no-auto-archive, got:\n{output}"
    );

    // Verify tasks were NOT archived
    let queue = test_support::read_queue(dir.path())?;
    let done = test_support::read_done(dir.path())?;

    // All tasks should still be in queue
    anyhow::ensure!(
        queue.tasks.len() == 3,
        "expected 3 tasks in queue with --no-auto-archive, got: {}",
        queue.tasks.len()
    );

    // Done should be empty
    anyhow::ensure!(
        done.tasks.is_empty(),
        "expected done.json to be empty with --no-auto-archive, got: {:?}",
        done.tasks.iter().map(|t| &t.id).collect::<Vec<_>>()
    );

    Ok(())
}

#[test]
fn task_edit_no_archive_message_when_no_terminal_tasks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create a config with auto_archive_terminal_after_days = 0 (immediate)
    let config = r#"{
        "version": 2,
        "queue": {
            "auto_archive_terminal_after_days": 0
        }
    }"#;
    std::fs::write(dir.path().join(".ralph/config.jsonc"), config)?;

    // Create only non-terminal tasks
    let todo_task = test_support::make_test_task("RQ-0001", "Todo task", TaskStatus::Todo);
    let doing_task = test_support::make_test_task("RQ-0002", "Doing task", TaskStatus::Doing);

    test_support::write_queue(dir.path(), &[todo_task, doing_task])?;
    test_support::write_done(dir.path(), &[])?;

    // Run edit command
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "edit", "title", "Updated title", "RQ-0001"],
    );
    anyhow::ensure!(
        status.success(),
        "task edit failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify output does NOT contain auto-archive message when no tasks to archive
    let output = format!("{}\n{}", stdout, stderr);
    anyhow::ensure!(
        !output.contains("Auto-archived"),
        "expected no 'Auto-archived' when no terminal tasks, got:\n{output}"
    );

    Ok(())
}

#[test]
fn task_edit_help_includes_no_auto_archive_flag() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "edit", "--help"]);
    anyhow::ensure!(
        status.success(),
        "task edit --help failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let output = format!("{}\n{}", stdout, stderr);

    // Verify --no-auto-archive flag is documented
    anyhow::ensure!(
        output.contains("--no-auto-archive"),
        "expected --no-auto-archive in help output, got:\n{output}"
    );

    // Verify help mentions auto-archive side effect
    anyhow::ensure!(
        output.contains("auto-archive") || output.contains("auto_archive"),
        "expected auto-archive mention in help output, got:\n{output}"
    );

    Ok(())
}
