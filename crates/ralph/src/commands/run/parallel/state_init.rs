//! Parallel state initialization and validation.
//!
//! Responsibilities:
//! - Load or initialize parallel state file with proper defaults.
//! - Reconcile PR records against current GitHub state.
//! - Clean up stale workspaces for merged/closed PRs.
//! - Validate base branch consistency and auto-heal when safe.
//!
//! Not handled here:
//! - State persistence I/O (see `super::state`).
//! - Worker orchestration (see `super::orchestration`).
//! - General state mutations during run (see `super::orchestration`).
//!
//! Invariants/assumptions:
//! - State file path is under `.ralph/cache/parallel/state.json`.
//! - GitHub CLI (`gh`) is available for PR reconciliation.
//! - Base branch changes are only allowed when no blocking work is in flight.

use crate::git;
use anyhow::{Result, bail};
use std::path::Path;

use super::ParallelSettings;
use super::prune_stale_tasks_in_flight;
use super::state::{self, ParallelStateFile};

/// Load existing state or create new, with pruning and validation.
pub(crate) fn load_or_init_parallel_state(
    repo_root: &Path,
    state_path: &Path,
    current_branch: &str,
    started_at: &str,
    settings: &mut ParallelSettings,
) -> Result<ParallelStateFile> {
    let current_branch = current_branch.trim();
    if let Some(mut existing) = state::load_state(state_path)? {
        let dropped_tasks = prune_stale_tasks_in_flight(&mut existing);
        if !dropped_tasks.is_empty() {
            log::warn!(
                "Dropping stale in-flight tasks: {}",
                dropped_tasks.join(", ")
            );
            state::save_state(state_path, &existing)?;
        }

        // Reconcile PR records against current GitHub state
        let summary = state::reconcile_pr_records(repo_root, &mut existing)?;
        if summary.has_changes() {
            log::info!(
                "Reconciled PR records: {} closed, {} merged, {} errors",
                summary.closed_count,
                summary.merged_count,
                summary.error_count
            );
            state::save_state(state_path, &existing)?;
        }

        let cleaned_workspaces = cleanup_pr_workspaces(&existing, &settings.workspace_root);
        if !cleaned_workspaces.is_empty() {
            log::info!(
                "Removed stale workspaces for merged/closed PRs: {}",
                cleaned_workspaces.join(", ")
            );
        }

        let mut normalized = false;
        let trimmed_base = existing.base_branch.trim().to_string();
        if trimmed_base != existing.base_branch {
            existing.base_branch = trimmed_base;
            normalized = true;
        }
        if existing.started_at.trim().is_empty() {
            existing.started_at = started_at.to_string();
            normalized = true;
        }

        let in_flight = in_flight_task_ids(&existing);
        let blocking_prs = blocking_pr_task_ids(&existing);

        if existing.base_branch.is_empty() {
            if in_flight.is_empty() && blocking_prs.is_empty() {
                log::warn!(
                    "Parallel state base branch missing; populating from current branch '{}'.",
                    current_branch
                );
                existing.base_branch = current_branch.to_string();
                existing.started_at = started_at.to_string();
                normalized = true;
            } else {
                bail!(format_base_branch_missing_error(
                    state_path,
                    current_branch,
                    &in_flight,
                    &blocking_prs
                ));
            }
        } else if existing.base_branch != current_branch {
            if in_flight.is_empty() && blocking_prs.is_empty() {
                log::warn!(
                    "Parallel state base branch '{}' does not match current branch '{}'; retargeting state at {}.",
                    existing.base_branch,
                    current_branch,
                    state_path.display()
                );
                existing.base_branch = current_branch.to_string();
                existing.started_at = started_at.to_string();
                normalized = true;
            } else {
                bail!(format_base_branch_mismatch_error(
                    state_path,
                    &existing.base_branch,
                    current_branch,
                    &in_flight,
                    &blocking_prs
                ));
            }
        }

        if normalized {
            state::save_state(state_path, &existing)?;
        }

        if existing.merge_method != settings.merge_method {
            log::warn!(
                "Parallel state merge_method {:?} overrides current settings {:?}.",
                existing.merge_method,
                settings.merge_method
            );
            settings.merge_method = existing.merge_method;
        }
        if existing.merge_when != settings.merge_when {
            log::warn!(
                "Parallel state merge_when {:?} overrides current settings {:?}.",
                existing.merge_when,
                settings.merge_when
            );
            settings.merge_when = existing.merge_when;
        }

        Ok(existing)
    } else {
        let state = state::ParallelStateFile::new(
            started_at.to_string(),
            current_branch.to_string(),
            settings.merge_method,
            settings.merge_when,
        );
        state::save_state(state_path, &state)?;
        Ok(state)
    }
}

/// Remove workspaces for PRs that are no longer open/unmerged.
pub(crate) fn cleanup_pr_workspaces(
    state_file: &ParallelStateFile,
    workspace_root: &Path,
) -> Vec<String> {
    let mut removed = Vec::new();

    for record in &state_file.prs {
        if record.is_open_unmerged() {
            continue;
        }

        let task_id = record.task_id.trim();
        if task_id.is_empty() {
            continue;
        }

        let path = workspace_root.join(task_id);
        if !path.exists() {
            continue;
        }

        let branch = format!("ralph/{}", task_id);
        let spec = git::WorkspaceSpec {
            path: path.clone(),
            branch,
        };

        if let Err(err) = git::remove_workspace(workspace_root, &spec, true) {
            log::warn!(
                "Failed to remove workspace for {} at {}: {:#}",
                task_id,
                path.display(),
                err
            );
        } else {
            removed.push(task_id.to_string());
        }
    }

    removed
}

// Helper functions (private):
fn in_flight_task_ids(state_file: &ParallelStateFile) -> Vec<String> {
    state_file
        .tasks_in_flight
        .iter()
        .map(|record| record.task_id.clone())
        .collect()
}

fn blocking_pr_task_ids(state_file: &ParallelStateFile) -> Vec<String> {
    state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
        .map(|record| record.task_id.clone())
        .collect()
}

fn format_base_branch_mismatch_error(
    state_path: &Path,
    recorded_branch: &str,
    current_branch: &str,
    in_flight: &[String],
    blocking_prs: &[String],
) -> String {
    let mut blockers = Vec::new();
    if !in_flight.is_empty() {
        blockers.push(format!(
            "- {} in-flight task(s): {}",
            in_flight.len(),
            in_flight.join(", ")
        ));
    }
    if !blocking_prs.is_empty() {
        blockers.push(format!(
            "- {} open PR(s): {}",
            blocking_prs.len(),
            blocking_prs.join(", ")
        ));
    }
    let blocker_text = if blockers.is_empty() {
        "- none".to_string()
    } else {
        blockers.join("\n")
    };

    format!(
        "Parallel state base branch '{}' does not match current branch '{}'.\nState file: {}\nUnsafe to retarget because:\n{}\nRecovery options:\n1) checkout '{}' and resume the parallel run\n2) if you are certain no parallel run is active, delete '{}'",
        recorded_branch,
        current_branch,
        state_path.display(),
        blocker_text,
        recorded_branch,
        state_path.display()
    )
}

fn format_base_branch_missing_error(
    state_path: &Path,
    current_branch: &str,
    in_flight: &[String],
    blocking_prs: &[String],
) -> String {
    let mut blockers = Vec::new();
    if !in_flight.is_empty() {
        blockers.push(format!(
            "- {} in-flight task(s): {}",
            in_flight.len(),
            in_flight.join(", ")
        ));
    }
    if !blocking_prs.is_empty() {
        blockers.push(format!(
            "- {} open PR(s): {}",
            blocking_prs.len(),
            blocking_prs.join(", ")
        ));
    }
    let blocker_text = if blockers.is_empty() {
        "- none".to_string()
    } else {
        blockers.join("\n")
    };

    format!(
        "Parallel state base branch is missing.\nState file: {}\nUnsafe to populate from current branch '{}' because:\n{}\nRecovery options:\n1) checkout the original base branch and resume the parallel run\n2) if you are certain no parallel run is active, delete '{}'",
        state_path.display(),
        current_branch,
        blocker_text,
        state_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, ParallelMergeWhen,
    };
    use crate::timeutil;
    use std::path::Path;
    use tempfile::TempDir;

    fn test_parallel_settings(repo_root: &Path) -> ParallelSettings {
        ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 5,
            workspace_root: repo_root.join("workspaces"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        }
    }

    #[test]
    fn base_branch_mismatch_auto_heals_when_state_empty() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let state_path = state::state_file_path(repo_root);
        let state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            &started_at,
            &mut settings,
        )?;

        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.started_at, started_at);

        let reloaded = state::load_state(&state_path)?.expect("state");
        assert_eq!(reloaded.base_branch, "main");
        assert_eq!(reloaded.started_at, started_at);
        Ok(())
    }

    #[test]
    fn base_branch_missing_auto_heals_when_state_empty() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let state_path = state::state_file_path(repo_root);
        let state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            &started_at,
            &mut settings,
        )?;

        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.started_at, started_at);

        let reloaded = state::load_state(&state_path)?.expect("state");
        assert_eq!(reloaded.base_branch, "main");
        assert_eq!(reloaded.started_at, started_at);
        Ok(())
    }

    #[test]
    fn base_branch_missing_errors_when_tasks_in_flight_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_path = repo_root.join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the record is not pruned by TTL
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: None,
            started_at: recent_timestamp,
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("base branch is missing"));
        assert!(msg.contains("in-flight"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    #[test]
    fn load_or_init_cleans_workspaces_for_merged_prs() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_root = repo_root.join("workspaces");
        let workspace_path = workspace_root.join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;
        std::fs::write(workspace_path.join("README.md"), "stale workspace")?;

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        let pr = git::PrInfo {
            number: 1,
            url: "https://example.com/pr/1".to_string(),
            head: "ralph/RQ-0001".to_string(),
            base: "main".to_string(),
        };
        let mut record = state::ParallelPrRecord::new("RQ-0001", &pr, Some(&workspace_path));
        record.lifecycle = state::ParallelPrLifecycle::Merged;
        state.prs.push(record);

        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        settings.workspace_root = workspace_root.clone();

        load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)?;

        assert!(
            !workspace_path.exists(),
            "merged PR workspace should be cleaned up"
        );
        Ok(())
    }

    #[test]
    fn base_branch_mismatch_errors_when_tasks_in_flight_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_path = repo_root.join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the record is not pruned by TTL
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: None,
            started_at: recent_timestamp,
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("Parallel state base branch"));
        assert!(msg.contains("in-flight"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    #[test]
    fn base_branch_mismatch_prunes_then_auto_heals_when_only_stale_tasks() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: repo_root
                .join("missing/RQ-0002")
                .to_string_lossy()
                .to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            &started_at,
            &mut settings,
        )?;

        assert!(loaded.tasks_in_flight.is_empty());
        assert_eq!(loaded.base_branch, "main");
        Ok(())
    }

    #[test]
    fn base_branch_mismatch_errors_when_blockers_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        let workspace_path = repo_root.join("workspaces").join("RQ-0003");
        std::fs::create_dir_all(&workspace_path)?;
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0003".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0003".to_string(),
            pid: Some(std::process::id()),
            started_at: crate::timeutil::now_utc_rfc3339_or_fallback(),
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("in-flight task"));
        assert!(msg.contains("state.json"));
        Ok(())
    }
}
