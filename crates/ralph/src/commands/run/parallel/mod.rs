//! Parallel run loop supervisor and worker orchestration.
//!
//! Responsibilities:
//! - Coordinate parallel task execution across multiple workers.
//! - Manage settings resolution and preflight validation.
//! - Track worker capacity and task pruning.
//! - Spawn merge-agent subprocess for PR merging (new architecture).
//!
//! Not handled here:
//! - Main orchestration loop (see `orchestration`).
//! - State initialization (see `state_init`).
//! - Queue sync after merges (see `queue_sync`).
//! - CLI parsing (see `crate::cli::run`).
//! - Worker lifecycle (see `worker`).
//! - State persistence format (see `state`).
//!
//! Invariants/assumptions:
//! - Queue order is authoritative for task selection.
//! - Workers run in isolated workspaces with dedicated branches.
//! - PR creation relies on authenticated `gh` CLI access.
//! - One active worker per task ID (enforced by upsert_task).
//! - One pending merge per task ID (enforced by enqueue_merge).

use crate::agent::AgentOverrides;
use crate::commands::run::merge_agent::{MergeAgentResult, exit_codes};
use crate::config;
use crate::contracts::{ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, ParallelMergeWhen};
use crate::{git, timeutil};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Stdio;

mod args;
mod cleanup_guard;
mod merge_runner;
mod orchestration;
mod path_map;
mod queue_sync;
pub mod state;
mod state_init;
mod sync;
mod worker;

// Re-export public APIs from submodules
pub(crate) use orchestration::run_loop_parallel;

use cleanup_guard::ParallelCleanupGuard;
use state_init::load_or_init_parallel_state;

// =============================================================================
// Merge-Agent Subprocess Helpers (new architecture)
// =============================================================================

/// Result of invoking merge-agent subprocess.
#[derive(Debug, Clone)]
pub(crate) struct MergeAgentOutcome {
    /// Exit code from the merge-agent subprocess.
    pub exit_code: i32,
    /// Parsed JSON result from stdout (if available).
    /// Note: Currently unused but kept for future extensibility.
    #[allow(dead_code)]
    pub result: Option<MergeAgentResult>,
    /// Stderr output (for diagnostics).
    pub stderr_output: String,
}

/// Classification of merge-agent exit codes for retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeExitClassification {
    /// Merge succeeded (exit code 0).
    Success,
    /// Task already finalized (exit code 6) - idempotent success.
    AlreadyFinalized,
    /// Merge conflict - retryable after resolution (exit code 3).
    ConflictRetryable,
    /// Runtime failure - retryable with backoff (exit code 1).
    RuntimeRetryable,
    /// Terminal failure - non-retryable (exit codes 2, 4, 5, or others).
    TerminalFailure,
}

/// Spawn merge-agent as a subprocess and wait for completion.
///
/// This replaces the internal merge-runner thread with explicit process boundaries.
/// The merge-agent runs in the coordinator repo context (CWD) and returns
/// structured JSON to stdout.
///
/// # Arguments
/// * `repo_root` - Repository root path (CWD for merge-agent)
/// * `task_id` - Task ID to finalize
/// * `pr_number` - PR number to merge
///
/// # Returns
/// A `MergeAgentOutcome` containing exit code, parsed result, and stderr output.
pub(crate) fn spawn_merge_agent(
    repo_root: &Path,
    task_id: &str,
    pr_number: u32,
) -> Result<MergeAgentOutcome> {
    let exe = std::env::current_exe().context("resolve current executable")?;

    let output = std::process::Command::new(exe)
        .current_dir(repo_root)
        .args([
            "run",
            "merge-agent",
            "--task",
            task_id,
            "--pr",
            &pr_number.to_string(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| {
            format!(
                "Failed to spawn merge-agent for task {} PR {}",
                task_id, pr_number
            )
        })?;

    let exit_code = output.status.code().unwrap_or(1);
    let stderr_output = String::from_utf8_lossy(&output.stderr).to_string();

    // Parse JSON result from stdout (may be empty on validation errors)
    let result = if !output.stdout.is_empty() {
        serde_json::from_slice::<MergeAgentResult>(&output.stdout)
            .ok()
            .or_else(|| {
                log::warn!(
                    "merge-agent stdout for {} was not valid JSON: {}",
                    task_id,
                    String::from_utf8_lossy(&output.stdout)
                );
                None
            })
    } else {
        None
    };

    Ok(MergeAgentOutcome {
        exit_code,
        result,
        stderr_output,
    })
}

/// Classify merge-agent exit code for retry decision.
///
/// Exit codes (per merge_agent.rs):
/// - 0: Success
/// - 1: Runtime failure (retryable with backoff)
/// - 2: Validation failure (non-retryable)
/// - 3: Merge conflict (retryable after conflict resolution)
/// - 4: PR not found/closed (non-retryable)
/// - 5: PR is draft (non-retryable)
/// - 6: Already finalized (idempotent success)
pub(crate) fn classify_merge_exit_code(code: i32) -> MergeExitClassification {
    match code {
        exit_codes::SUCCESS => MergeExitClassification::Success,
        exit_codes::ALREADY_FINALIZED => MergeExitClassification::AlreadyFinalized,
        exit_codes::MERGE_CONFLICT => MergeExitClassification::ConflictRetryable,
        exit_codes::RUNTIME_FAILURE => MergeExitClassification::RuntimeRetryable,
        _ => MergeExitClassification::TerminalFailure,
    }
}

pub(crate) struct ParallelRunOptions {
    pub max_tasks: u32,
    pub workers: u8,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
    pub merge_when: ParallelMergeWhen,
}

pub(crate) struct ParallelSettings {
    pub(crate) workers: u8,
    pub(crate) merge_when: ParallelMergeWhen,
    pub(crate) merge_method: ParallelMergeMethod,
    pub(crate) auto_pr: bool,
    pub(crate) auto_merge: bool,
    pub(crate) draft_on_failure: bool,
    #[allow(dead_code)]
    pub(crate) conflict_policy: ConflictPolicy,
    pub(crate) merge_retries: u8,
    pub(crate) workspace_root: PathBuf,
    pub(crate) branch_prefix: String,
    #[allow(dead_code)]
    pub(crate) delete_branch_on_merge: bool,
    #[allow(dead_code)]
    pub(crate) merge_runner: MergeRunnerConfig,
}

// Settings resolution (stays in mod.rs)
fn resolve_parallel_settings(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
) -> Result<ParallelSettings> {
    let cfg = &resolved.config.parallel;
    Ok(ParallelSettings {
        workers: opts.workers,
        merge_when: opts.merge_when,
        merge_method: cfg.merge_method.unwrap_or(ParallelMergeMethod::Squash),
        auto_pr: cfg.auto_pr.unwrap_or(true),
        auto_merge: cfg.auto_merge.unwrap_or(true),
        draft_on_failure: cfg.draft_on_failure.unwrap_or(true),
        conflict_policy: cfg.conflict_policy.unwrap_or(ConflictPolicy::AutoResolve),
        merge_retries: cfg.merge_retries.unwrap_or(5),
        workspace_root: git::workspace_root(&resolved.repo_root, &resolved.config),
        branch_prefix: cfg
            .branch_prefix
            .clone()
            .unwrap_or_else(|| "ralph/".to_string()),
        delete_branch_on_merge: cfg.delete_branch_on_merge.unwrap_or(true),
        merge_runner: cfg.merge_runner.clone().unwrap_or_default(),
    })
}

fn apply_git_commit_push_policy_to_parallel_settings(
    settings: &mut ParallelSettings,
    git_commit_push_enabled: bool,
) {
    if !git_commit_push_enabled {
        settings.auto_pr = false;
        settings.auto_merge = false;
        settings.draft_on_failure = false;
    }
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

// Preflight check (stays in mod.rs)
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
    // - check the directory path itself
    // - and a dummy child to ensure `foo/` directory patterns match
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

// Worker spawning helper (stays in mod.rs)
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

// Task pruning (stays in mod.rs - called by orchestration loop)
fn prune_stale_tasks_in_flight(state_file: &mut state::ParallelStateFile) -> Vec<String> {
    let now = time::OffsetDateTime::now_utc();
    let ttl_secs: i64 = crate::constants::timeouts::PARALLEL_FINISHED_WITHOUT_PR_BLOCKER_TTL
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);

    let mut dropped = Vec::new();
    state_file.tasks_in_flight.retain(|record| {
        let path = Path::new(&record.workspace_path);
        if !path.exists() {
            dropped.push(record.task_id.clone());
            return false;
        }

        if let Some(pid) = record.pid {
            // Only prune when definitively dead; retain when running or indeterminate.
            if crate::lock::pid_liveness(pid).is_definitely_not_running() {
                dropped.push(record.task_id.clone());
                return false;
            }
            return true;
        }

        // PID is missing: time-bound it so it can't block capacity forever.
        let Some(started_at) = timeutil::parse_rfc3339_opt(&record.started_at) else {
            log::warn!(
                "Dropping stale in-flight task {} with missing pid: missing/invalid started_at (workspace: {}).",
                record.task_id,
                record.workspace_path
            );
            dropped.push(record.task_id.clone());
            return false;
        };

        let age_secs = (now.unix_timestamp() - started_at.unix_timestamp()).max(0);
        if age_secs >= ttl_secs {
            log::warn!(
                "Dropping stale in-flight task {} with missing pid after TTL (age_secs={}, ttl_secs={}, started_at='{}', workspace: {}).",
                record.task_id,
                age_secs,
                ttl_secs,
                record.started_at,
                record.workspace_path
            );
            dropped.push(record.task_id.clone());
            return false;
        }

        true
    });
    dropped
}

// Capacity tracking (stays in mod.rs)
fn effective_in_flight_count(
    state_file: &state::ParallelStateFile,
    guard_in_flight_len: usize,
) -> usize {
    state_file.tasks_in_flight.len().max(guard_in_flight_len)
}

fn initial_tasks_started(
    state_file: &state::ParallelStateFile,
    _now: time::OffsetDateTime,
    _auto_pr_enabled: bool,
    _draft_on_failure: bool,
) -> u32 {
    let open_unmerged_prs = state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
        .count();

    let total = state_file
        .tasks_in_flight
        .len()
        .saturating_add(open_unmerged_prs);

    u32::try_from(total).unwrap_or(u32::MAX)
}

fn can_start_more_tasks(tasks_started: u32, max_tasks: u32) -> bool {
    max_tasks == 0 || tasks_started < max_tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, ParallelMergeWhen,
    };
    use std::cell::Cell;
    use tempfile::TempDir;

    fn create_test_cleanup_guard(temp: &TempDir) -> ParallelCleanupGuard {
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).expect("create workspace root");

        let state_path = temp.path().join("state.json");
        let state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        ParallelCleanupGuard::new_simple(state_path, state_file, workspace_root)
    }

    #[test]
    fn prune_stale_tasks_drops_missing_workspace() -> Result<()> {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: "/nonexistent/path/RQ-0001".to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-0001"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_dead_pid_with_existing_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0002");
        std::fs::create_dir_all(&workspace_path)?;

        // Spawn a short-lived process and wait for it to exit
        let mut child = std::process::Command::new("true").spawn()?;
        let pid = child.id();
        child.wait()?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(pid),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-0002"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_retains_missing_pid_within_ttl() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0003");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the record is within TTL
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0003".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0003".to_string(),
            pid: None,
            started_at: recent_timestamp,
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert!(dropped.is_empty());
        assert_eq!(state_file.tasks_in_flight.len(), 1);
        assert_eq!(state_file.tasks_in_flight[0].task_id, "RQ-0003");
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_missing_pid_beyond_ttl() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-OLD");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Use a very old timestamp (well beyond the 24h TTL)
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-OLD".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-OLD".to_string(),
            pid: None,
            started_at: "2020-01-01T00:00:00Z".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-OLD"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_missing_pid_with_missing_started_at() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-LEGACY");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Simulate a legacy record with missing started_at (empty string)
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-LEGACY".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-LEGACY".to_string(),
            pid: None,
            started_at: "".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-LEGACY"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_retains_running_pid_with_existing_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0004");
        std::fs::create_dir_all(&workspace_path)?;

        // Spawn a long-running process (sleep) that will still be running
        let child = std::process::Command::new("sleep").arg("10").spawn()?;
        let pid = child.id();

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0004".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0004".to_string(),
            pid: Some(pid),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        // Clean up the child process
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();

        assert!(dropped.is_empty());
        assert_eq!(state_file.tasks_in_flight.len(), 1);
        assert_eq!(state_file.tasks_in_flight[0].task_id, "RQ-0004");
        Ok(())
    }

    #[test]
    fn resume_in_flight_counts_toward_max_tasks() -> Result<()> {
        use crate::timeutil;

        let temp = TempDir::new()?;
        let ws_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&ws_root)?;

        // Create workspace directories so records are considered blocking
        let ws1 = ws_root.join("RQ-0001");
        let ws2 = ws_root.join("RQ-0002");
        std::fs::create_dir_all(&ws1)?;
        std::fs::create_dir_all(&ws2)?;

        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z")?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Simulate 2 tasks in flight from resumed state
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: ws1.to_string_lossy().to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: ws2.to_string_lossy().to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12346),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        // 2 in-flight tasks count as started
        assert_eq!(initial_tasks_started(&state_file, now, false, true), 2);
        assert_eq!(initial_tasks_started(&state_file, now, true, true), 2);

        // With max_tasks = 2, should not be able to start more
        assert!(!can_start_more_tasks(2, 2));

        // With max_tasks = 3, should be able to start more
        assert!(can_start_more_tasks(2, 3));

        // With max_tasks = 0 (unlimited), should be able to start more
        assert!(can_start_more_tasks(2, 0));

        Ok(())
    }

    #[test]
    fn resume_open_prs_count_toward_max_tasks() {
        use crate::timeutil;

        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z").unwrap();

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // One open PR from a previous run should count as "already started"
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0100".to_string(),
            pr_number: 1,
            lifecycle: state::ParallelPrLifecycle::Open,
        });

        // These should NOT count toward started (they are not open)
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0101".to_string(),
            pr_number: 2,
            lifecycle: state::ParallelPrLifecycle::Closed,
        });
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0102".to_string(),
            pr_number: 3,
            lifecycle: state::ParallelPrLifecycle::Merged,
        });

        let started = initial_tasks_started(&state_file, now, true, true);
        assert_eq!(started, 1);

        // With max_tasks=1, we should NOT be allowed to start any new tasks on resume.
        assert!(!can_start_more_tasks(started, 1));

        // With max_tasks=2, we can start one more.
        assert!(can_start_more_tasks(started, 2));
    }

    #[test]
    fn resume_in_flight_counts_toward_worker_capacity() {
        let state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // Test with tasks_in_flight.len() == 2 and guard_in_flight_len == 0
        let state_with_tasks = {
            let mut s = state_file.clone();
            s.tasks_in_flight.push(state::ParallelTaskRecord {
                task_id: "RQ-0001".to_string(),
                workspace_path: "/tmp/ws/RQ-0001".to_string(),
                branch: "ralph/RQ-0001".to_string(),
                pid: Some(12345),
                started_at: "2026-02-02T00:00:00Z".to_string(),
            });
            s.tasks_in_flight.push(state::ParallelTaskRecord {
                task_id: "RQ-0002".to_string(),
                workspace_path: "/tmp/ws/RQ-0002".to_string(),
                branch: "ralph/RQ-0002".to_string(),
                pid: Some(12346),
                started_at: "2026-02-02T00:00:00Z".to_string(),
            });
            s
        };

        // effective_in_flight_count should return 2 (from state file)
        assert_eq!(effective_in_flight_count(&state_with_tasks, 0), 2);

        // With workers_limit == 2, has_capacity should be false
        let has_capacity = effective_in_flight_count(&state_with_tasks, 0) < 2;
        assert!(!has_capacity);

        // With workers_limit == 3, has_capacity should be true
        let has_capacity = effective_in_flight_count(&state_with_tasks, 0) < 3;
        assert!(has_capacity);
    }

    #[test]
    fn capacity_does_not_double_count_guard_and_state() {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: "/tmp/ws/RQ-0001".to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: "/tmp/ws/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12346),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        // With tasks_in_flight.len() == 2 and guard_in_flight_len == 1,
        // effective_in_flight_count should return 2 (max, not sum)
        assert_eq!(effective_in_flight_count(&state_file, 1), 2);

        // With tasks_in_flight.len() == 2 and guard_in_flight_len() == 3,
        // effective_in_flight_count should return 3 (max, not sum)
        assert_eq!(effective_in_flight_count(&state_file, 3), 3);
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

    // ============================================================================
    // Stop signal idle-stop exit tests (RQ-0570)
    // ============================================================================

    /// Test helper: determine if the loop should break based on current state
    /// Mirrors the logic in the main loop for testing purposes
    fn should_exit_loop(
        stop_requested: bool,
        in_flight_is_empty: bool,
        no_more_tasks: bool,
        next_available: bool,
    ) -> bool {
        if in_flight_is_empty {
            // Exit if: max tasks reached, no more tasks available, or stop requested
            no_more_tasks || !next_available || stop_requested
        } else {
            // Don't exit if workers are still in flight
            false
        }
    }

    #[test]
    fn stop_requested_and_idle_should_exit() {
        // stop_requested=true, in_flight_is_empty=true, next_available=true => break
        assert!(should_exit_loop(true, true, false, true));
    }

    #[test]
    fn stop_requested_with_in_flight_should_not_exit() {
        // stop_requested=true, in_flight_is_empty=false => do not break (wait for in-flight)
        assert!(!should_exit_loop(true, false, false, true));
        assert!(!should_exit_loop(true, false, true, false));
        assert!(!should_exit_loop(true, false, true, true));
    }

    #[test]
    fn no_stop_no_next_available_should_exit() {
        // stop_requested=false, in_flight_is_empty=true, next_available=false => break
        assert!(should_exit_loop(false, true, false, false));
    }

    #[test]
    fn no_stop_no_more_tasks_should_exit() {
        // stop_requested=false, in_flight_is_empty=true, no_more_tasks=true => break
        assert!(should_exit_loop(false, true, true, false));
    }

    #[test]
    fn normal_operation_should_not_exit() {
        // stop_requested=false, in_flight_is_empty=true, next_available=true => continue
        assert!(!should_exit_loop(false, true, false, true));
    }

    #[test]
    fn stop_signal_cleared_on_parallel_loop_exit() {
        use crate::signal;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join(".ralph/cache");

        // Create stop signal
        signal::create_stop_signal(&cache_dir).unwrap();
        assert!(signal::stop_signal_exists(&cache_dir));

        // Clear it (simulating what the parallel loop does on exit)
        let cleared = signal::clear_stop_signal(&cache_dir).unwrap();
        assert!(cleared);
        assert!(!signal::stop_signal_exists(&cache_dir));
    }

    #[test]
    fn apply_git_commit_push_policy_leaves_settings_unchanged_when_enabled() {
        let mut settings = ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 5,
            workspace_root: PathBuf::from("/tmp/workspaces"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        };

        // When git_commit_push_enabled is true, settings should remain unchanged
        apply_git_commit_push_policy_to_parallel_settings(&mut settings, true);

        assert!(settings.auto_pr);
        assert!(settings.auto_merge);
        assert!(settings.draft_on_failure);
    }

    #[test]
    fn apply_git_commit_push_policy_disables_pr_automation_when_disabled() {
        let mut settings = ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 5,
            workspace_root: PathBuf::from("/tmp/workspaces"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        };

        // When git_commit_push_enabled is false, PR automation should be disabled
        apply_git_commit_push_policy_to_parallel_settings(&mut settings, false);

        assert!(!settings.auto_pr);
        assert!(!settings.auto_merge);
        assert!(!settings.draft_on_failure);
    }

    // =========================================================================
    // Merge-Agent Subprocess Tests
    // =========================================================================

    #[test]
    fn classify_merge_exit_codes() {
        assert_eq!(
            classify_merge_exit_code(exit_codes::SUCCESS),
            MergeExitClassification::Success
        );
        assert_eq!(
            classify_merge_exit_code(exit_codes::ALREADY_FINALIZED),
            MergeExitClassification::AlreadyFinalized
        );
        assert_eq!(
            classify_merge_exit_code(exit_codes::MERGE_CONFLICT),
            MergeExitClassification::ConflictRetryable
        );
        assert_eq!(
            classify_merge_exit_code(exit_codes::RUNTIME_FAILURE),
            MergeExitClassification::RuntimeRetryable
        );
        // Terminal failures
        assert_eq!(
            classify_merge_exit_code(exit_codes::VALIDATION_FAILURE),
            MergeExitClassification::TerminalFailure
        );
        assert_eq!(
            classify_merge_exit_code(exit_codes::PR_NOT_FOUND),
            MergeExitClassification::TerminalFailure
        );
        assert_eq!(
            classify_merge_exit_code(exit_codes::PR_IS_DRAFT),
            MergeExitClassification::TerminalFailure
        );
        // Unknown codes are terminal
        assert_eq!(
            classify_merge_exit_code(99),
            MergeExitClassification::TerminalFailure
        );
    }

    #[test]
    fn merge_exit_classification_retry_semantics() {
        // Success and already finalized are not retryable (they're done)
        assert!(!matches!(
            MergeExitClassification::Success,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));
        assert!(!matches!(
            MergeExitClassification::AlreadyFinalized,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));

        // Conflict and runtime are retryable
        assert!(matches!(
            MergeExitClassification::ConflictRetryable,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));
        assert!(matches!(
            MergeExitClassification::RuntimeRetryable,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));

        // Terminal is not retryable
        assert!(!matches!(
            MergeExitClassification::TerminalFailure,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));
    }
}
