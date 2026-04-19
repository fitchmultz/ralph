//! Sequential run-loop state machine.
//!
//! Responsibilities:
//! - Route between sequential and parallel execution.
//! - Drive per-iteration task execution, wait transitions, and abort handling.
//!
//! Not handled here:
//! - Session recovery policy details.
//! - Wait-loop file watching internals.
//!
//! Invariants/assumptions:
//! - Queue lock contention, dirty repos, and queue validation failures are terminal.
//! - Parallel execution handoff happens before sequential state is initialized.

use anyhow::Result;

use crate::config;
use crate::contracts::TaskStatus;
use crate::{queue, runutil};

use super::lifecycle::LoopLifecycle;
use super::session::resolve_resume_state;
use super::types::RunLoopOptions;
use super::wait::{WaitExit, WaitMode, wait_for_work};
use crate::commands::run::queue_lock::{
    clear_stale_queue_lock_for_resume, is_queue_lock_already_held_error, queue_lock_blocking_state,
};
use crate::commands::run::run_one::{RunOneResumeOptions, RunOutcome, run_one_with_handlers};
use crate::commands::run::{emit_blocked_state_changed, emit_blocked_state_cleared};

pub fn run_loop(resolved: &config::Resolved, opts: RunLoopOptions) -> Result<()> {
    if let Some(result) = maybe_run_parallel(resolved, &opts)? {
        return result;
    }

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let resume_state = resolve_resume_state(resolved, &opts)?;

    if resume_state.resume_task_id.is_some()
        && let Err(err) = clear_stale_queue_lock_for_resume(&resolved.repo_root)
    {
        log::warn!("Failed to clear stale queue lock for resume: {}", err);
    }

    let initial_todo_count = queue_file
        .tasks
        .iter()
        .filter(|task| {
            task.status == TaskStatus::Todo || (include_draft && task.status == TaskStatus::Draft)
        })
        .count() as u32;

    if initial_todo_count == 0 && resume_state.resume_task_id.is_none() {
        if include_draft {
            log::info!("No todo or draft tasks found.");
        } else {
            log::info!("No todo tasks found.");
        }
        if !opts.wait_when_empty {
            return Ok(());
        }
    }

    let label = format!(
        "RunLoop (todo={initial_todo_count}, max_tasks={})",
        opts.max_tasks
    );
    let mut lifecycle =
        LoopLifecycle::start(resolved, initial_todo_count, resume_state.completed_count);

    let result = crate::commands::run::logging::with_scope(&label, || {
        run_loop_state_machine(
            resolved,
            &opts,
            include_draft,
            resume_state.resume_task_id.as_deref(),
            &mut lifecycle,
        )
    });

    lifecycle.finish(resolved, &opts, &result);
    result
}

fn maybe_run_parallel(
    resolved: &config::Resolved,
    opts: &RunLoopOptions,
) -> Result<Option<Result<()>>> {
    let parallel_workers = opts.parallel_workers.or(resolved.config.parallel.workers);
    if let Some(workers) = parallel_workers
        && workers >= 2
    {
        if opts.auto_resume {
            log::warn!("Parallel run ignores --resume; starting a fresh parallel loop.");
        }
        if opts.starting_completed != 0 {
            log::warn!("Parallel run ignores starting_completed; counters will start at zero.");
        }
        return Ok(Some(crate::commands::run::parallel::run_loop_parallel(
            resolved,
            crate::commands::run::parallel::ParallelRunOptions {
                max_tasks: opts.max_tasks,
                workers,
                agent_overrides: opts.agent_overrides.clone(),
                force: opts.force,
            },
        )));
    }

    Ok(None)
}

fn run_loop_state_machine(
    resolved: &config::Resolved,
    opts: &RunLoopOptions,
    include_draft: bool,
    resume_task_id: Option<&str>,
    lifecycle: &mut LoopLifecycle,
) -> Result<()> {
    let mut pending_resume_task_id = resume_task_id.map(str::to_string);
    let mut active_blocking = None;

    loop {
        if lifecycle.max_tasks_reached(opts) {
            log::info!(
                "RunLoop: end (reached max task limit: {})",
                lifecycle.completed()
            );
            return Ok(());
        }

        if lifecycle.stop_requested() {
            log::info!("Stop signal detected; no new tasks will be started.");
            lifecycle.clear_stop_signal();
            return Ok(());
        }

        match run_one_with_handlers(
            resolved,
            &opts.agent_overrides,
            opts.force,
            RunOneResumeOptions::resolved(pending_resume_task_id.take()),
            None,
            opts.run_event_handler.clone(),
        ) {
            Ok(RunOutcome::NoCandidates) => {
                let idle_state = crate::contracts::BlockingState::idle(include_draft)
                    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback());
                active_blocking = Some(idle_state.clone());
                emit_blocked_state_changed(&idle_state, opts.run_event_handler.as_ref());

                if !opts.wait_when_empty {
                    log::info!("{}", idle_state.message);
                    return Ok(());
                }
                match wait_for_work(
                    resolved,
                    include_draft,
                    WaitMode::EmptyAllowed,
                    opts.wait_poll_ms,
                    opts.empty_poll_ms,
                    0,
                    opts.notify_when_unblocked,
                    lifecycle.webhook_context(),
                )? {
                    WaitExit::RunnableAvailable { .. } => {
                        log::info!("RunLoop: new runnable tasks detected; continuing");
                        if active_blocking.take().is_some() {
                            emit_blocked_state_cleared(opts.run_event_handler.as_ref());
                        }
                    }
                    WaitExit::QueueStillIdle { state } => {
                        log::info!("{}", state.message);
                        return Ok(());
                    }
                    WaitExit::TimedOut { state } => {
                        log::info!(
                            "RunLoop: end (wait timeout reached while {})",
                            state.message
                        );
                        return Ok(());
                    }
                    WaitExit::StopRequested { state } => {
                        if let Some(state) = state {
                            log::info!(
                                "RunLoop: end (stop signal received while {})",
                                state.message
                            );
                        } else {
                            log::info!("RunLoop: end (stop signal received)");
                        }
                        return Ok(());
                    }
                }
            }
            Ok(RunOutcome::Blocked { summary, state }) => {
                active_blocking = Some((*state).clone());

                if !(opts.wait_when_blocked || opts.wait_when_empty) {
                    log::info!(
                        "{} (ready={} deps={} sched={})",
                        state.message,
                        summary.runnable_candidates,
                        summary.blocked_by_dependencies,
                        summary.blocked_by_schedule
                    );
                    return Ok(());
                }

                let mode = if opts.wait_when_empty {
                    WaitMode::EmptyAllowed
                } else {
                    WaitMode::BlockedOnly
                };

                match wait_for_work(
                    resolved,
                    include_draft,
                    mode,
                    opts.wait_poll_ms,
                    opts.empty_poll_ms,
                    opts.wait_timeout_seconds,
                    opts.notify_when_unblocked,
                    lifecycle.webhook_context(),
                )? {
                    WaitExit::RunnableAvailable {
                        summary: new_summary,
                    } => {
                        log::info!(
                            "RunLoop: unblocked (ready={}, deps={}, sched={}); continuing",
                            new_summary.runnable_candidates,
                            new_summary.blocked_by_dependencies,
                            new_summary.blocked_by_schedule
                        );
                        if active_blocking.take().is_some() {
                            emit_blocked_state_cleared(opts.run_event_handler.as_ref());
                        }
                    }
                    WaitExit::QueueStillIdle { state } => {
                        log::info!("{}", state.message);
                        return Ok(());
                    }
                    WaitExit::TimedOut { state } => {
                        log::info!(
                            "RunLoop: end (wait timeout reached while {})",
                            state.message
                        );
                        return Ok(());
                    }
                    WaitExit::StopRequested { state } => {
                        if let Some(state) = state {
                            log::info!(
                                "RunLoop: end (stop signal received while {})",
                                state.message
                            );
                        } else {
                            log::info!("RunLoop: end (stop signal received)");
                        }
                        return Ok(());
                    }
                }
            }
            Ok(RunOutcome::Ran { .. }) => {
                if active_blocking.take().is_some() {
                    emit_blocked_state_cleared(opts.run_event_handler.as_ref());
                }
                lifecycle.record_success()
            }
            Err(err) => {
                if let Some(reason) = runutil::abort_reason(&err) {
                    match reason {
                        runutil::RunAbortReason::Interrupted => {
                            log::info!("RunLoop: aborting after interrupt");
                        }
                        runutil::RunAbortReason::UserRevert => {
                            log::info!("RunLoop: aborting after user-requested revert");
                        }
                    }
                    return Err(err);
                }

                if is_queue_lock_already_held_error(&err) {
                    if let Some(state) = queue_lock_blocking_state(&resolved.repo_root, &err) {
                        emit_blocked_state_changed(&state, opts.run_event_handler.as_ref());
                    }
                    log::error!("RunLoop: aborting due to queue lock contention");
                    return Err(err);
                }
                if runutil::is_dirty_repo_error(&err) {
                    log::error!("RunLoop: aborting due to dirty repository");
                    return Err(err);
                }
                if runutil::is_queue_validation_error(&err) {
                    log::error!("RunLoop: aborting due to queue validation error");
                    return Err(err);
                }
                if let Some(ci_failure) =
                    err.downcast_ref::<crate::commands::run::supervision::CiFailure>()
                {
                    emit_blocked_state_changed(
                        &ci_failure.blocking_state(),
                        opts.run_event_handler.as_ref(),
                    );
                }

                lifecycle.record_failure(&err)?;
            }
        }
    }
}
