//! Worker-event handling helpers for parallel orchestration.
//!
//! Purpose:
//! - Worker-event handling helpers for parallel orchestration.
//!
//! Responsibilities:
//! - Summarize blocked workers at loop start.
//! - Apply worker exit events to persisted parallel state.
//!
//! Non-scope:
//! - Worker spawning or selection.
//! - Loop termination decisions.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::commands::run::parallel::cleanup_guard::ParallelCleanupGuard;
use crate::commands::run::parallel::state::{self, WorkerLifecycle, WorkerRecord};
use crate::commands::run::parallel::worker::FinishedWorker;
use crate::commands::run::parallel::workspace_cleanup::remove_workspace_best_effort;
use crate::contracts::QueueFile;
use crate::timeutil;

use super::stats::ParallelRunStats;

fn summarize_block_reason(reason: &str) -> String {
    let first_line = reason.lines().next().unwrap_or(reason).trim();
    const MAX_REASON_LEN: usize = 180;
    if first_line.len() <= MAX_REASON_LEN {
        return first_line.to_string();
    }
    let mut truncated = first_line
        .chars()
        .take(MAX_REASON_LEN - 3)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

pub(super) fn announce_blocked_tasks_at_loop_start(
    queue_file: &QueueFile,
    state_file: &state::ParallelStateFile,
) {
    let queued_ids: HashSet<&str> = queue_file
        .tasks
        .iter()
        .map(|task| task.id.trim())
        .filter(|task_id| !task_id.is_empty())
        .collect();

    let blocked_workers: Vec<&WorkerRecord> = state_file
        .workers
        .iter()
        .filter(|worker| worker.lifecycle == WorkerLifecycle::BlockedPush)
        .filter(|worker| queued_ids.contains(worker.task_id.trim()))
        .collect();

    if blocked_workers.is_empty() {
        return;
    }

    log::warn!(
        "Parallel loop start: {} queued task(s) are in blocked_push and will be skipped until retried.",
        blocked_workers.len()
    );
    for worker in blocked_workers {
        let reason = worker
            .last_error
            .as_deref()
            .map(summarize_block_reason)
            .unwrap_or_else(|| "No failure reason recorded".to_string());
        log::warn!(
            "Blocked task {} (attempts: {}) reason: {}",
            worker.task_id,
            worker.push_attempts,
            reason
        );
    }
    log::warn!("Use `ralph run parallel retry --task <TASK_ID>` to retry a blocked task.");
}

pub(super) fn handle_finished_workers(
    finished: Vec<FinishedWorker>,
    guard: &mut ParallelCleanupGuard,
    state_path: &Path,
    workspace_root: &Path,
    coordinator_repo_root: &Path,
    target_branch: &str,
    stats: &mut ParallelRunStats,
) -> Result<()> {
    for finished_worker in finished {
        let FinishedWorker {
            task_id,
            task_title: _task_title,
            workspace,
            status,
        } = finished_worker;

        if status.success() {
            stats.record_success();

            if let Some(worker) = guard.state_file_mut().get_worker_mut(&task_id) {
                worker.mark_completed(timeutil::now_utc_rfc3339_or_fallback());
            }

            log::info!("Worker {} completed successfully", task_id);
            refresh_coordinator_branch_best_effort(coordinator_repo_root, target_branch);
        } else {
            stats.record_failure();

            let blocked_marker =
                match super::super::integration::read_blocked_push_marker(&workspace.path) {
                    Ok(marker) => marker,
                    Err(err) => {
                        log::warn!(
                            "Failed reading blocked marker for {} ({}): {}",
                            task_id,
                            workspace.path.display(),
                            err
                        );
                        None
                    }
                };

            if let Some(marker) = blocked_marker {
                if let Some(worker) = guard.state_file_mut().get_worker_mut(&task_id) {
                    worker.push_attempts = marker.attempt;
                    worker.mark_blocked(
                        timeutil::now_utc_rfc3339_or_fallback(),
                        marker.reason.clone(),
                    );
                }

                log::warn!(
                    "Worker {} blocked after {}/{} integration attempts: {}",
                    task_id,
                    marker.attempt,
                    marker.max_attempts,
                    marker.reason
                );
                log::warn!(
                    "Retaining blocked workspace for retry: {}",
                    workspace.path.display()
                );
            } else {
                if let Some(worker) = guard.state_file_mut().get_worker_mut(&task_id) {
                    worker.mark_failed(
                        timeutil::now_utc_rfc3339_or_fallback(),
                        format!("Worker exited with status: {:?}", status.code()),
                    );
                }

                log::warn!(
                    "Worker {} failed with exit status: {:?}",
                    task_id,
                    status.code()
                );

                remove_workspace_best_effort(workspace_root, &workspace, "worker failure");
            }
        }

        state::save_state(state_path, guard.state_file())?;
        guard.remove_worker(&task_id);
    }

    Ok(())
}

fn refresh_coordinator_branch_best_effort(repo_root: &Path, target_branch: &str) {
    if let Err(err) = crate::git::branch::fast_forward_branch_to_origin(repo_root, target_branch) {
        log::warn!(
            "Worker completed, but local branch refresh to origin/{} failed: {:#}",
            target_branch,
            err
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::run::parallel::state::{
        self, ParallelStateFile, WorkerLifecycle, WorkerRecord,
    };
    use crate::commands::run::parallel::worker::start_worker_monitor;
    use crate::git::WorkspaceSpec;
    use anyhow::Result;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    fn create_guard(temp: &TempDir, state_path: std::path::PathBuf) -> ParallelCleanupGuard {
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).expect("create workspace root");
        let state_file =
            ParallelStateFile::new("2026-04-25T00:00:00Z".to_string(), "main".to_string());
        ParallelCleanupGuard::new_simple(state_path, state_file, workspace_root)
    }

    fn register_finished_worker_monitor(
        guard: &mut ParallelCleanupGuard,
        workspace: &WorkspaceSpec,
        task_id: &str,
    ) -> Result<()> {
        let child: Child = Command::new(std::env::current_exe()?)
            .arg("--help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let worker = start_worker_monitor(
            task_id,
            "Test task".to_string(),
            workspace.clone(),
            child,
            guard.worker_event_sender(),
        );
        guard.register_worker(task_id.to_string(), worker);
        Ok(())
    }

    fn worker_workspace(temp: &TempDir, task_id: &str) -> Result<WorkspaceSpec> {
        let path = temp.path().join("workspaces").join(task_id);
        std::fs::create_dir_all(&path)?;
        Ok(WorkspaceSpec {
            path,
            branch: "main".to_string(),
        })
    }

    fn synthetic_status(code: i32) -> ExitStatus {
        #[cfg(unix)]
        {
            ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            ExitStatus::from_raw(code as u32)
        }
    }

    #[test]
    fn finished_success_marks_completed_persists_state_and_removes_worker() -> Result<()> {
        let temp = TempDir::new()?;
        let state_path = temp.path().join("state.json");
        let mut guard = create_guard(&temp, state_path.clone());
        let workspace = worker_workspace(&temp, "RQ-0006")?;
        register_finished_worker_monitor(&mut guard, &workspace, "RQ-0006")?;
        guard.state_file_mut().upsert_worker(WorkerRecord::new(
            "RQ-0006",
            workspace.path.clone(),
            "2026-04-25T00:00:00Z".to_string(),
        ));

        let mut stats = ParallelRunStats::default();
        handle_finished_workers(
            vec![crate::commands::run::parallel::worker::FinishedWorker {
                task_id: "RQ-0006".to_string(),
                task_title: "Task".to_string(),
                workspace: workspace.clone(),
                status: synthetic_status(0),
            }],
            &mut guard,
            &state_path,
            &temp.path().join("workspaces"),
            temp.path(),
            "main",
            &mut stats,
        )?;

        let saved = state::load_state(&state_path)?.expect("saved state");
        let worker = saved.get_worker("RQ-0006").expect("worker record");
        assert_eq!(worker.lifecycle, WorkerLifecycle::Completed);
        assert_eq!(stats.succeeded(), 1);
        assert!(guard.in_flight().is_empty());
        Ok(())
    }

    #[test]
    fn finished_blocked_push_retains_workspace_and_records_attempts() -> Result<()> {
        let temp = TempDir::new()?;
        let state_path = temp.path().join("state.json");
        let mut guard = create_guard(&temp, state_path.clone());
        let workspace = worker_workspace(&temp, "RQ-0006")?;
        register_finished_worker_monitor(&mut guard, &workspace, "RQ-0006")?;
        guard.state_file_mut().upsert_worker(WorkerRecord::new(
            "RQ-0006",
            workspace.path.clone(),
            "2026-04-25T00:00:00Z".to_string(),
        ));

        let marker_path = workspace
            .path
            .join(".ralph/cache/parallel/blocked_push.json");
        std::fs::create_dir_all(marker_path.parent().expect("marker parent"))?;
        std::fs::write(
            &marker_path,
            serde_json::json!({
                "task_id": "RQ-0006",
                "reason": "blocked by integration",
                "attempt": 3,
                "max_attempts": 5,
                "generated_at": "2026-04-25T00:01:00Z"
            })
            .to_string(),
        )?;

        let mut stats = ParallelRunStats::default();
        handle_finished_workers(
            vec![crate::commands::run::parallel::worker::FinishedWorker {
                task_id: "RQ-0006".to_string(),
                task_title: "Task".to_string(),
                workspace: workspace.clone(),
                status: synthetic_status(1),
            }],
            &mut guard,
            &state_path,
            &temp.path().join("workspaces"),
            temp.path(),
            "main",
            &mut stats,
        )?;

        let saved = state::load_state(&state_path)?.expect("saved state");
        let worker = saved.get_worker("RQ-0006").expect("worker record");
        assert_eq!(worker.lifecycle, WorkerLifecycle::BlockedPush);
        assert_eq!(worker.push_attempts, 3);
        assert_eq!(worker.last_error.as_deref(), Some("blocked by integration"));
        assert!(workspace.path.exists());
        assert_eq!(stats.failed(), 1);
        Ok(())
    }

    #[test]
    fn finished_failure_without_block_marker_cleans_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let state_path = temp.path().join("state.json");
        let mut guard = create_guard(&temp, state_path.clone());
        let workspace = worker_workspace(&temp, "RQ-0006")?;
        register_finished_worker_monitor(&mut guard, &workspace, "RQ-0006")?;
        guard.state_file_mut().upsert_worker(WorkerRecord::new(
            "RQ-0006",
            workspace.path.clone(),
            "2026-04-25T00:00:00Z".to_string(),
        ));

        let mut stats = ParallelRunStats::default();
        handle_finished_workers(
            vec![crate::commands::run::parallel::worker::FinishedWorker {
                task_id: "RQ-0006".to_string(),
                task_title: "Task".to_string(),
                workspace: workspace.clone(),
                status: synthetic_status(9),
            }],
            &mut guard,
            &state_path,
            &temp.path().join("workspaces"),
            temp.path(),
            "main",
            &mut stats,
        )?;

        let saved = state::load_state(&state_path)?.expect("saved state");
        let worker = saved.get_worker("RQ-0006").expect("worker record");
        assert_eq!(worker.lifecycle, WorkerLifecycle::Failed);
        assert!(!workspace.path.exists());
        assert_eq!(stats.failed(), 1);
        Ok(())
    }

    #[test]
    fn finished_worker_state_save_failure_keeps_guard_tracking() -> Result<()> {
        let temp = TempDir::new()?;
        let bad_state_path = temp.path().join("state-dir");
        std::fs::create_dir_all(&bad_state_path)?;
        let mut guard = create_guard(&temp, bad_state_path.clone());
        let workspace = worker_workspace(&temp, "RQ-0006")?;
        register_finished_worker_monitor(&mut guard, &workspace, "RQ-0006")?;
        guard.state_file_mut().upsert_worker(WorkerRecord::new(
            "RQ-0006",
            workspace.path.clone(),
            "2026-04-25T00:00:00Z".to_string(),
        ));

        let mut stats = ParallelRunStats::default();
        let err = handle_finished_workers(
            vec![crate::commands::run::parallel::worker::FinishedWorker {
                task_id: "RQ-0006".to_string(),
                task_title: "Task".to_string(),
                workspace: workspace.clone(),
                status: synthetic_status(0),
            }],
            &mut guard,
            &bad_state_path,
            &temp.path().join("workspaces"),
            temp.path(),
            "main",
            &mut stats,
        )
        .expect_err("state save should fail");

        assert!(err.to_string().contains("write parallel state"));
        assert!(guard.in_flight().contains_key("RQ-0006"));
        Ok(())
    }

    #[test]
    fn finished_success_tolerates_branch_refresh_failure() -> Result<()> {
        let temp = TempDir::new()?;
        let state_path = temp.path().join("state.json");
        let mut guard = create_guard(&temp, state_path.clone());
        let workspace = worker_workspace(&temp, "RQ-0006")?;
        register_finished_worker_monitor(&mut guard, &workspace, "RQ-0006")?;
        guard.state_file_mut().upsert_worker(WorkerRecord::new(
            "RQ-0006",
            workspace.path.clone(),
            "2026-04-25T00:00:00Z".to_string(),
        ));

        let mut stats = ParallelRunStats::default();
        handle_finished_workers(
            vec![crate::commands::run::parallel::worker::FinishedWorker {
                task_id: "RQ-0006".to_string(),
                task_title: "Task".to_string(),
                workspace,
                status: synthetic_status(0),
            }],
            &mut guard,
            &state_path,
            &temp.path().join("workspaces"),
            &temp.path().join("not-a-git-repo"),
            "main",
            &mut stats,
        )?;

        assert_eq!(stats.succeeded(), 1);
        Ok(())
    }
}
