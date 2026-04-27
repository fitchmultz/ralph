//! Parallel run-loop control state machine.
//!
//! Purpose:
//! - Parallel run-loop control state machine.
//!
//! Responsibilities:
//! - Drive worker spawn, worker-finish handling, idle-exit checks, and bounded control waiting.
//! - Keep the active-loop decisions separate from bootstrap and shutdown concerns.
//!
//! Not handled here:
//! - Preflight validation.
//! - Final notification/webhook logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Worker child completion is event-driven through `ParallelCleanupGuard`.
//! - A short control wait slice is retained only to observe Ctrl-C and stop-signal files while blocked on worker exits.

use anyhow::Result;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::config;
use crate::queue;

use super::preflight::{PreparedParallelRun, prepare_parallel_run};
use super::shutdown::finalize_parallel_run;
use crate::commands::run::RunLoopOutcome;
use crate::commands::run::parallel::orchestration::events::{
    FinishedWorkerHandlingContext, handle_finished_workers,
};
use crate::commands::run::parallel::state::{self, WorkerRecord};
use crate::commands::run::parallel::sync::sync_ralph_state;
use crate::commands::run::parallel::worker::{
    NextTaskSelection, collect_excluded_ids, select_next_task_locked,
    select_next_task_state_locked, spawn_worker, start_worker_monitor,
};
use crate::commands::run::parallel::{
    ParallelRunOptions, can_start_more_tasks, effective_active_worker_count, prune_stale_workers,
    spawn_worker_with_registered_workspace,
};

const WORKER_CONTROL_WAIT_SLICE: Duration = Duration::from_millis(250);

pub(crate) fn run_loop_parallel(
    resolved: &config::Resolved,
    opts: ParallelRunOptions,
) -> Result<RunLoopOutcome> {
    let queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "run loop", opts.force)?;
    let ctrlc_state = crate::runner::ctrlc_state().ok().cloned();
    let mut prepared = prepare_parallel_run(resolved, &opts)?;

    let loop_result = drive_parallel_loop(
        resolved,
        &opts,
        &queue_lock,
        ctrlc_state.as_deref(),
        &mut prepared,
    );
    finalize_parallel_run(resolved, &opts, &mut prepared, loop_result)
}

fn drive_parallel_loop(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
    queue_lock: &crate::lock::DirLock,
    ctrlc: Option<&crate::runner::CtrlCState>,
    prepared: &mut PreparedParallelRun,
) -> Result<RunLoopOutcome> {
    loop {
        if ctrlc.is_some_and(|ctrlc| ctrlc.interrupted.load(Ordering::SeqCst)) {
            prepared.interrupted = true;
            log::info!("Ctrl+C detected; stopping parallel run and cleaning up.");
            break;
        }

        if !prepared.stop_requested && crate::signal::stop_signal_exists(&prepared.cache_dir) {
            prepared.stop_requested = true;
            log::info!("Stop signal detected; no new tasks will be started.");
        }

        let pruned_workers = prune_stale_workers(prepared.guard.state_file_mut());
        if !pruned_workers.is_empty() {
            log::warn!("Pruned stale workers: {}", pruned_workers.join(", "));
            state::save_state(&prepared.state_path, prepared.guard.state_file())?;
        }

        spawn_available_workers(resolved, opts, queue_lock, prepared)?;

        drain_and_handle_finished(resolved, queue_lock, prepared)?;

        if prepared.guard.in_flight().is_empty()
            && let Some(outcome) = terminal_idle_outcome(resolved, opts, queue_lock, prepared)?
        {
            return Ok(outcome);
        }

        if !prepared.guard.in_flight().is_empty() {
            let finished = prepared
                .guard
                .wait_for_finished_workers(WORKER_CONTROL_WAIT_SLICE);
            if !finished.is_empty() {
                handle_finished_workers(
                    finished,
                    &mut prepared.guard,
                    FinishedWorkerHandlingContext {
                        state_path: &prepared.state_path,
                        workspace_root: &prepared.settings.workspace_root,
                        resolved,
                        target_branch: &prepared.target_branch,
                        queue_lock,
                    },
                    &mut prepared.stats,
                )?;
            }
        }
    }

    Ok(RunLoopOutcome::Completed)
}

fn spawn_available_workers(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
    queue_lock: &crate::lock::DirLock,
    prepared: &mut PreparedParallelRun,
) -> Result<()> {
    while effective_active_worker_count(
        prepared.guard.state_file(),
        prepared.guard.in_flight().len(),
    ) < prepared.settings.workers as usize
        && can_start_more_tasks(prepared.tasks_started, opts.max_tasks)
        && !prepared.stop_requested
    {
        let excluded = collect_excluded_ids(
            prepared.guard.state_file(),
            prepared.guard.in_flight(),
            &prepared.attempted_task_ids,
        );
        let (task_id, task_title) =
            match select_next_task_locked(resolved, prepared.include_draft, &excluded, queue_lock)?
            {
                Some(task) => task,
                None => break,
            };

        let target_branch = prepared.target_branch.clone();
        let worker_overrides = prepared.worker_overrides.clone();
        let workspace_root = prepared.settings.workspace_root.clone();
        let (workspace, child) = spawn_worker_with_registered_workspace(
            &mut prepared.guard,
            &task_id,
            || {
                let workspace = crate::git::create_workspace_at(
                    &resolved.repo_root,
                    &workspace_root,
                    &task_id,
                    &target_branch,
                )?;
                Ok(workspace)
            },
            |path| sync_ralph_state(resolved, path),
            |workspace| {
                spawn_worker(
                    resolved,
                    &workspace.path,
                    &task_id,
                    &target_branch,
                    &worker_overrides,
                    opts.force,
                )
            },
        )?;

        let task_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
        let record = WorkerRecord::new(&task_id, workspace.path.clone(), task_started_at);
        prepared.guard.state_file_mut().upsert_worker(record);
        state::save_state(&prepared.state_path, prepared.guard.state_file())?;

        let worker = start_worker_monitor(
            &task_id,
            task_title,
            workspace.clone(),
            child,
            prepared.guard.worker_event_sender(),
        );
        prepared.guard.register_worker(task_id.clone(), worker);
        prepared.attempted_task_ids.insert(task_id);
        prepared.tasks_started += 1;
    }

    Ok(())
}

fn drain_and_handle_finished(
    resolved: &config::Resolved,
    queue_lock: &crate::lock::DirLock,
    prepared: &mut PreparedParallelRun,
) -> Result<()> {
    let finished = prepared.guard.drain_finished_workers();
    handle_finished_workers(
        finished,
        &mut prepared.guard,
        FinishedWorkerHandlingContext {
            state_path: &prepared.state_path,
            workspace_root: &prepared.settings.workspace_root,
            resolved,
            target_branch: &prepared.target_branch,
            queue_lock,
        },
        &mut prepared.stats,
    )
}

fn terminal_idle_outcome(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
    queue_lock: &crate::lock::DirLock,
    prepared: &PreparedParallelRun,
) -> Result<Option<RunLoopOutcome>> {
    if opts.max_tasks != 0 && prepared.tasks_started >= opts.max_tasks {
        return Ok(Some(RunLoopOutcome::Completed));
    }

    if prepared.stop_requested {
        return Ok(Some(RunLoopOutcome::Stopped { blocking: None }));
    }

    let excluded = collect_excluded_ids(
        prepared.guard.state_file(),
        prepared.guard.in_flight(),
        &prepared.attempted_task_ids,
    );
    let outcome = match select_next_task_state_locked(
        resolved,
        prepared.include_draft,
        &excluded,
        queue_lock,
    )? {
        NextTaskSelection::Runnable(_) => None,
        NextTaskSelection::NoCandidates { blocking } => {
            Some(RunLoopOutcome::NoCandidates { blocking })
        }
        NextTaskSelection::Blocked { summary, blocking } => {
            Some(RunLoopOutcome::Blocked { summary, blocking })
        }
    };
    Ok(outcome)
}
