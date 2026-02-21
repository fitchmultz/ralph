//! Parallel run loop supervisor and worker orchestration for direct-push mode.
//!
//! Responsibilities:
//! - Coordinate parallel task execution across multiple workers.
//! - Manage settings resolution and preflight validation.
//! - Track worker capacity and task pruning.
//! - Handle direct-push integration from workers.
//!
//! Not handled here:
//! - Main orchestration loop (see `orchestration.rs`).
//! - State initialization (see `state_init.rs`).
//! - Worker lifecycle (see `worker.rs`).
//! - Integration loop logic (see `integration.rs`).
//!
//! Invariants/assumptions:
//! - Queue order is authoritative for task selection.
//! - Workers run in isolated workspaces with dedicated branches.
//! - Workers push directly to the target branch (no PRs).
//! - One active worker per task ID (enforced by upsert_worker).

use crate::agent::AgentOverrides;
use crate::config;
use crate::git;
use crate::timeutil;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

mod args;
mod cleanup_guard;
mod integration;
mod orchestration;
mod path_map;
pub mod state;
mod state_init;
mod sync;
mod worker;
mod workspace_cleanup;

// =============================================================================
// Marker File Constants (for CI failure detection)
// =============================================================================

/// Marker file name for CI gate failure.
/// Written to workspace when CI fails, checked by coordinator before draft PR creation.
pub const CI_FAILURE_MARKER_FILE: &str = ".ralph/cache/ci-failure-marker";

/// Fallback marker file used only when primary marker path is unavailable.
pub const CI_FAILURE_MARKER_FALLBACK_FILE: &str = ".ralph-ci-failure-marker";

/// Default push backoff intervals in milliseconds.
pub fn default_push_backoff_ms() -> Vec<u64> {
    vec![500, 2000, 5000, 10000]
}

// Re-export public APIs from submodules
pub use integration::{IntegrationConfig, IntegrationOutcome, RemediationHandoff};
pub(crate) use orchestration::run_loop_parallel;
pub use state::{WorkerLifecycle, WorkerRecord};

use cleanup_guard::ParallelCleanupGuard;
use state_init::load_or_init_parallel_state;

pub(crate) struct ParallelRunOptions {
    pub max_tasks: u32,
    pub workers: u8,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
}

#[allow(dead_code)]
pub(crate) struct ParallelSettings {
    pub(crate) workers: u8,
    pub(crate) workspace_root: PathBuf,
    pub(crate) max_push_attempts: u8,
    pub(crate) push_backoff_ms: Vec<u64>,
    pub(crate) workspace_retention_hours: u32,
}

// Settings resolution
fn resolve_parallel_settings(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
) -> Result<ParallelSettings> {
    let cfg = &resolved.config.parallel;
    Ok(ParallelSettings {
        workers: opts.workers,
        workspace_root: git::workspace_root(&resolved.repo_root, &resolved.config),
        max_push_attempts: cfg.max_push_attempts.unwrap_or(5),
        push_backoff_ms: cfg
            .push_backoff_ms
            .clone()
            .unwrap_or_else(default_push_backoff_ms),
        workspace_retention_hours: cfg.workspace_retention_hours.unwrap_or(24),
    })
}

fn overrides_for_parallel_workers(
    resolved: &config::Resolved,
    overrides: &AgentOverrides,
) -> AgentOverrides {
    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(overrides, resolved);
    if repoprompt_flags.plan_required || repoprompt_flags.tool_injection {
        log::warn!(
            "Parallel workers disable RepoPrompt plan/tooling instructions to keep edits in workspace clones."
        );
    }

    let mut worker_overrides = overrides.clone();
    worker_overrides.repoprompt_plan_required = Some(false);
    worker_overrides.repoprompt_tool_injection = Some(false);
    worker_overrides
}

// Preflight check: require workspace_root to be gitignored if inside repo
fn preflight_parallel_workspace_root_is_gitignored(
    repo_root: &Path,
    workspace_root: &Path,
) -> Result<()> {
    // Only enforce when workspace_root is inside the repo.
    let Ok(rel) = workspace_root.strip_prefix(repo_root) else {
        return Ok(());
    };

    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let rel_trimmed = rel_str.trim_matches('/');

    // If workspace_root == repo_root, that effectively asks to ignore the whole repo (nonsense).
    if rel_trimmed.is_empty() {
        bail!(
            "Parallel preflight: parallel.workspace_root resolves to the repo root ({}). Refusing to run.",
            repo_root.display()
        );
    }

    // Check ignore rules without creating the directory:
    let dir_candidate = rel_trimmed.to_string();
    let dummy_candidate = format!("{}/__ralph_ignore_probe__", rel_trimmed);

    let ignored_dir = git::is_path_ignored(repo_root, &dir_candidate)
        .with_context(|| format!("Parallel preflight: check-ignore {}", dir_candidate))?;
    let ignored_dummy = git::is_path_ignored(repo_root, &dummy_candidate)
        .with_context(|| format!("Parallel preflight: check-ignore {}", dummy_candidate))?;

    if ignored_dir || ignored_dummy {
        return Ok(());
    }

    let ignore_rule = format!("{}/", rel_trimmed.trim_end_matches('/'));
    bail!(
        "Parallel preflight: parallel.workspace_root resolves inside the repo but is not gitignored.\n\
workspace_root: {}\n\
repo_root: {}\n\
\n\
Ralph will create clone workspaces under this directory, which would leave untracked files and make the repo appear dirty.\n\
\n\
Fix options:\n\
1) Recommended: set parallel.workspace_root to an absolute path OUTSIDE the repo (or remove it to use the default outside-repo location).\n\
2) If you intentionally keep workspaces inside the repo, ignore it:\n\
   - Shared (tracked): add `{}` to `.gitignore` and commit it\n\
   - Local-only: add `{}` to `.git/info/exclude`\n",
        workspace_root.display(),
        repo_root.display(),
        ignore_rule,
        ignore_rule
    );
}

// Worker spawning helper
fn spawn_worker_with_registered_workspace<CreateWorkspace, SyncWorkspace, SpawnWorker>(
    guard: &mut ParallelCleanupGuard,
    task_id: &str,
    create_workspace: CreateWorkspace,
    sync_workspace: SyncWorkspace,
    spawn: SpawnWorker,
) -> Result<(git::WorkspaceSpec, std::process::Child)>
where
    CreateWorkspace: FnOnce() -> Result<git::WorkspaceSpec>,
    SyncWorkspace: FnOnce(&Path) -> Result<()>,
    SpawnWorker: FnOnce(&git::WorkspaceSpec) -> Result<std::process::Child>,
{
    let workspace = create_workspace()?;
    guard.register_workspace(task_id.to_string(), workspace.clone());
    sync_workspace(&workspace.path)?;
    let child = spawn(&workspace)?;
    Ok((workspace, child))
}

// Task pruning: remove stale records
fn prune_stale_workers(state_file: &mut state::ParallelStateFile) -> Vec<String> {
    let now = time::OffsetDateTime::now_utc();
    let ttl_secs: i64 = crate::constants::timeouts::PARALLEL_FINISHED_WITHOUT_PR_BLOCKER_TTL
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);

    let mut dropped = Vec::new();
    state_file.workers.retain(|worker| {
        // Don't prune active workers
        if !worker.is_terminal() {
            return true;
        }

        // Check if workspace still exists
        if !worker.workspace_path.exists() {
            dropped.push(worker.task_id.clone());
            return false;
        }

        // Time-bound terminal workers so they don't block capacity forever
        let Some(started_at) = timeutil::parse_rfc3339_opt(&worker.started_at) else {
            log::warn!(
                "Dropping stale worker {} with invalid started_at (workspace: {}).",
                worker.task_id,
                worker.workspace_path.display()
            );
            dropped.push(worker.task_id.clone());
            return false;
        };

        let age_secs = (now.unix_timestamp() - started_at.unix_timestamp()).max(0);
        if age_secs >= ttl_secs {
            log::warn!(
                "Dropping stale worker {} after TTL (age_secs={}, ttl_secs={}, started_at='{}', workspace: {}).",
                worker.task_id,
                age_secs,
                ttl_secs,
                worker.started_at,
                worker.workspace_path.display()
            );
            dropped.push(worker.task_id.clone());
            return false;
        }

        true
    });
    dropped
}

// Capacity tracking
fn effective_active_worker_count(
    state_file: &state::ParallelStateFile,
    guard_in_flight_len: usize,
) -> usize {
    state_file.active_worker_count().max(guard_in_flight_len)
}

fn initial_tasks_started(state_file: &state::ParallelStateFile) -> u32 {
    let active = state_file.active_worker_count();
    u32::try_from(active).unwrap_or(u32::MAX)
}

fn can_start_more_tasks(tasks_started: u32, max_tasks: u32) -> bool {
    max_tasks == 0 || tasks_started < max_tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tempfile::TempDir;

    fn create_test_cleanup_guard(temp: &TempDir) -> ParallelCleanupGuard {
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).expect("create workspace root");

        let state_path = temp.path().join("state.json");
        let state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        ParallelCleanupGuard::new_simple(state_path, state_file, workspace_root)
    }

    #[test]
    fn prune_stale_workers_drops_missing_workspace() -> Result<()> {
        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/nonexistent/path/RQ-0001"),
            "2026-02-20T00:00:00Z".to_string(),
        );
        worker.mark_completed("2026-02-20T01:00:00Z".to_string());
        state_file.upsert_worker(worker);

        let dropped = prune_stale_workers(&mut state_file);

        assert_eq!(dropped, vec!["RQ-0001"]);
        assert!(state_file.workers.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_workers_retains_active() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0002");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        // Active worker (not terminal)
        let worker = WorkerRecord::new(
            "RQ-0002",
            workspace_path,
            timeutil::now_utc_rfc3339_or_fallback(),
        );
        state_file.upsert_worker(worker);

        let dropped = prune_stale_workers(&mut state_file);

        assert!(dropped.is_empty());
        assert_eq!(state_file.workers.len(), 1);
        Ok(())
    }

    #[test]
    fn spawn_failure_cleans_registered_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_cleanup_guard(&temp);
        let workspace_root = temp.path().join("workspaces");
        let workspace_path = workspace_root.join("RQ-0001");

        let result = spawn_worker_with_registered_workspace(
            &mut guard,
            "RQ-0001",
            || {
                std::fs::create_dir_all(&workspace_path)?;
                Ok(git::WorkspaceSpec {
                    path: workspace_path.clone(),
                    branch: "ralph/RQ-0001".to_string(),
                })
            },
            |_| Ok(()),
            |_| Err(anyhow::anyhow!("spawn failed")),
        );

        assert!(result.is_err());
        guard.cleanup()?;
        assert!(!workspace_path.exists());
        Ok(())
    }

    #[test]
    fn sync_failure_cleans_registered_workspace_without_spawning() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_cleanup_guard(&temp);
        let workspace_root = temp.path().join("workspaces");
        let workspace_path = workspace_root.join("RQ-0002");
        let spawn_called = Cell::new(false);

        let result = spawn_worker_with_registered_workspace(
            &mut guard,
            "RQ-0002",
            || {
                std::fs::create_dir_all(&workspace_path)?;
                Ok(git::WorkspaceSpec {
                    path: workspace_path.clone(),
                    branch: "ralph/RQ-0002".to_string(),
                })
            },
            |_| Err(anyhow::anyhow!("sync failed")),
            |_| {
                spawn_called.set(true);
                Err(anyhow::anyhow!("spawn should not run"))
            },
        );

        assert!(result.is_err());
        assert!(!spawn_called.get());
        guard.cleanup()?;
        assert!(!workspace_path.exists());
        Ok(())
    }

    #[test]
    fn effective_active_worker_count_uses_max() {
        let state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        // With empty state and guard_in_flight=2, should return 2
        assert_eq!(effective_active_worker_count(&state_file, 2), 2);

        // With empty state and guard_in_flight=0, should return 0
        assert_eq!(effective_active_worker_count(&state_file, 0), 0);
    }

    #[test]
    fn can_start_more_tasks_logic() {
        // max_tasks=0 means unlimited
        assert!(can_start_more_tasks(100, 0));

        // With max_tasks=5
        assert!(can_start_more_tasks(0, 5));
        assert!(can_start_more_tasks(4, 5));
        assert!(!can_start_more_tasks(5, 5));
        assert!(!can_start_more_tasks(6, 5));
    }
}
