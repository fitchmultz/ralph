//! Regression tests for parallel cleanup guard resource teardown.
//!
//! Purpose:
//! - Regression tests for parallel cleanup guard resource teardown.
//!
//! Responsibilities:
//! - Verify cleanup kills workers and persists terminal state correctly.
//! - Ensure Drop-triggered cleanup and disarm semantics stay intact.
//! - Confirm blocked workspaces are retained for explicit retry.
//!
//! Not handled here:
//! - Parallel worker execution logic.
//! - State-file schema serialization details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Cleanup remains idempotent and best-effort.
//! - Disarmed guards must not terminate workers on drop.

use super::*;
use crate::commands::run::parallel::worker::start_worker_monitor;
use crate::lock;
use std::process::{Child, Command};
use tempfile::TempDir;

fn create_test_guard(temp: &TempDir) -> ParallelCleanupGuard {
    let workspace_root = temp.path().join("workspaces");
    std::fs::create_dir_all(&workspace_root).unwrap();

    let state_path = temp.path().join("state.json");
    let state_file =
        state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

    ParallelCleanupGuard::new_simple(state_path, state_file, workspace_root)
}

fn register_sleeping_worker(
    guard: &mut ParallelCleanupGuard,
    temp: &TempDir,
    task_id: &str,
) -> Result<u32> {
    let child: Child = Command::new("sleep").arg("10").spawn()?;
    let pid = child.id();
    let workspace_path = temp.path().join("workspaces").join(task_id);
    std::fs::create_dir_all(&workspace_path)?;

    let worker = start_worker_monitor(
        task_id,
        "Test task".to_string(),
        WorkspaceSpec {
            path: workspace_path,
            branch: "main".to_string(),
        },
        child,
        guard.worker_event_sender(),
    );
    guard.register_worker(task_id.to_string(), worker);
    Ok(pid)
}

#[cfg(unix)]
fn kill_test_process(pid: u32) {
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn kill_test_process(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

#[cfg(all(not(unix), not(windows)))]
fn kill_test_process(_pid: u32) {}

#[test]
fn guard_cleanup_kills_worker_and_clears_state() -> Result<()> {
    let temp = TempDir::new()?;
    let mut guard = create_test_guard(&temp);

    let pid = register_sleeping_worker(&mut guard, &temp, "RQ-0001")?;
    let workspace_path = temp.path().join("workspaces").join("RQ-0001");
    guard
        .state_file_mut()
        .upsert_worker(state::WorkerRecord::new(
            "RQ-0001",
            workspace_path.clone(),
            "2026-02-20T00:00:00Z".to_string(),
        ));

    assert_eq!(
        lock::pid_is_running(pid),
        Some(true),
        "Worker should be running before cleanup"
    );

    guard.cleanup()?;

    let running = lock::pid_is_running(pid);
    assert!(
        running == Some(false) || running.is_none(),
        "Worker should be terminated after cleanup, got: {:?}",
        running
    );

    assert!(
        guard.state_file.workers.is_empty(),
        "workers should be empty after cleanup"
    );

    Ok(())
}

#[test]
fn guard_disarm_prevents_cleanup() -> Result<()> {
    let temp = TempDir::new()?;
    let mut guard = create_test_guard(&temp);

    let pid = register_sleeping_worker(&mut guard, &temp, "RQ-0001")?;

    guard.mark_completed();
    drop(guard);

    assert_eq!(
        lock::pid_is_running(pid),
        Some(true),
        "Worker should still be running after disarmed drop"
    );

    kill_test_process(pid);

    Ok(())
}

#[test]
fn guard_cleanup_is_idempotent() -> Result<()> {
    let temp = TempDir::new()?;
    let mut guard = create_test_guard(&temp);

    let pid = register_sleeping_worker(&mut guard, &temp, "RQ-0001")?;

    guard.cleanup()?;

    let running = lock::pid_is_running(pid);
    assert!(
        running == Some(false) || running.is_none(),
        "Worker should be terminated after first cleanup, got: {:?}",
        running
    );

    guard.cleanup()?;

    Ok(())
}

#[test]
fn guard_cleanup_runs_on_drop() -> Result<()> {
    let temp = TempDir::new()?;

    let mut guard = create_test_guard(&temp);
    let pid = register_sleeping_worker(&mut guard, &temp, "RQ-0001")?;

    assert_eq!(
        lock::pid_is_running(pid),
        Some(true),
        "Worker should be running before drop"
    );

    drop(guard);

    let running = lock::pid_is_running(pid);
    assert!(
        running == Some(false) || running.is_none(),
        "Worker should be terminated after guard drop, got: {:?}",
        running
    );

    Ok(())
}

#[test]
fn guard_cleanup_retains_terminal_workers_for_status_retry() -> Result<()> {
    let temp = TempDir::new()?;
    let mut guard = create_test_guard(&temp);

    let running_workspace = temp.path().join("workspaces").join("RQ-0001");
    let completed_workspace = temp.path().join("workspaces").join("RQ-0002");
    std::fs::create_dir_all(&running_workspace)?;
    std::fs::create_dir_all(&completed_workspace)?;

    guard
        .state_file_mut()
        .upsert_worker(state::WorkerRecord::new(
            "RQ-0001",
            running_workspace,
            "2026-02-20T00:00:00Z".to_string(),
        ));

    let mut completed = state::WorkerRecord::new(
        "RQ-0002",
        completed_workspace,
        "2026-02-20T00:00:00Z".to_string(),
    );
    completed.mark_completed("2026-02-20T00:01:00Z".to_string());
    guard.state_file_mut().upsert_worker(completed);

    guard.cleanup()?;

    assert_eq!(guard.state_file.workers.len(), 1);
    assert_eq!(guard.state_file.workers[0].task_id, "RQ-0002");
    assert!(guard.state_file.workers[0].is_terminal());
    Ok(())
}

#[test]
fn guard_cleanup_retains_blocked_workspace_for_retry() -> Result<()> {
    let temp = TempDir::new()?;
    let mut guard = create_test_guard(&temp);

    let blocked_workspace = temp.path().join("workspaces").join("RQ-0099");
    std::fs::create_dir_all(&blocked_workspace)?;

    guard.register_workspace(
        "RQ-0099".to_string(),
        WorkspaceSpec {
            path: blocked_workspace.clone(),
            branch: "main".to_string(),
        },
    );

    let mut blocked = state::WorkerRecord::new(
        "RQ-0099",
        blocked_workspace.clone(),
        "2026-02-20T00:00:00Z".to_string(),
    );
    blocked.mark_blocked("2026-02-20T00:05:00Z".to_string(), "blocked");
    guard.state_file_mut().upsert_worker(blocked);

    guard.cleanup()?;

    assert!(blocked_workspace.exists());
    assert_eq!(guard.state_file.workers.len(), 1);
    assert!(matches!(
        guard.state_file.workers[0].lifecycle,
        state::WorkerLifecycle::BlockedPush
    ));
    Ok(())
}
