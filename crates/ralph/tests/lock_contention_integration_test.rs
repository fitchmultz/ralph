//! Integration tests for lock contention handling and cleanup.
//!
//! Responsibilities:
//! - Validate contention errors when another process holds the queue lock.
//! - Verify error messages include lock path context.
//! - Ensure lock errors abort immediately without spinning the 50-failure loop.
//!
//! Not covered here:
//! - Stale lock cleanup behavior (see `stale_lock_cleanup_test.rs`).
//! - Temp directory helpers or atomic writes.
//!
//! Invariants/assumptions:
//! - Subprocess-based lock holder signals readiness before contention check.
//! - Lock directory path is stable under the temp repo.

mod test_support;

use anyhow::{Context, Result};
use ralph::{lock, queue};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn current_exe() -> PathBuf {
    std::env::current_exe().expect("resolve current test executable path")
}

#[test]
fn lock_holder_process() -> Result<()> {
    if std::env::var("RALPH_TEST_LOCK_HOLD").ok().as_deref() != Some("1") {
        return Ok(());
    }

    let repo_root = std::env::var("RALPH_TEST_REPO_ROOT").context("read RALPH_TEST_REPO_ROOT")?;
    let repo_root = PathBuf::from(repo_root);

    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let label =
        std::env::var("RALPH_TEST_LOCK_LABEL").unwrap_or_else(|_| "lock holder".to_string());

    let _lock = queue::acquire_queue_lock(&repo_root, &label, false)?;
    println!("LOCK_HELD");
    let _ = std::io::stdout().flush();

    // Parent closes stdin when it's done asserting contention behavior.
    let mut stdin = std::io::stdin();
    let mut buf = [0u8; 1];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    Ok(())
}

#[test]
fn lock_contention_blocks_second_process() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let mut child = Command::new(current_exe())
        .arg("--exact")
        .arg("lock_holder_process")
        .arg("--nocapture")
        .env("RALPH_TEST_LOCK_HOLD", "1")
        .env("RALPH_TEST_REPO_ROOT", &repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawn lock holder process")?;

    let child_stdin = child.stdin.take().context("capture lock holder stdin")?;
    let stdout = child.stdout.take().context("capture lock holder stdout")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line.clone()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let got_signal =
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            while let Ok(line) = rx.try_recv() {
                if line.contains("LOCK_HELD") {
                    return true;
                }
            }
            false
        });
    anyhow::ensure!(got_signal, "lock holder did not signal readiness");

    let err = queue::acquire_queue_lock(&repo_root, "contender", false).unwrap_err();
    let msg = format!("{err:#}");
    let lock_dir = lock::queue_lock_dir(&repo_root);

    anyhow::ensure!(
        msg.contains(lock_dir.to_string_lossy().as_ref()),
        "expected lock path in error: {msg}"
    );

    drop(child_stdin);
    let _ = child.wait();

    let _ = std::fs::remove_dir_all(&lock_dir);

    Ok(())
}

/// Test that a parallel run loop prevents another run loop from starting.
///
/// This validates the concurrency contract: the queue lock is held for the
/// entire parallel run loop duration, preventing duplicate task selection.
#[test]
fn parallel_supervisor_prevents_second_supervisor() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    // Spawn a subprocess that acquires the queue lock with "run loop" label
    let mut child = Command::new(current_exe())
        .arg("--exact")
        .arg("lock_holder_process")
        .arg("--nocapture")
        .env("RALPH_TEST_LOCK_HOLD", "1")
        .env("RALPH_TEST_REPO_ROOT", &repo_root)
        .env("RALPH_TEST_LOCK_LABEL", "run loop")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawn lock holder process")?;

    let child_stdin = child.stdin.take().context("capture lock holder stdin")?;
    let stdout = child.stdout.take().context("capture lock holder stdout")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line.clone()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let got_signal =
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            while let Ok(line) = rx.try_recv() {
                if line.contains("LOCK_HELD") {
                    return true;
                }
            }
            false
        });
    anyhow::ensure!(got_signal, "lock holder did not signal readiness");

    // Attempt to acquire the queue lock - should fail with contention
    let err = queue::acquire_queue_lock(&repo_root, "run loop", false).unwrap_err();
    let msg = format!("{err:#}");
    let lock_dir = lock::queue_lock_dir(&repo_root);

    // Verify the error message includes the lock path
    anyhow::ensure!(
        msg.contains(lock_dir.to_string_lossy().as_ref()),
        "expected lock path in error: {msg}"
    );

    // Verify the error message indicates a supervising process holds the lock
    anyhow::ensure!(
        msg.contains("run loop") || msg.contains("already held"),
        "expected 'run loop' or 'already held' in error: {msg}"
    );

    drop(child_stdin);
    let _ = child.wait();

    let _ = std::fs::remove_dir_all(&lock_dir);

    Ok(())
}

/// Test that run loop aborts immediately on queue lock error without hitting the
/// 50-failure abort loop (regression test for RQ-0643).
///
/// This test verifies that when the queue lock is held by a live process,
/// the run loop returns the lock error immediately rather than retrying
/// and eventually hitting "aborting after 50 consecutive failures".
#[test]
fn run_loop_aborts_immediately_on_queue_lock_error() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    // Create a minimal queue with a todo task
    let queue = ralph::contracts::QueueFile {
        version: 1,
        tasks: vec![ralph::contracts::Task {
            id: "RQ-0001".to_string(),
            status: ralph::contracts::TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
            priority: ralph::contracts::TaskPriority::Medium,
            tags: vec![],
            scope: vec!["src/main.rs".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-02-06T00:00:00Z".to_string()),
            updated_at: Some("2026-02-06T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: Default::default(),
            parent_id: None,
        }],
    };
    let queue_path = repo_root.join(".ralph/queue.jsonc");
    let done_path = repo_root.join(".ralph/done.jsonc");
    ralph::queue::save_queue(&queue_path, &queue)?;
    ralph::queue::save_queue(&done_path, &ralph::contracts::QueueFile::default())?;

    // Acquire and hold the queue lock in-process
    let _lock = ralph::queue::acquire_queue_lock(&repo_root, "test lock holder", false)?;

    // Set up resolved config
    let resolved = ralph::config::Resolved {
        config: ralph::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path: queue_path.clone(),
        done_path: done_path.clone(),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
    };

    // Attempt to run the loop - should fail immediately with lock error
    let start = std::time::Instant::now();
    let result = ralph::commands::run::run_loop(
        &resolved,
        ralph::commands::run::RunLoopOptions {
            max_tasks: 0,
            agent_overrides: ralph::agent::AgentOverrides::default(),
            force: false,
            auto_resume: false,
            starting_completed: 0,
            non_interactive: true,
            parallel_workers: None,
            wait_when_blocked: false,
            wait_poll_ms: 1000,
            wait_timeout_seconds: 0,
            notify_when_unblocked: false,
            wait_when_empty: false,
            empty_poll_ms: 30_000,
        },
    );
    let elapsed = start.elapsed();

    // Verify the error
    let err = result.expect_err("expected run_loop to fail with lock error");
    let err_msg = format!("{:#}", err);

    // Error should contain "Queue lock already held"
    anyhow::ensure!(
        err_msg.contains("Queue lock already held"),
        "expected 'Queue lock already held' in error: {err_msg}"
    );

    // Error should NOT contain "50 consecutive failures" - this would indicate
    // the run loop was retrying instead of aborting immediately
    anyhow::ensure!(
        !err_msg.contains("50 consecutive failures"),
        "run loop hit 50-failure abort instead of returning immediately: {err_msg}"
    );

    // Should have failed quickly (under 1 second), not after retries
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
fn run_loop_aborts_immediately_on_queue_validation_error() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    // Create a queue with an invalid relates_to reference (RQ-9999 doesn't exist)
    let queue = ralph::contracts::QueueFile {
        version: 1,
        tasks: vec![ralph::contracts::Task {
            id: "RQ-0001".to_string(),
            status: ralph::contracts::TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
            priority: ralph::contracts::TaskPriority::Medium,
            tags: vec![],
            scope: vec!["src/main.rs".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-02-06T00:00:00Z".to_string()),
            updated_at: Some("2026-02-06T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec!["RQ-9999".to_string()], // Non-existent task
            duplicates: None,
            custom_fields: Default::default(),
            parent_id: None,
        }],
    };
    let queue_path = repo_root.join(".ralph/queue.jsonc");
    let done_path = repo_root.join(".ralph/done.jsonc");
    ralph::queue::save_queue(&queue_path, &queue)?;
    ralph::queue::save_queue(&done_path, &ralph::contracts::QueueFile::default())?;

    // Set up resolved config
    let resolved = ralph::config::Resolved {
        config: ralph::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path: queue_path.clone(),
        done_path: done_path.clone(),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
    };

    // Attempt to run the loop - should fail immediately with validation error
    let start = std::time::Instant::now();
    let result = ralph::commands::run::run_loop(
        &resolved,
        ralph::commands::run::RunLoopOptions {
            max_tasks: 0,
            agent_overrides: ralph::agent::AgentOverrides::default(),
            force: false,
            auto_resume: false,
            starting_completed: 0,
            non_interactive: true,
            parallel_workers: None,
            wait_when_blocked: false,
            wait_poll_ms: 1000,
            wait_timeout_seconds: 0,
            notify_when_unblocked: false,
            wait_when_empty: false,
            empty_poll_ms: 30_000,
        },
    );
    let elapsed = start.elapsed();

    // Verify the error
    let err = result.expect_err("expected run_loop to fail with validation error");
    let err_msg = format!("{:#}", err);

    // Error should contain "relationship" and "non-existent"
    anyhow::ensure!(
        err_msg.contains("relationship"),
        "expected 'relationship' in error: {err_msg}"
    );
    anyhow::ensure!(
        err_msg.contains("non-existent") || err_msg.contains("RQ-9999"),
        "expected 'non-existent' or task ID in error: {err_msg}"
    );

    // Error should NOT contain "50 consecutive failures" - this would indicate
    // the run loop was retrying instead of aborting immediately
    anyhow::ensure!(
        !err_msg.contains("50 consecutive failures"),
        "run loop hit 50-failure abort instead of returning immediately: {err_msg}"
    );

    // Should have failed quickly (under 1 second), not after retries
    anyhow::ensure!(
        elapsed < Duration::from_secs(1),
        "run loop took too long ({elapsed:?}), should have failed immediately"
    );

    Ok(())
}
