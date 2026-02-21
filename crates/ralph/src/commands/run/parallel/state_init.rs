//! Parallel state initialization and validation for direct-push mode.
//!
//! Responsibilities:
//! - Load or initialize parallel state file with proper defaults.
//! - Validate target branch consistency and auto-heal when safe.
//! - Clean up stale workspaces for completed/failed workers.
//!
//! Not handled here:
//! - State persistence I/O (see `super::state`).
//! - Worker orchestration (see `super::orchestration`).
//! - General state mutations during run (see `super::orchestration`).
//!
//! Invariants/assumptions:
//! - State file path is under `.ralph/cache/parallel/state.json`.
//! - Target branch changes are only allowed when no active work is in flight.

use crate::git;
use anyhow::{Result, bail};
use std::path::Path;

use super::ParallelSettings;
use super::state::{self, ParallelStateFile};
use super::workspace_cleanup::remove_workspace_best_effort;

/// Load existing state or create new, with pruning and validation.
pub(crate) fn load_or_init_parallel_state(
    _repo_root: &Path,
    state_path: &Path,
    current_branch: &str,
    started_at: &str,
    settings: &ParallelSettings,
) -> Result<ParallelStateFile> {
    let current_branch = current_branch.trim();

    if let Some(mut existing) = state::load_state(state_path)? {
        // Prune stale workers (terminal state with missing workspace or expired TTL)
        let dropped_workers = super::prune_stale_workers(&mut existing);
        if !dropped_workers.is_empty() {
            log::warn!("Dropping stale workers: {}", dropped_workers.join(", "));
            state::save_state(state_path, &existing)?;
        }

        // Clean up workspaces for terminal workers
        let cleaned_workspaces = cleanup_terminal_workspaces(&existing, &settings.workspace_root);
        if !cleaned_workspaces.is_empty() {
            log::info!(
                "Cleaned up workspaces for terminal workers: {}",
                cleaned_workspaces.join(", ")
            );
        }

        // Validate and potentially auto-heal target branch
        let mut normalized = false;

        if existing.target_branch.is_empty() {
            let active_workers = existing.active_worker_count();
            if active_workers == 0 {
                log::warn!(
                    "Parallel state target branch missing; populating from current branch '{}'.",
                    current_branch
                );
                existing.target_branch = current_branch.to_string();
                existing.started_at = started_at.to_string();
                normalized = true;
            } else {
                bail!(format_target_branch_missing_error(
                    state_path,
                    current_branch,
                    active_workers
                ));
            }
        } else if existing.target_branch != current_branch {
            let active_workers = existing.active_worker_count();
            if active_workers == 0 {
                log::warn!(
                    "Parallel state target branch '{}' does not match current branch '{}'; retargeting state at {}.",
                    existing.target_branch,
                    current_branch,
                    state_path.display()
                );
                existing.target_branch = current_branch.to_string();
                existing.started_at = started_at.to_string();
                normalized = true;
            } else {
                bail!(format_target_branch_mismatch_error(
                    state_path,
                    &existing.target_branch,
                    current_branch,
                    active_workers
                ));
            }
        }

        if normalized {
            state::save_state(state_path, &existing)?;
        }

        Ok(existing)
    } else {
        // Create fresh state
        let state = ParallelStateFile::new(started_at.to_string(), current_branch.to_string());
        state::save_state(state_path, &state)?;
        Ok(state)
    }
}

/// Remove workspaces for workers in terminal states.
pub(crate) fn cleanup_terminal_workspaces(
    state_file: &ParallelStateFile,
    workspace_root: &Path,
) -> Vec<String> {
    let mut removed = Vec::new();

    for worker in &state_file.workers {
        // Only clean up terminal states (completed, failed, blocked_push)
        if !worker.is_terminal() {
            continue;
        }

        let task_id = worker.task_id.trim();
        if task_id.is_empty() {
            continue;
        }

        if !worker.workspace_path.exists() {
            continue;
        }

        let spec = git::WorkspaceSpec {
            path: worker.workspace_path.clone(),
            branch: format!("ralph/{}", task_id),
        };

        remove_workspace_best_effort(workspace_root, &spec, "terminal worker cleanup");

        if !worker.workspace_path.exists() {
            removed.push(task_id.to_string());
        }
    }

    removed
}

fn format_target_branch_mismatch_error(
    state_path: &Path,
    recorded_branch: &str,
    current_branch: &str,
    active_workers: usize,
) -> String {
    format!(
        "Parallel state target branch '{}' does not match current branch '{}'.\n\
State file: {}\n\
Unsafe to retarget because {} worker(s) are active.\n\
\n\
Recovery options:\n\
1) checkout '{}' and resume the parallel run\n\
2) if you are certain no parallel run is active, delete '{}'",
        recorded_branch,
        current_branch,
        state_path.display(),
        active_workers,
        recorded_branch,
        state_path.display()
    )
}

fn format_target_branch_missing_error(
    state_path: &Path,
    current_branch: &str,
    active_workers: usize,
) -> String {
    format!(
        "Parallel state target branch is missing.\n\
State file: {}\n\
Unsafe to populate from current branch '{}' because {} worker(s) are active.\n\
\n\
Recovery options:\n\
1) checkout the original base branch and resume the parallel run\n\
2) if you are certain no parallel run is active, delete '{}'",
        state_path.display(),
        current_branch,
        active_workers,
        state_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeutil;
    use std::path::Path;
    use tempfile::TempDir;

    fn test_settings(repo_root: &Path) -> super::ParallelSettings {
        super::ParallelSettings {
            workers: 2,
            workspace_root: repo_root.join("workspaces"),
            max_push_attempts: 5,
            push_backoff_ms: vec![500, 2000, 5000, 10000],
            workspace_retention_hours: 24,
        }
    }

    #[test]
    fn target_branch_mismatch_auto_heals_when_no_active_workers() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let state_path = state::state_file_path(repo_root);
        let settings = test_settings(repo_root);

        // Create state with old target branch but no active workers
        let state = ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "old".to_string());
        // Add a completed worker (terminal state)
        let mut worker = super::super::WorkerRecord::new(
            "RQ-0001",
            repo_root.join("workspaces/RQ-0001"),
            "2026-02-20T00:00:00Z".to_string(),
        );
        worker.mark_completed("2026-02-20T01:00:00Z".to_string());

        state::save_state(&state_path, &state)?;

        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            "2026-02-21T00:00:00Z",
            &settings,
        )?;

        assert_eq!(loaded.target_branch, "main");
        assert_eq!(loaded.started_at, "2026-02-21T00:00:00Z");

        Ok(())
    }

    #[test]
    fn target_branch_missing_auto_heals_when_no_active_workers() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let state_path = state::state_file_path(repo_root);
        let settings = test_settings(repo_root);

        let state = ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "".to_string());
        state::save_state(&state_path, &state)?;

        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            "2026-02-21T00:00:00Z",
            &settings,
        )?;

        assert_eq!(loaded.target_branch, "main");
        Ok(())
    }

    #[test]
    fn target_branch_mismatch_errors_when_active_workers() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_path = repo_root.join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        let state_path = state::state_file_path(repo_root);
        let settings = test_settings(repo_root);
        let mut state =
            ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "old".to_string());

        // Add an active (non-terminal) worker
        let worker = super::super::WorkerRecord::new(
            "RQ-0001",
            workspace_path,
            timeutil::now_utc_rfc3339_or_fallback(),
        );
        state.upsert_worker(worker);
        state::save_state(&state_path, &state)?;

        let err = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            "2026-02-21T00:00:00Z",
            &settings,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("target branch"));
        assert!(msg.contains("does not match"));
        Ok(())
    }

    #[test]
    fn cleanup_terminal_workspaces_removes_completed() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_root = repo_root.join("workspaces");
        let workspace_path = workspace_root.join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;
        std::fs::write(workspace_path.join("README.md"), "stale workspace")?;

        let mut state =
            ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        let mut worker = super::super::WorkerRecord::new(
            "RQ-0001",
            workspace_path.clone(),
            "2026-02-20T00:00:00Z".to_string(),
        );
        worker.mark_completed("2026-02-20T01:00:00Z".to_string());
        state.upsert_worker(worker);

        let removed = cleanup_terminal_workspaces(&state, &workspace_root);

        assert_eq!(removed, vec!["RQ-0001"]);
        assert!(!workspace_path.exists());
        Ok(())
    }

    #[test]
    fn cleanup_terminal_workspaces_preserves_active() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_root = repo_root.join("workspaces");
        let workspace_path = workspace_root.join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state =
            ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        // Active (non-terminal) worker
        let worker = super::super::WorkerRecord::new(
            "RQ-0001",
            workspace_path.clone(),
            "2026-02-20T00:00:00Z".to_string(),
        );
        state.upsert_worker(worker);

        let removed = cleanup_terminal_workspaces(&state, &workspace_root);

        assert!(removed.is_empty());
        assert!(workspace_path.exists());
        Ok(())
    }
}
