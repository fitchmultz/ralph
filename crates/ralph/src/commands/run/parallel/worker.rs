//! Worker lifecycle management for parallel task execution (direct-push mode).
//!
//! Responsibilities:
//! - Select runnable tasks from the queue for parallel execution.
//! - Spawn worker processes in isolated git workspaces.
//! - Track worker state and provide graceful termination.
//!
//! Not handled here:
//! - State persistence (see `super::state`).
//! - CLI argument construction (see `super::args`).
//!
//! Invariants/assumptions:
//! - Workers run in isolated workspaces with dedicated branches.
//! - Task selection respects queue order and exclusion sets.
//! - Workers push directly to target branch (no PRs in direct-push mode).

use crate::agent::AgentOverrides;
use crate::commands::run::parallel::args::build_override_args;
use crate::commands::run::selection::select_run_one_task_index_excluding;
use crate::config;
use crate::git::WorkspaceSpec;
use crate::lock::DirLock;
use crate::queue;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command, Stdio};

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

/// Collect IDs that should be excluded from task selection.
/// In direct-push mode, we exclude:
/// - Workers currently in-flight (being tracked by the guard)
/// - Workers in the state file that are not in terminal state
pub(crate) fn collect_excluded_ids(
    state_file: &state::ParallelStateFile,
    in_flight: &HashMap<String, WorkerState>,
) -> HashSet<String> {
    let mut excluded = HashSet::new();

    // Exclude workers being tracked by the guard
    for key in in_flight.keys() {
        excluded.insert(key.trim().to_string());
    }

    // Exclude workers from state file that are not terminal
    // (running or integrating - still active)
    for worker in &state_file.workers {
        if !worker.is_terminal() {
            excluded.insert(worker.task_id.trim().to_string());
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
        if let Err(e) = worker.child.wait() {
            log::debug!("Failed to wait for worker {}: {}", worker.task_id, e);
        }
    }
}

/// Spawn a worker process for the given task in the specified workspace.
pub(crate) fn spawn_worker(
    resolved: &config::Resolved,
    workspace_path: &Path,
    task_id: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<Child> {
    let (mut cmd, args) =
        build_worker_command(resolved, workspace_path, task_id, overrides, force)?;
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
    resolved: &config::Resolved,
    workspace_path: &Path,
    task_id: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<(Command, Vec<String>)> {
    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut cmd = Command::new(exe);
    cmd.current_dir(workspace_path);
    cmd.env("PWD", workspace_path);
    cmd.stdin(Stdio::null());

    let mut args: Vec<String> = Vec::new();
    if force {
        args.push("--force".to_string());
    }
    args.push("run".to_string());
    args.push("one".to_string());
    args.push("--id".to_string());
    args.push(task_id.to_string());
    args.push("--parallel-worker".to_string());
    args.push("--non-interactive".to_string());
    args.push("--no-progress".to_string());

    // Pass coordinator's queue/done paths via CLI flags (not env vars)
    // This allows workers to read task context without env var leakage to child processes
    args.push("--coordinator-queue-path".to_string());
    args.push(resolved.queue_path.to_string_lossy().to_string());
    args.push("--coordinator-done-path".to_string());
    args.push(resolved.done_path.to_string_lossy().to_string());

    args.extend(build_override_args(overrides));

    Ok((cmd, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn build_worker_command_sets_cwd_and_args() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_path)?;

        let ralph_dir = temp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: temp.path().to_path_buf(),
            queue_path: ralph_dir.join("queue.json"),
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let overrides = AgentOverrides::default();
        let (cmd, args) =
            build_worker_command(&resolved, &workspace_path, "RQ-1234", &overrides, true)?;

        assert_eq!(cmd.get_current_dir(), Some(workspace_path.as_path()));

        let mut pwd_seen = false;
        for (key, value) in cmd.get_envs() {
            if key == std::ffi::OsStr::new("PWD") {
                pwd_seen = true;
                assert_eq!(value, Some(workspace_path.as_os_str()));
            }
        }
        assert!(pwd_seen, "PWD env should be set for workspace execution");

        assert!(args.contains(&"--force".to_string()));
        assert!(args.contains(&"--no-progress".to_string()));
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"one".to_string()));
        assert!(args.contains(&"--parallel-worker".to_string()));
        assert!(args.contains(&"--non-interactive".to_string()));
        // Default overrides should not emit git-commit-push flags
        assert!(!args.contains(&"--git-commit-push-on".to_string()));
        assert!(!args.contains(&"--git-commit-push-off".to_string()));

        let run_pos = args.iter().position(|arg| arg == "run").expect("run");
        let one_pos = args.iter().position(|arg| arg == "one").expect("one");
        let no_progress_pos = args
            .iter()
            .position(|arg| arg == "--no-progress")
            .expect("--no-progress");
        assert!(
            no_progress_pos > one_pos && one_pos > run_pos,
            "--no-progress must be scoped under `run one`, got args: {:?}",
            args
        );

        let id_pos = args.iter().position(|arg| arg == "--id").expect("--id");
        assert_eq!(args.get(id_pos + 1), Some(&"RQ-1234".to_string()));

        // Verify coordinator paths are passed via CLI flags
        let queue_path_pos = args
            .iter()
            .position(|arg| arg == "--coordinator-queue-path")
            .expect("--coordinator-queue-path should be in args");
        assert_eq!(
            args.get(queue_path_pos + 1),
            Some(&resolved.queue_path.to_string_lossy().to_string()),
            "coordinator queue path should follow --coordinator-queue-path flag"
        );

        let done_path_pos = args
            .iter()
            .position(|arg| arg == "--coordinator-done-path")
            .expect("--coordinator-done-path should be in args");
        assert_eq!(
            args.get(done_path_pos + 1),
            Some(&resolved.done_path.to_string_lossy().to_string()),
            "coordinator done path should follow --coordinator-done-path flag"
        );

        Ok(())
    }

    #[test]
    fn build_worker_command_emits_git_commit_push_on_when_overridden() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_path)?;

        let ralph_dir = temp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: temp.path().to_path_buf(),
            queue_path: ralph_dir.join("queue.json"),
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let overrides = AgentOverrides {
            git_commit_push_enabled: Some(true),
            ..Default::default()
        };
        let (_cmd, args) =
            build_worker_command(&resolved, &workspace_path, "RQ-1234", &overrides, false)?;

        assert!(args.contains(&"--git-commit-push-on".to_string()));
        assert!(!args.contains(&"--git-commit-push-off".to_string()));

        Ok(())
    }

    #[test]
    fn build_worker_command_emits_git_commit_push_off_when_overridden() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_path)?;

        let ralph_dir = temp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: temp.path().to_path_buf(),
            queue_path: ralph_dir.join("queue.json"),
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let overrides = AgentOverrides {
            git_commit_push_enabled: Some(false),
            ..Default::default()
        };
        let (_cmd, args) =
            build_worker_command(&resolved, &workspace_path, "RQ-1234", &overrides, false)?;

        assert!(args.contains(&"--git-commit-push-off".to_string()));
        assert!(!args.contains(&"--git-commit-push-on".to_string()));

        Ok(())
    }

    #[test]
    fn collect_excluded_ids_includes_active_workers() -> Result<()> {
        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        // Running worker (should be excluded)
        let running_worker = state::WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/tmp/workspace/RQ-0001"),
            "2026-02-20T00:00:00Z".to_string(),
        );
        state_file.upsert_worker(running_worker);

        // Integrating worker (should be excluded - not terminal)
        let mut integrating_worker = state::WorkerRecord::new(
            "RQ-0002",
            PathBuf::from("/tmp/workspace/RQ-0002"),
            "2026-02-20T00:00:00Z".to_string(),
        );
        integrating_worker.start_integration();
        state_file.upsert_worker(integrating_worker);

        // Completed worker (should NOT be excluded - terminal state)
        let mut completed_worker = state::WorkerRecord::new(
            "RQ-0003",
            PathBuf::from("/tmp/workspace/RQ-0003"),
            "2026-02-20T00:00:00Z".to_string(),
        );
        completed_worker.mark_completed("2026-02-20T01:00:00Z".to_string());
        state_file.upsert_worker(completed_worker);

        // Failed worker (should NOT be excluded - terminal state)
        let mut failed_worker = state::WorkerRecord::new(
            "RQ-0004",
            PathBuf::from("/tmp/workspace/RQ-0004"),
            "2026-02-20T00:00:00Z".to_string(),
        );
        failed_worker.mark_failed("2026-02-20T01:00:00Z".to_string(), "error");
        state_file.upsert_worker(failed_worker);

        let mut in_flight = HashMap::new();
        let child = std::process::Command::new("true").spawn()?;
        in_flight.insert(
            "RQ-0005".to_string(),
            WorkerState {
                task_id: "RQ-0005".to_string(),
                task_title: "title".to_string(),
                workspace: WorkspaceSpec {
                    path: PathBuf::from("/tmp/workspaces/RQ-0005"),
                    branch: "ralph/RQ-0005".to_string(),
                },
                child,
            },
        );

        let excluded = collect_excluded_ids(&state_file, &in_flight);

        // Active workers should be excluded
        assert!(
            excluded.contains("RQ-0001"),
            "running worker should be excluded"
        );
        assert!(
            excluded.contains("RQ-0002"),
            "integrating worker should be excluded"
        );
        assert!(
            excluded.contains("RQ-0005"),
            "in-flight worker should be excluded"
        );

        // Terminal workers should NOT be excluded
        assert!(
            !excluded.contains("RQ-0003"),
            "completed worker should NOT be excluded"
        );
        assert!(
            !excluded.contains("RQ-0004"),
            "failed worker should NOT be excluded"
        );

        for worker in in_flight.values_mut() {
            if let Err(e) = worker.child.wait() {
                log::debug!(
                    "Failed to wait for worker {} in test: {}",
                    worker.task_id,
                    e
                );
            }
        }

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
            description: None,
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
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            estimated_minutes: None,
            actual_minutes: None,
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
