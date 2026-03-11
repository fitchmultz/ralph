//! Dispatch logic for `ralph run`.
//!
//! Responsibilities:
//! - Resolve config/profile context for run commands.
//! - Route parsed clap commands into the runtime entrypoints.
//!
//! Not handled here:
//! - Clap type definitions.
//! - Help-text content.
//!
//! Invariants/assumptions:
//! - Profile resolution happens once before per-command dispatch.
//! - Parallel worker internal flags are validated before execution.

use anyhow::{Result, anyhow};

use crate::{agent, commands::run as run_cmd, config, debuglog};

use super::args::{ParallelSubcommand, RunCommand, RunLoopArgs, RunOneArgs};

pub fn handle_run(cmd: RunCommand, force: bool) -> Result<()> {
    let profile = selected_profile(&cmd);
    let resolved = config::resolve_from_cwd_with_profile(profile)?;

    match cmd {
        RunCommand::Resume(args) => {
            maybe_enable_debug(args.debug, &resolved)?;
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;
            run_cmd::run_loop(
                &resolved,
                run_cmd::RunLoopOptions {
                    max_tasks: 0,
                    agent_overrides: overrides,
                    force: args.force || force,
                    auto_resume: true,
                    starting_completed: 0,
                    non_interactive: args.non_interactive,
                    parallel_workers: None,
                    wait_when_blocked: false,
                    wait_poll_ms: 1000,
                    wait_timeout_seconds: 0,
                    notify_when_unblocked: false,
                    wait_when_empty: false,
                    empty_poll_ms: 30_000,
                },
            )
        }
        RunCommand::One(args) => handle_run_one(args, force, &resolved),
        RunCommand::Loop(args) => handle_run_loop(args, force, &resolved),
        RunCommand::Parallel(args) => match args.command {
            ParallelSubcommand::Status(status_args) => {
                run_cmd::parallel_status(&resolved, status_args.json)
            }
            ParallelSubcommand::Retry(retry_args) => {
                run_cmd::parallel_retry(&resolved, &retry_args.task, force)
            }
        },
    }
}

fn selected_profile(cmd: &RunCommand) -> Option<&str> {
    match cmd {
        RunCommand::Resume(args) => args.agent.profile.as_deref(),
        RunCommand::One(args) => args.agent.profile.as_deref(),
        RunCommand::Loop(args) => args.agent.profile.as_deref(),
        RunCommand::Parallel(_) => None,
    }
}

fn maybe_enable_debug(debug: bool, resolved: &config::Resolved) -> Result<()> {
    if debug {
        debuglog::enable(&resolved.repo_root)?;
    }
    Ok(())
}

fn handle_run_one(args: RunOneArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    maybe_enable_debug(args.debug, resolved)?;
    let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

    if args.dry_run {
        if args.parallel_worker {
            return Err(anyhow!("--dry-run cannot be used with --parallel-worker"));
        }
        return run_cmd::dry_run_one(resolved, &overrides, args.id.as_deref());
    }

    if args.parallel_worker {
        return handle_parallel_worker_run_one(args, force, resolved, overrides);
    }

    if let Some(task_id) = args.id.as_deref() {
        run_cmd::run_one_with_id(resolved, &overrides, force, task_id, None, None, None)?;
    } else {
        run_cmd::run_one(resolved, &overrides, force, None)?;
    }

    Ok(())
}

fn handle_parallel_worker_run_one(
    args: RunOneArgs,
    force: bool,
    resolved: &config::Resolved,
    overrides: crate::agent::AgentOverrides,
) -> Result<()> {
    let task_id = args
        .id
        .as_deref()
        .ok_or_else(|| anyhow!("--parallel-worker requires --id <TASK_ID>"))?;
    let target_branch = args
        .parallel_target_branch
        .as_deref()
        .ok_or_else(|| anyhow!("--parallel-worker requires --parallel-target-branch"))?;

    let mut worker_resolved = resolved.clone();
    worker_resolved.queue_path = args
        .coordinator_queue_path
        .clone()
        .ok_or_else(|| anyhow!("--parallel-worker requires --coordinator-queue-path"))?;
    worker_resolved.done_path = args
        .coordinator_done_path
        .clone()
        .ok_or_else(|| anyhow!("--parallel-worker requires --coordinator-done-path"))?;

    log::debug!(
        "parallel worker using queue/done paths: queue={}, done={}, target_branch={}",
        worker_resolved.queue_path.display(),
        worker_resolved.done_path.display(),
        target_branch
    );

    run_cmd::run_one_parallel_worker(&worker_resolved, &overrides, force, task_id, target_branch)?;
    Ok(())
}

fn handle_run_loop(args: RunLoopArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    maybe_enable_debug(args.debug, resolved)?;
    let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

    if args.dry_run {
        return run_cmd::dry_run_loop(resolved, &overrides);
    }

    run_cmd::run_loop(
        resolved,
        run_cmd::RunLoopOptions {
            max_tasks: args.max_tasks,
            agent_overrides: overrides,
            force,
            auto_resume: args.resume,
            starting_completed: 0,
            non_interactive: args.non_interactive,
            parallel_workers: args.parallel,
            wait_when_blocked: args.wait_when_blocked,
            wait_poll_ms: args.wait_poll_ms,
            wait_timeout_seconds: args.wait_timeout_seconds,
            notify_when_unblocked: args.notify_when_unblocked,
            wait_when_empty: args.wait_when_empty,
            empty_poll_ms: args.empty_poll_ms,
        },
    )
}
