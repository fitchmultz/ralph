//! Worker subprocess command construction.
//!
//! Purpose:
//! - Worker subprocess command construction.
//!
//! Responsibilities:
//! - Build the CLI argv/environment for parallel worker subprocesses.
//! - Map coordinator queue/done paths into the worker workspace.
//!
//! Non-scope:
//! - Task selection or worker lifecycle management.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::agent::AgentOverrides;
use crate::commands::run::parallel::args::build_override_args;
use crate::commands::run::parallel::path_map::map_resolved_path_into_workspace;
use crate::config;
use crate::runutil::isolate_child_process_group;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) fn build_worker_command(
    resolved: &config::Resolved,
    workspace_path: &Path,
    task_id: &str,
    target_branch: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<Command> {
    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut cmd = Command::new(exe);
    isolate_child_process_group(&mut cmd);

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

    let worker_queue_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        workspace_path,
        &resolved.queue_path,
        "queue",
    )
    .context("map queue path into worker workspace")?;
    let worker_done_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        workspace_path,
        &resolved.done_path,
        "done",
    )
    .context("map done path into worker workspace")?;
    args.push("--coordinator-queue-path".to_string());
    args.push(worker_queue_path.to_string_lossy().to_string());
    args.push("--coordinator-done-path".to_string());
    args.push(worker_done_path.to_string_lossy().to_string());
    args.push("--parallel-target-branch".to_string());
    args.push(target_branch.to_string());

    args.extend(build_override_args(overrides));
    cmd.args(&args);

    Ok(cmd)
}

pub(crate) fn debug_command_args(cmd: &Command) -> Vec<String> {
    cmd.get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}
