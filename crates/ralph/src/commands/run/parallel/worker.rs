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
use crate::git::WorkspaceSpec;
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

/// Select the next runnable task from the queue, excluding certain IDs.
pub(crate) fn select_next_task(
    resolved: &config::Resolved,
    include_draft: bool,
    excluded_ids: &HashSet<String>,
    force: bool,
) -> Result<Option<(String, String)>> {
    let _lock = queue::acquire_queue_lock(&resolved.repo_root, "parallel selection", force)?;
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };

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

/// Collect IDs that should be excluded from task selection (in-flight, inflight in state, unmerged PRs).
pub(crate) fn collect_excluded_ids(
    state_file: &state::ParallelStateFile,
    in_flight: &HashMap<String, WorkerState>,
) -> HashSet<String> {
    let mut excluded = HashSet::new();
    for key in in_flight.keys() {
        excluded.insert(key.trim().to_string());
    }
    for record in &state_file.tasks_in_flight {
        excluded.insert(record.task_id.trim().to_string());
    }
    for record in &state_file.prs {
        if !record.merged {
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
    args.push("--git-commit-push-on".to_string());

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
        for (key, value) in cmd.get_envs() {
            if key == std::ffi::OsStr::new("PWD") {
                pwd_seen = true;
                assert_eq!(value, Some(workspace_path.as_os_str()));
            }
            if key == std::ffi::OsStr::new(crate::config::REPO_ROOT_OVERRIDE_ENV) {
                override_seen = true;
                assert_eq!(value, Some(workspace_path.as_os_str()));
            }
        }
        assert!(pwd_seen, "PWD env should be set for workspace execution");
        assert!(
            override_seen,
            "{} env should be set for workspace execution",
            crate::config::REPO_ROOT_OVERRIDE_ENV
        );

        assert!(args.contains(&"--force".to_string()));
        assert!(args.contains(&"--no-progress".to_string()));
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"one".to_string()));
        assert!(args.contains(&"--parallel-worker".to_string()));
        assert!(args.contains(&"--non-interactive".to_string()));
        assert!(args.contains(&"--git-commit-push-on".to_string()));

        let id_pos = args.iter().position(|arg| arg == "--id").expect("--id");
        assert_eq!(args.get(id_pos + 1), Some(&"RQ-1234".to_string()));

        Ok(())
    }

    #[test]
    fn collect_excluded_ids_includes_state_and_in_flight() -> Result<()> {
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
        });
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 7,
            pr_url: "https://example.com/pr/7".to_string(),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
        });

        let mut in_flight = HashMap::new();
        let child = std::process::Command::new("true").spawn()?;
        in_flight.insert(
            "RQ-0004".to_string(),
            WorkerState {
                task_id: "RQ-0004".to_string(),
                task_title: "title".to_string(),
                workspace: WorkspaceSpec {
                    task_id: "RQ-0004".to_string(),
                    path: PathBuf::from("/tmp/workspaces/RQ-0004"),
                    branch: "ralph/RQ-0004".to_string(),
                },
                child,
            },
        );

        let excluded = collect_excluded_ids(&state_file, &in_flight);
        assert!(excluded.contains("RQ-0002"));
        assert!(excluded.contains("RQ-0003"));
        assert!(excluded.contains("RQ-0004"));

        for worker in in_flight.values_mut() {
            let _ = worker.child.wait();
        }

        Ok(())
    }
}
