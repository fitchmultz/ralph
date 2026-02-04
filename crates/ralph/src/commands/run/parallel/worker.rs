//! Worker lifecycle management for parallel task execution.
//!
//! Responsibilities:
//! - Select runnable tasks from the queue for parallel execution.
//! - Spawn worker processes in isolated git workspaces.
//! - Track worker state and provide graceful termination.
//!
//! Not handled here:
//! - PR creation or merge logic (see `super::merge_runner`).
//! - State persistence (see `super::state`).
//! - CLI argument construction (see `super::args`).
//!
//! Invariants/assumptions:
//! - Workers run in isolated workspaces with dedicated branches.
//! - Task selection respects queue order and exclusion sets.

use crate::agent::AgentOverrides;
use crate::commands::run::parallel::args::build_override_args;
use crate::commands::run::selection::select_run_one_task_index_excluding;
use crate::config;
use crate::constants::paths::ENV_FORCE_COMPLETION_SIGNAL;
use crate::git::WorkspaceSpec;
use crate::lock::DirLock;
use crate::queue;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use time::OffsetDateTime;

use super::state;

/// Tracks the state of an in-flight worker process.
pub(crate) struct WorkerState {
    pub task_id: String,
    pub task_title: String,
    pub workspace: WorkspaceSpec,
    pub child: Child,
}

/// Select the next runnable task from the queue, requiring the caller to hold the queue lock.
///
/// The `_queue_lock` parameter enforces at compile time that the caller holds the lock.
/// This prevents race conditions during task selection in parallel mode.
pub(crate) fn select_next_task_locked(
    resolved: &config::Resolved,
    include_draft: bool,
    excluded_ids: &HashSet<String>,
    _queue_lock: &DirLock,
) -> Result<Option<(String, String)>> {
    let done_path_exists = resolved.done_path.exists();
    let done = if done_path_exists {
        queue::load_queue_with_repair(&resolved.done_path)?
    } else {
        crate::contracts::QueueFile::default()
    };
    let done_ref = if done.tasks.is_empty() && !done_path_exists {
        None
    } else {
        Some(&done)
    };

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let (queue_file, warnings) = queue::load_queue_with_repair_and_validate(
        &resolved.queue_path,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    let idx =
        select_run_one_task_index_excluding(&queue_file, done_ref, include_draft, excluded_ids)?;
    let idx = match idx {
        Some(idx) => idx,
        None => return Ok(None),
    };
    let task = &queue_file.tasks[idx];
    Ok(Some((
        task.id.trim().to_string(),
        task.title.trim().to_string(),
    )))
}

/// Collect IDs that should be excluded from task selection (in-flight, open PRs, blocking finished-without-PR).
pub(crate) fn collect_excluded_ids(
    state_file: &state::ParallelStateFile,
    in_flight: &HashMap<String, WorkerState>,
    now: OffsetDateTime,
    auto_pr_enabled: bool,
    draft_on_failure: bool,
) -> HashSet<String> {
    let mut excluded = HashSet::new();
    for key in in_flight.keys() {
        excluded.insert(key.trim().to_string());
    }
    for record in &state_file.tasks_in_flight {
        excluded.insert(record.task_id.trim().to_string());
    }
    // Only exclude tasks with PRs that are still open and not merged
    for record in &state_file.prs {
        if record.is_open_unmerged() {
            excluded.insert(record.task_id.trim().to_string());
        }
    }
    // Only exclude finished-without-PR records that are currently blocking
    for record in &state_file.finished_without_pr {
        if record.is_blocking(now, auto_pr_enabled, draft_on_failure) {
            excluded.insert(record.task_id.trim().to_string());
        }
    }
    excluded
}

/// Terminate all in-flight workers gracefully.
pub(crate) fn terminate_workers(in_flight: &mut HashMap<String, WorkerState>) {
    for worker in in_flight.values_mut() {
        if let Err(err) = worker.child.kill() {
            log::warn!("Failed to terminate worker {}: {}", worker.task_id, err);
        }
    }

    for worker in in_flight.values_mut() {
        let _ = worker.child.wait();
    }
}

/// Spawn a worker process for the given task in the specified workspace.
pub(crate) fn spawn_worker(
    _resolved: &config::Resolved,
    workspace_path: &Path,
    task_id: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<Child> {
    let (mut cmd, args) = build_worker_command(workspace_path, task_id, overrides, force)?;
    log::debug!(
        "Spawning parallel worker {} in {} with args: {:?}",
        task_id,
        workspace_path.display(),
        args
    );
    cmd.args(args);
    let child = cmd.spawn().context("spawn parallel worker")?;
    Ok(child)
}

/// Build the command and arguments for a worker subprocess.
fn build_worker_command(
    workspace_path: &Path,
    task_id: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<(Command, Vec<String>)> {
    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut cmd = Command::new(exe);
    cmd.current_dir(workspace_path);
    cmd.env("PWD", workspace_path);
    cmd.env(crate::config::REPO_ROOT_OVERRIDE_ENV, workspace_path);
    cmd.env(ENV_FORCE_COMPLETION_SIGNAL, "1");
    cmd.stdin(Stdio::null());

    let mut args: Vec<String> = Vec::new();
    if force {
        args.push("--force".to_string());
    }
    args.push("--no-progress".to_string());
    args.push("run".to_string());
    args.push("one".to_string());
    args.push("--id".to_string());
    args.push(task_id.to_string());
    args.push("--parallel-worker".to_string());
    args.push("--non-interactive".to_string());

    args.extend(build_override_args(overrides));

    Ok((cmd, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn build_worker_command_sets_cwd_and_args() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_path)?;

        let overrides = AgentOverrides::default();
        let (cmd, args) = build_worker_command(&workspace_path, "RQ-1234", &overrides, true)?;

        assert_eq!(cmd.get_current_dir(), Some(workspace_path.as_path()));

        let mut pwd_seen = false;
        let mut override_seen = false;
        let mut force_seen = false;
        for (key, value) in cmd.get_envs() {
            if key == std::ffi::OsStr::new("PWD") {
                pwd_seen = true;
                assert_eq!(value, Some(workspace_path.as_os_str()));
            }
            if key == std::ffi::OsStr::new(crate::config::REPO_ROOT_OVERRIDE_ENV) {
                override_seen = true;
                assert_eq!(value, Some(workspace_path.as_os_str()));
            }
            if key == std::ffi::OsStr::new(ENV_FORCE_COMPLETION_SIGNAL) {
                force_seen = true;
                assert_eq!(value, Some(std::ffi::OsStr::new("1")));
            }
        }
        assert!(pwd_seen, "PWD env should be set for workspace execution");
        assert!(
            override_seen,
            "{} env should be set for workspace execution",
            crate::config::REPO_ROOT_OVERRIDE_ENV
        );
        assert!(
            force_seen,
            "{} env should be set for workspace execution",
            ENV_FORCE_COMPLETION_SIGNAL
        );

        assert!(args.contains(&"--force".to_string()));
        assert!(args.contains(&"--no-progress".to_string()));
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"one".to_string()));
        assert!(args.contains(&"--parallel-worker".to_string()));
        assert!(args.contains(&"--non-interactive".to_string()));
        // Default overrides should not emit git-commit-push flags
        assert!(!args.contains(&"--git-commit-push-on".to_string()));
        assert!(!args.contains(&"--git-commit-push-off".to_string()));

        let id_pos = args.iter().position(|arg| arg == "--id").expect("--id");
        assert_eq!(args.get(id_pos + 1), Some(&"RQ-1234".to_string()));

        Ok(())
    }

    #[test]
    fn build_worker_command_emits_git_commit_push_on_when_overridden() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_path)?;

        let overrides = AgentOverrides {
            git_commit_push_enabled: Some(true),
            ..Default::default()
        };
        let (_cmd, args) = build_worker_command(&workspace_path, "RQ-1234", &overrides, false)?;

        assert!(args.contains(&"--git-commit-push-on".to_string()));
        assert!(!args.contains(&"--git-commit-push-off".to_string()));

        Ok(())
    }

    #[test]
    fn build_worker_command_emits_git_commit_push_off_when_overridden() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_path)?;

        let overrides = AgentOverrides {
            git_commit_push_enabled: Some(false),
            ..Default::default()
        };
        let (_cmd, args) = build_worker_command(&workspace_path, "RQ-1234", &overrides, false)?;

        assert!(args.contains(&"--git-commit-push-off".to_string()));
        assert!(!args.contains(&"--git-commit-push-on".to_string()));

        Ok(())
    }

    #[test]
    fn collect_excluded_ids_includes_state_and_in_flight() -> Result<()> {
        use crate::timeutil;

        let temp = TempDir::new()?;
        let ws_path = temp.path().join("workspace");
        std::fs::create_dir_all(&ws_path)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: "/tmp/workspace/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(123),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        // Open PR should be excluded
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 7,
            pr_url: "https://example.com/pr/7".to_string(),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Open,
            merge_blocker: None,
        });
        // Closed PR should NOT be excluded
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0005".to_string(),
            pr_number: 8,
            pr_url: "https://example.com/pr/8".to_string(),
            head: Some("ralph/RQ-0005".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Closed,
            merge_blocker: None,
        });
        // Merged PR should NOT be excluded
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0006".to_string(),
            pr_number: 9,
            pr_url: "https://example.com/pr/9".to_string(),
            head: Some("ralph/RQ-0006".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: true,
            lifecycle: state::ParallelPrLifecycle::Merged,
            merge_blocker: None,
        });
        // Finished without PR should be excluded (when auto_pr is disabled)
        state_file
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-0007".to_string(),
                workspace_path: ws_path.to_string_lossy().to_string(),
                branch: "ralph/RQ-0007".to_string(),
                success: true,
                finished_at: "2026-02-02T00:00:00Z".to_string(),
                reason: state::ParallelNoPrReason::AutoPrDisabled,
                message: None,
            });

        let mut in_flight = HashMap::new();
        let child = std::process::Command::new("true").spawn()?;
        in_flight.insert(
            "RQ-0004".to_string(),
            WorkerState {
                task_id: "RQ-0004".to_string(),
                task_title: "title".to_string(),
                workspace: WorkspaceSpec {
                    path: PathBuf::from("/tmp/workspaces/RQ-0004"),
                    branch: "ralph/RQ-0004".to_string(),
                },
                child,
            },
        );

        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z")?;
        // auto_pr_enabled=false, so AutoPrDisabled records should block
        let excluded = collect_excluded_ids(&state_file, &in_flight, now, false, true);
        assert!(
            excluded.contains("RQ-0002"),
            "in-flight task should be excluded"
        );
        assert!(
            excluded.contains("RQ-0003"),
            "open PR task should be excluded"
        );
        assert!(
            excluded.contains("RQ-0004"),
            "in-flight worker should be excluded"
        );
        assert!(
            excluded.contains("RQ-0007"),
            "finished-without-PR task should be excluded when auto_pr is disabled"
        );
        assert!(
            !excluded.contains("RQ-0005"),
            "closed PR task should NOT be excluded"
        );
        assert!(
            !excluded.contains("RQ-0006"),
            "merged PR task should NOT be excluded"
        );

        // With auto_pr enabled, AutoPrDisabled records should NOT block
        let excluded_enabled = collect_excluded_ids(&state_file, &in_flight, now, true, true);
        assert!(
            !excluded_enabled.contains("RQ-0007"),
            "finished-without-PR task should NOT be excluded when auto_pr is enabled"
        );

        for worker in in_flight.values_mut() {
            let _ = worker.child.wait();
        }

        Ok(())
    }

    #[test]
    fn collect_excluded_ids_finished_without_pr_is_conditional_on_settings() -> Result<()> {
        use crate::timeutil;

        let temp = TempDir::new()?;
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(&ws)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-0007".to_string(),
                workspace_path: ws.to_string_lossy().to_string(),
                branch: "ralph/RQ-0007".to_string(),
                success: true,
                finished_at: "2026-02-02T00:00:00Z".to_string(),
                reason: state::ParallelNoPrReason::AutoPrDisabled,
                message: None,
            });

        let in_flight = HashMap::new();
        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z")?;

        let excluded_when_disabled =
            collect_excluded_ids(&state_file, &in_flight, now, false, true);
        assert!(excluded_when_disabled.contains("RQ-0007"));

        let excluded_when_enabled = collect_excluded_ids(&state_file, &in_flight, now, true, true);
        assert!(!excluded_when_enabled.contains("RQ-0007"));

        Ok(())
    }

    #[test]
    fn collect_excluded_ids_pr_create_failed_expires() -> Result<()> {
        use crate::timeutil;

        let temp = TempDir::new()?;
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(&ws)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        state_file
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-FAIL-NEW".to_string(),
                workspace_path: ws.to_string_lossy().to_string(),
                branch: "ralph/RQ-FAIL-NEW".to_string(),
                success: true,
                finished_at: "2026-02-02T23:30:00Z".to_string(),
                reason: state::ParallelNoPrReason::PrCreateFailed,
                message: None,
            });

        state_file
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-FAIL-OLD".to_string(),
                workspace_path: ws.to_string_lossy().to_string(),
                branch: "ralph/RQ-FAIL-OLD".to_string(),
                success: true,
                finished_at: "2020-01-01T00:00:00Z".to_string(),
                reason: state::ParallelNoPrReason::PrCreateFailed,
                message: None,
            });

        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z")?;
        let excluded = collect_excluded_ids(&state_file, &HashMap::new(), now, true, true);

        assert!(excluded.contains("RQ-FAIL-NEW"));
        assert!(!excluded.contains("RQ-FAIL-OLD"));

        Ok(())
    }

    #[test]
    fn select_next_task_locked_works_under_held_lock() -> Result<()> {
        use crate::config;
        use crate::contracts::{QueueFile, Task, TaskStatus};
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        let repo_root = temp.path().to_path_buf();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        // Create a queue with one todo task
        let queue_path = ralph_dir.join("queue.json");
        let mut queue_file = QueueFile::default();
        queue_file.tasks.push(Task {
            id: "RQ-0001".to_string(),
            title: "Test task".to_string(),
            status: TaskStatus::Todo,
            priority: crate::contracts::TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
        });
        queue::save_queue(&queue_path, &queue_file)?;

        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path: queue_path.clone(),
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        // Acquire the queue lock (as the parallel supervisor would)
        let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;

        // Call select_next_task_locked with the held lock
        let excluded = HashSet::new();
        let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

        // Should return the todo task
        assert!(result.is_some());
        let (task_id, task_title) = result.unwrap();
        assert_eq!(task_id, "RQ-0001");
        assert_eq!(task_title, "Test task");

        Ok(())
    }

    #[test]
    fn select_next_task_locked_returns_none_when_no_tasks() -> Result<()> {
        use crate::config;
        use crate::contracts::QueueFile;
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        let repo_root = temp.path().to_path_buf();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        // Create an empty queue
        let queue_path = ralph_dir.join("queue.json");
        let queue_file = QueueFile::default();
        queue::save_queue(&queue_path, &queue_file)?;

        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path: queue_path.clone(),
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        // Acquire the queue lock
        let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;

        // Call select_next_task_locked with the held lock
        let excluded = HashSet::new();
        let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

        // Should return None since no tasks are available
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn parallel_select_next_task_locked_repairs_trailing_commas() -> Result<()> {
        use crate::config;
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        let repo_root = temp.path().to_path_buf();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test task", "status": "todo", "tags": ["bug",], "scope": ["file",], "evidence": ["observed",], "plan": ["do thing",], "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",}]}"#;
        std::fs::write(&queue_path, malformed)?;

        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path,
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
        let excluded = HashSet::new();
        let result = select_next_task_locked(&resolved, false, &excluded, &queue_lock)?;

        assert!(result.is_some());
        let (task_id, task_title) = result.unwrap();
        assert_eq!(task_id, "RQ-0001");
        assert_eq!(task_title, "Test task");

        Ok(())
    }

    #[test]
    fn parallel_select_next_task_locked_rejects_semantically_invalid_queue() -> Result<()> {
        use crate::config;
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        let repo_root = temp.path().to_path_buf();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        // Intentionally missing created_at / updated_at (should fail semantic validation).
        let invalid = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test task", "status": "todo", "tags": ["bug"], "scope": ["file"], "evidence": [], "plan": []}]}"#;
        std::fs::write(&queue_path, invalid)?;

        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path,
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let queue_lock = queue::acquire_queue_lock(&repo_root, "test", false)?;
        let excluded = HashSet::new();

        let err = select_next_task_locked(&resolved, false, &excluded, &queue_lock)
            .expect_err("expected semantic validation failure");
        let err_msg = err
            .chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            err_msg.contains("created_at") || err_msg.contains("updated_at"),
            "error should mention missing timestamps: {err_msg}"
        );

        Ok(())
    }
}
