//! Run command orchestration and supervision.
//!
//! Responsibilities:
//! - Select runnable tasks and orchestrate run loop/one workflows.
//! - Coordinate queue bookkeeping, pre-run updates, and post-run supervision.
//! - Delegate phase-specific execution to `crate::commands::run::phases`.
//!
//! Not handled here:
//! - CLI argument parsing or config persistence.
//! - Runner process implementation details.
//! - Prompt template rendering outside run phases.
//!
//! Invariants/assumptions:
//! - Queue ordering is authoritative for task selection.
//! - Pre-run updates and CI gates honor config defaults unless overridden.
//! - Phase runners expect stream-json output for execution.
//!
//! Size: ~1100 LOC (reduced from 1315 after extracting session, iteration, context).
//! Remaining bloat: `run_one_impl` (~550 LOC) contains tightly-coupled phase execution
//! logic (Phase 1/2/3 orchestration) that resists further splitting without fragmenting
//! the execution flow. Submodules handle: phases/, parallel/, supervision/, selection/,
//! logging/, plus new session/, iteration/, and context/ modules.

use crate::commands::task as task_cmd;
use crate::config;
use crate::constants::limits::MAX_CONSECUTIVE_FAILURES;
use crate::contracts::{
    GitRevertMode, ParallelMergeWhen, ProjectType, RunnerCliOptionsPatch, TaskStatus,
};
use crate::promptflow;
use crate::queue::RunnableSelectionOptions;
use crate::session::{self, SessionValidationResult};
use crate::signal;

use crate::{git, prompts, queue, runner, runutil};
use anyhow::{Context, Result, bail};
use std::cell::RefCell;
use std::path::Path;

const QUEUE_LOCK_ALREADY_HELD_PREFIX: &str = "Queue lock already held at:";

/// Clear stale queue lock when resuming a session.
///
/// This helper is called during resume to preemptively clean up stale locks
/// left behind by a crashed or killed ralph process. It uses `force=true`
/// which only clears the lock if the owning PID is confirmed dead (see lock.rs).
///
/// Returns Ok(()) if no lock exists, lock was cleared, or lock is held by
/// a live process/unreadable metadata (those cases are handled later during
/// normal acquisition with a single actionable error).
fn clear_stale_queue_lock_for_resume(repo_root: &Path) -> Result<()> {
    let lock_dir = crate::lock::queue_lock_dir(repo_root);
    if !lock_dir.exists() {
        return Ok(());
    }

    // `force=true` only clears when the PID is confirmed stale (see lock.rs).
    // We acquire+drop immediately: this performs stale cleanup without holding the lock.
    let lock = match crate::queue::acquire_queue_lock(repo_root, "run loop resume", true) {
        Ok(lock) => lock,
        Err(err) => {
            // If the lock is held by a live process, or the owner metadata is missing/unreadable,
            // we cannot safely clear it here. Let normal acquisition report the actionable error.
            if is_queue_lock_already_held_error(&err) {
                return Ok(());
            }
            return Err(err);
        }
    };
    drop(lock);
    Ok(())
}

/// Check if an error is a "Queue lock already held" error.
///
/// This is used to detect lock contention errors that should not be retried
/// in the run loop, preventing the 50-failure abort loop.
fn is_queue_lock_already_held_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .to_string()
            .starts_with(QUEUE_LOCK_ALREADY_HELD_PREFIX)
    })
}

mod context;
mod execution_history_cli;
mod execution_timings;
mod iteration;
mod logging;
pub mod parallel;
mod phases;
mod run_session;
mod selection;
mod supervision;

pub(crate) use context::{mark_task_doing, task_context_for_prompt};
pub(crate) use execution_timings::RunExecutionTimings;
pub(crate) use iteration::{apply_followup_reasoning_effort, resolve_iteration_settings};
pub(crate) use run_session::{create_session_for_task, validate_resumed_task};
pub(crate) use selection::select_run_one_task_index;
pub(crate) use supervision::{PushPolicy, post_run_supervise, post_run_supervise_parallel_worker};

// Preserve existing `commands::run` unit tests which call phase 3 helpers directly.
#[allow(unused_imports)]
pub(crate) use phases::{apply_phase3_completion_signal, finalize_phase3_if_done};

// Re-export PhaseType for use by runner module.
pub(crate) use phases::PhaseType;

pub use crate::agent::AgentOverrides;

// Re-export parallel state types for TUI overlay
pub use parallel::state::{
    ParallelFinishedWithoutPrRecord, ParallelNoPrReason, ParallelPrLifecycle, ParallelPrRecord,
    ParallelStateFile, load_state, state_file_path,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueueLockMode {
    Acquire,
    Held,
    /// Acquire the queue lock but allow creating upstream branches (used by parallel workers).
    /// This combines the safety of lock acquisition with the push policy of Skip mode.
    AcquireAllowUpstream,
}

pub enum RunOutcome {
    NoTodo,
    Ran { task_id: String },
}

pub struct RunLoopOptions {
    /// 0 means "no limit"
    pub max_tasks: u32,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
    /// Auto-resume without prompting (for --resume flag)
    pub auto_resume: bool,
    /// Starting completed count (for resumed sessions)
    pub starting_completed: u32,
    /// Skip interactive prompts (for CI/non-interactive runs)
    pub non_interactive: bool,
    /// Number of parallel workers to use when parallel mode is enabled.
    pub parallel_workers: Option<u8>,
}

pub fn run_loop(resolved: &config::Resolved, opts: RunLoopOptions) -> Result<()> {
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
        let merge_when = resolved
            .config
            .parallel
            .merge_when
            .unwrap_or(ParallelMergeWhen::AsCreated);
        return parallel::run_loop_parallel(
            resolved,
            parallel::ParallelRunOptions {
                max_tasks: opts.max_tasks,
                workers,
                agent_overrides: opts.agent_overrides,
                force: opts.force,
                merge_when,
            },
        );
    }

    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let queue_file = queue::load_queue(&resolved.queue_path)?;

    // Handle session recovery (use configured timeout, defaulting to 24 hours)
    let session_timeout_hours = resolved.config.agent.session_timeout_hours;
    let (resume_task_id, completed_count) =
        match session::check_session(&cache_dir, &queue_file, session_timeout_hours)? {
            SessionValidationResult::NoSession => (None, opts.starting_completed),
            SessionValidationResult::Valid(session) => {
                if opts.auto_resume {
                    log::info!("Auto-resuming session for task {}", session.task_id);
                    (Some(session.task_id), session.tasks_completed_in_loop)
                } else {
                    match session::prompt_session_recovery(&session, opts.non_interactive)? {
                        true => (Some(session.task_id), session.tasks_completed_in_loop),
                        false => {
                            session::clear_session(&cache_dir)?;
                            (None, opts.starting_completed)
                        }
                    }
                }
            }
            SessionValidationResult::Stale { reason } => {
                log::info!("Stale session cleared: {}", reason);
                session::clear_session(&cache_dir)?;
                (None, opts.starting_completed)
            }
            SessionValidationResult::Timeout { hours, session } => {
                let threshold = session_timeout_hours
                    .unwrap_or(crate::constants::timeouts::DEFAULT_SESSION_TIMEOUT_HOURS);
                match session::prompt_session_recovery_timeout(
                    &session,
                    hours,
                    threshold,
                    opts.non_interactive,
                )? {
                    true => (Some(session.task_id), session.tasks_completed_in_loop),
                    false => {
                        session::clear_session(&cache_dir)?;
                        (None, opts.starting_completed)
                    }
                }
            }
        };

    // Preemptively clear stale queue lock when resuming a session.
    // This handles the case where a previous ralph process crashed/killed
    // and left behind a stale lock file.
    if resume_task_id.is_some()
        && let Err(err) = clear_stale_queue_lock_for_resume(&resolved.repo_root)
    {
        log::warn!("Failed to clear stale queue lock for resume: {}", err);
        // Continue anyway - the lock acquisition in run_one will fail
        // with a more specific error if the lock is still held.
    }

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let initial_todo_count = queue_file
        .tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
        })
        .count() as u32;

    if initial_todo_count == 0 && resume_task_id.is_none() {
        // Keep this phrase stable; some tests look for it.
        if include_draft {
            log::info!("No todo or draft tasks found.");
        } else {
            log::info!("No todo tasks found.");
        }
        return Ok(());
    }

    let label = format!(
        "RunLoop (todo={initial_todo_count}, max_tasks={})",
        opts.max_tasks
    );

    // Track loop completion stats for notification
    let mut tasks_attempted: usize = 0;
    let mut tasks_succeeded: usize = 0;
    let mut tasks_failed: usize = 0;

    // Track consecutive failures to prevent infinite loops
    let mut consecutive_failures: u32 = 0;

    // Use a mutable reference to allow modification inside the closure
    let mut completed = completed_count;

    // Clear any stale stop signal from previous runs to ensure clean state
    signal::clear_stop_signal_at_loop_start(&cache_dir);

    // Emit loop_started webhook before entering the run loop
    let loop_start_time = std::time::Instant::now();
    let loop_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let loop_webhook_ctx = crate::webhook::WebhookContext {
        repo_root: Some(resolved.repo_root.display().to_string()),
        branch: crate::git::current_branch(&resolved.repo_root).ok(),
        commit: crate::session::get_git_head_commit(&resolved.repo_root),
        ..Default::default()
    };
    crate::webhook::notify_loop_started(
        &resolved.config.agent.webhook,
        &loop_started_at,
        loop_webhook_ctx.clone(),
    );

    let result = logging::with_scope(&label, || {
        loop {
            if opts.max_tasks != 0 && completed >= opts.max_tasks {
                log::info!("RunLoop: end (reached max task limit: {completed})");
                return Ok(());
            }

            // Check for graceful stop signal before starting next task
            if signal::stop_signal_exists(&cache_dir) {
                log::info!("Stop signal detected; no new tasks will be started.");
                if let Err(e) = signal::clear_stop_signal(&cache_dir) {
                    log::warn!("Failed to clear stop signal: {}", e);
                }
                return Ok(());
            }

            match run_one(
                resolved,
                &opts.agent_overrides,
                opts.force,
                resume_task_id.as_deref(),
            ) {
                Ok(RunOutcome::NoTodo) => {
                    log::info!("RunLoop: end (no more todo tasks remaining)");
                    return Ok(());
                }
                Ok(RunOutcome::Ran { task_id: _ }) => {
                    completed += 1;
                    tasks_attempted += 1;
                    tasks_succeeded += 1;
                    consecutive_failures = 0; // Reset on success
                    log::info!("RunLoop: task-complete ({completed}/{initial_todo_count})");
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

                    // Queue lock errors are non-retriable - return immediately
                    // to prevent the 50-failure abort loop on deterministic lock errors.
                    if is_queue_lock_already_held_error(&err) {
                        log::error!("RunLoop: aborting due to queue lock contention");
                        return Err(err);
                    }

                    completed += 1;
                    tasks_attempted += 1;
                    tasks_failed += 1;
                    consecutive_failures += 1;
                    log::error!("RunLoop: task failed: {:#}", err);

                    // Safety check: prevent infinite loops from rapid consecutive failures
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        log::error!(
                            "RunLoop: aborting after {MAX_CONSECUTIVE_FAILURES} consecutive failures"
                        );
                        return Err(anyhow::anyhow!(
                            "Run loop aborted after {} consecutive task failures. \
                             This usually indicates a systemic issue (e.g., repo dirty, \
                             runner misconfiguration, or interrupt flag stuck). \
                             Check logs above for root cause.",
                            MAX_CONSECUTIVE_FAILURES
                        ));
                    }
                    // Continue with next task even if one failed
                }
            }
        }
    });

    // Send loop completion notification
    if tasks_attempted > 0 {
        let notify_on_complete = opts
            .agent_overrides
            .notify_on_complete
            .or(resolved.config.agent.notification.notify_on_complete)
            .unwrap_or(true);
        let notify_on_fail = opts
            .agent_overrides
            .notify_on_fail
            .or(resolved.config.agent.notification.notify_on_fail)
            .unwrap_or(true);
        let notify_on_loop_complete = resolved
            .config
            .agent
            .notification
            .notify_on_loop_complete
            .unwrap_or(true);
        // enabled acts as a global on/off switch - true if ANY notification type is enabled
        let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

        let notify_config = crate::notification::NotificationConfig {
            enabled,
            notify_on_complete,
            notify_on_fail,
            notify_on_loop_complete,
            suppress_when_active: resolved
                .config
                .agent
                .notification
                .suppress_when_active
                .unwrap_or(true),
            sound_enabled: opts
                .agent_overrides
                .notify_sound
                .or(resolved.config.agent.notification.sound_enabled)
                .unwrap_or(false),
            sound_path: resolved.config.agent.notification.sound_path.clone(),
            timeout_ms: resolved
                .config
                .agent
                .notification
                .timeout_ms
                .unwrap_or(8000),
        };
        crate::notification::notify_loop_complete(
            tasks_attempted,
            tasks_succeeded,
            tasks_failed,
            &notify_config,
        );
    }

    // Emit loop_stopped webhook after loop completes
    let loop_stopped_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let loop_duration_ms = loop_start_time.elapsed().as_millis() as u64;
    let loop_note = match &result {
        Ok(()) => Some(format!(
            "Completed: {}/{} succeeded",
            tasks_succeeded, tasks_attempted
        )),
        Err(e) => Some(format!("Error: {}", e)),
    };
    crate::webhook::notify_loop_stopped(
        &resolved.config.agent.webhook,
        &loop_stopped_at,
        crate::webhook::WebhookContext {
            duration_ms: Some(loop_duration_ms),
            ..loop_webhook_ctx
        },
        loop_note.as_deref(),
    );

    // Clear session on successful completion
    if result.is_ok()
        && let Err(e) = session::clear_session(&cache_dir)
    {
        log::warn!("Failed to clear session on loop completion: {}", e);
    }

    result
}

pub fn run_one_with_id(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    // Re-use run_one logic but target specific ID.
    // However, run_one finds the task based on status logic (Todo vs Doing).
    // run_one_with_id implies we selected a specific task.
    // We should probably adapt run_one_logic to take an optional task_id.
    // For now, let's just delegate to a shared implementation.
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        Some(task_id),
        None,
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

/// Run a specific task as a parallel worker (acquires queue lock, allows upstream creation).
pub fn run_one_parallel_worker(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
) -> Result<()> {
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::AcquireAllowUpstream,
        Some(task_id),
        None,
        None,
        None,
    )
    .map(|_| ())
}

/// Run a specific task when the queue lock is already held by the caller.
pub fn run_one_with_id_locked(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Held,
        Some(task_id),
        None,
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    resume_task_id: Option<&str>,
) -> Result<RunOutcome> {
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        None,
        resume_task_id,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_one_impl(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
    target_task_id: Option<&str>,
    resume_task_id: Option<&str>,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<RunOutcome> {
    // Handle Ctrl+C state initialization and pre-run interrupt detection.
    // If the handler setup fails, surface it as an error so Ctrl+C issues are visible.
    let ctrlc = crate::runner::ctrlc_state()
        .map_err(|e| anyhow::anyhow!("Ctrl-C handler initialization failed: {}", e))?;

    // Check for pre-run interrupt BEFORE resetting the flag.
    // If an interrupt was already pending (e.g., from a previous run or user pressed Ctrl+C
    // before we got here), we should abort without clearing the flag.
    if ctrlc.interrupted.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(runutil::RunAbort::new(
            runutil::RunAbortReason::Interrupted,
            "Ctrl+C was pressed before task execution started",
        )
        .into());
    }

    // Now safe to reset the flag for this run.
    // This prevents stale interrupts from previous runs affecting this one.
    ctrlc
        .interrupted
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let _queue_lock = match lock_mode {
        QueueLockMode::Acquire | QueueLockMode::AcquireAllowUpstream => Some(
            queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?,
        ),
        QueueLockMode::Held => None,
    };
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    // Determine execution shape and policy
    let phases: u8 = agent_overrides
        .phases
        .or(resolved.config.agent.phases)
        .unwrap_or(2);
    if !(1..=3).contains(&phases) {
        bail!("Invalid phases value: {} (expected 1, 2, or 3)", phases);
    }

    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(agent_overrides, resolved);

    let git_revert_mode = agent_overrides
        .git_revert_mode
        .or(resolved.config.agent.git_revert_mode)
        .unwrap_or(GitRevertMode::Ask);

    let git_commit_push_enabled = agent_overrides
        .git_commit_push_enabled
        .or(resolved.config.agent.git_commit_push_enabled)
        .unwrap_or(true);

    let push_policy = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PushPolicy::AllowCreateUpstream,
        QueueLockMode::Acquire | QueueLockMode::Held => PushPolicy::RequireUpstream,
    };

    let post_run_mode = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => phases::PostRunMode::ParallelWorker,
        QueueLockMode::Acquire | QueueLockMode::Held => phases::PostRunMode::Normal,
    };

    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: repoprompt_flags.plan_required,
        repoprompt_tool_injection: repoprompt_flags.tool_injection,
    };

    // --- Task Selection ---
    // Prefer resuming a `doing` task (crash recovery), otherwise take first runnable `todo`.
    let include_draft = agent_overrides.include_draft.unwrap_or(false);

    // Determine effective target: explicit target > resume_task_id > normal selection
    let effective_target = if target_task_id.is_some() {
        target_task_id
    } else if let Some(resume_id) = resume_task_id {
        // Validate the resumed task before using it
        match validate_resumed_task(&queue_file, resume_id, &resolved.repo_root) {
            Ok(()) => Some(resume_id),
            Err(e) => {
                log::info!("Session resume failed: {e}");
                None
            }
        }
    } else {
        None
    };

    let task_idx = match select_run_one_task_index(
        &queue_file,
        done_ref,
        effective_target,
        include_draft,
    )? {
        Some(idx) => idx,
        None => {
            // Count candidates (same logic as has_runnable, but more detailed)
            let candidates: Vec<_> = queue_file
                .tasks
                .iter()
                .filter(|t| {
                    t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
                })
                .collect();

            if candidates.is_empty() {
                // No candidates at all
                if include_draft {
                    log::info!("No todo or draft tasks found.");
                } else {
                    log::info!("No todo tasks found.");
                }
            } else {
                // Candidates exist but none are runnable - use runnability report for accurate messaging
                let options = RunnableSelectionOptions::new(include_draft, true);
                match crate::queue::operations::queue_runnability_report(
                    &queue_file,
                    done_ref,
                    options,
                ) {
                    Ok(report) => {
                        if report.summary.blocked_by_schedule > 0
                            && report.summary.blocked_by_dependencies > 0
                        {
                            log::info!(
                                "All runnable tasks are blocked by unmet dependencies and future schedule ({} deps, {} scheduled).",
                                report.summary.blocked_by_dependencies,
                                report.summary.blocked_by_schedule
                            );
                        } else if report.summary.blocked_by_schedule > 0 {
                            log::info!(
                                "All runnable tasks are blocked by future schedule ({} scheduled).",
                                report.summary.blocked_by_schedule
                            );
                        } else if report.summary.blocked_by_dependencies > 0 {
                            log::info!(
                                "All runnable tasks are blocked by unmet dependencies ({} blocked).",
                                report.summary.blocked_by_dependencies
                            );
                        } else if report.summary.blocked_by_status_or_flags > 0 {
                            log::info!(
                                "All tasks are blocked by status or flags ({} blocked).",
                                report.summary.blocked_by_status_or_flags
                            );
                        } else {
                            log::info!("All runnable tasks are blocked.");
                        }
                        log::info!("Run 'ralph queue explain' for details.");
                    }
                    Err(e) => {
                        // If analysis fails, avoid claiming a specific blocker.
                        log::info!(
                            "No runnable tasks found (failed to analyze blockers: {}).",
                            e
                        );
                        log::info!("Run 'ralph queue explain' for details.");
                    }
                }
            }
            return Ok(RunOutcome::NoTodo);
        }
    };

    let mut task = queue_file.tasks[task_idx].clone();
    let task_id = task.id.trim().to_string();

    let iteration_settings = resolve_iteration_settings(&task, &resolved.config.agent)?;
    log::info!(
        "RunOne: selected {task_id} (phases={phases}, iterations={})",
        iteration_settings.count
    );

    // Resolve per-phase settings matrix for the execution.
    let (phase_matrix, phase_warnings) = runner::resolve_phase_settings_matrix(
        agent_overrides,
        &resolved.config.agent,
        task.agent.as_ref(),
        phases,
    )?;

    // Log resolution warnings if any phase overrides won't be used.
    if phase_warnings.unused_phase1 {
        log::warn!("Task {task_id}: Phase 1 overrides specified but will not be used (phases < 2)");
    }
    if phase_warnings.unused_phase2 {
        log::warn!(
            "Task {task_id}: Phase 2 overrides specified but will not be used (phases < 2 or single-phase mode)"
        );
    }
    if phase_warnings.unused_phase3 {
        log::warn!("Task {task_id}: Phase 3 overrides specified but will not be used (phases < 3)");
    }

    // Log resolved per-phase matrix for visibility.
    log::info!("Task {task_id}: Resolved phase settings:");
    if phases >= 2 {
        log::info!(
            "  Phase 1 (Planning): runner={:?}, model={}",
            phase_matrix.phase1.runner,
            phase_matrix.phase1.model.as_str()
        );
    }
    log::info!(
        "  Phase 2 (Implementation): runner={:?}, model={}",
        phase_matrix.phase2.runner,
        phase_matrix.phase2.model.as_str()
    );
    if phases >= 3 {
        log::info!(
            "  Phase 3 (Review): runner={:?}, model={}",
            phase_matrix.phase3.runner,
            phase_matrix.phase3.model.as_str()
        );
    }

    // Require clean repo before the first iteration starts.
    let preexisting_dirty_allowed = git::repo_dirty_only_allowed_paths(
        &resolved.repo_root,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    // Optional pre-run task update: run once per task ID, immediately before we mark the task as doing.
    let mut update_task_before_run = agent_overrides
        .update_task_before_run
        .or(resolved.config.agent.update_task_before_run)
        .unwrap_or(false);

    let fail_on_prerun_update_error = agent_overrides
        .fail_on_prerun_update_error
        .or(resolved.config.agent.fail_on_prerun_update_error)
        .unwrap_or(false);

    if matches!(post_run_mode, phases::PostRunMode::ParallelWorker) && update_task_before_run {
        log::info!(
            "Task {task_id}: parallel worker mode skips pre-run task update to avoid queue writes"
        );
        update_task_before_run = false;
    }

    if update_task_before_run {
        log::info!("Task {task_id}: pre-run update enabled; running task updater");
        // Determine which phase settings to use for pre-run update:
        // - Multi-phase (phases >= 2): use Phase 1 (planning) settings
        // - Single-phase (phases = 1): use Phase 2 (implementation) settings
        let update_phase_settings = if phases >= 2 {
            &phase_matrix.phase1
        } else {
            &phase_matrix.phase2
        };
        let runner_cli_overrides = RunnerCliOptionsPatch {
            output_format: Some(update_phase_settings.runner_cli.output_format),
            verbosity: Some(update_phase_settings.runner_cli.verbosity),
            approval_mode: Some(update_phase_settings.runner_cli.approval_mode),
            sandbox: Some(update_phase_settings.runner_cli.sandbox),
            plan_mode: Some(update_phase_settings.runner_cli.plan_mode),
            unsupported_option_policy: Some(
                update_phase_settings.runner_cli.unsupported_option_policy,
            ),
        };
        let update_settings = task_cmd::TaskUpdateSettings {
            fields: "scope,evidence,plan,notes,tags,depends_on".to_string(),
            runner_override: Some(update_phase_settings.runner.clone()),
            model_override: Some(update_phase_settings.model.clone()),
            reasoning_effort_override: update_phase_settings.reasoning_effort,
            runner_cli_overrides,
            force,
            repoprompt_tool_injection: policy.repoprompt_tool_injection,
            dry_run: false,
        };

        // Run pre-run task update, but don't fail if it errors - log warning and continue
        match task_cmd::update_task_without_lock(resolved, &task_id, &update_settings) {
            Ok(()) => {
                log::info!("Task {task_id}: pre-run update completed successfully");
            }
            Err(err) => {
                if runutil::abort_reason(&err).is_some() {
                    return Err(err);
                }
                if fail_on_prerun_update_error {
                    return Err(anyhow::anyhow!(
                        "Pre-run task update failed for {}: {}\n\n\
                         Troubleshooting:\n\
                         - Check runner configuration (agent.runner, agent.model)\n\
                         - Verify runner binary is on PATH\n\
                         - Run with --force to skip this check\n\
                         - Or set fail_on_prerun_update_error: false in config to warn only",
                        task_id,
                        err
                    ));
                }
                log::warn!(
                    "Task {task_id}: pre-run update failed (continuing with original task): {:#}",
                    err
                );
                log::debug!("Pre-run update error details: {:?}", err);
                // Continue with original task - don't fail the run
            }
        }

        // Reload the task so the execution prompt includes updated fields.
        // Use repair mechanism in case the update left malformed JSON.
        // Validate after repair to catch semantic errors early.
        let (updated_queue_file, validation_warnings) = queue::load_queue_with_repair_and_validate(
            &resolved.queue_path,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )
        .context("validate repaired queue after pre-run update")?;
        queue::log_warnings(&validation_warnings);
        task = updated_queue_file
            .tasks
            .into_iter()
            .find(|t| t.id.trim() == task_id)
            .context("reload selected task after pre-run update")?;
    }

    // Mark the task as doing before running the agent (skip in parallel worker mode).
    if matches!(post_run_mode, phases::PostRunMode::ParallelWorker) {
        log::info!(
            "Task {task_id}: parallel worker mode skips mark_task_doing to avoid queue writes"
        );
    } else {
        mark_task_doing(resolved, &task_id)?;
    }

    // Save session state for crash recovery (before task execution)
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let session = create_session_for_task(
        &task_id,
        resolved,
        agent_overrides,
        iteration_settings.count,
        Some(&phase_matrix),
    );
    if let Err(e) = session::save_session(&cache_dir, &session) {
        log::warn!("Failed to save session state: {}", e);
    }

    let bins = runner::resolve_binaries(&resolved.config.agent);

    log::info!("Task {task_id}: start");

    // Create execution timings accumulator for CLI runs (not parallel worker mode)
    let execution_timings: Option<RefCell<RunExecutionTimings>> =
        if post_run_mode == phases::PostRunMode::ParallelWorker {
            None
        } else {
            Some(RefCell::new(RunExecutionTimings::default()))
        };

    let exec_result: Result<()> = (|| {
        // --- Prompt Construction ---
        let template = prompts::load_worker_prompt(&resolved.repo_root)?;
        let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
        let mut base_prompt =
            prompts::render_worker_prompt(&template, &task_id, project_type, &resolved.config)?;
        base_prompt = prompts::wrap_with_instruction_files(
            &resolved.repo_root,
            &base_prompt,
            &resolved.config,
        )?;

        // Inject an authoritative task context block to prevent the agent from selecting
        // a different task (e.g., "first todo" or "lowest ID") after Ralph marks the
        // selected task as `doing`.
        let task_context = task_context_for_prompt(&task)?;
        base_prompt = format!("{task_context}\n\n---\n\n{base_prompt}");

        let output_stream = if output_handler.is_some() {
            runner::OutputStream::HandlerOnly
        } else {
            runner::OutputStream::Terminal
        };

        for iteration_index in 1..=iteration_settings.count {
            let is_followup = iteration_index > 1;
            let is_final_iteration = iteration_index == iteration_settings.count;

            log::info!(
                "Task {task_id}: iteration {iteration_index}/{}",
                iteration_settings.count
            );

            // Apply follow-up reasoning effort to Phase 2 settings (implementation phase).
            // Phase 1 and Phase 3 use their original settings.
            let phase2_settings = apply_followup_reasoning_effort(
                &phase_matrix.phase2.to_agent_settings(),
                iteration_settings.followup_reasoning_effort,
                is_followup,
            );

            let iteration_context = if is_followup {
                prompts::ITERATION_CONTEXT_REFINEMENT
            } else {
                ""
            };
            let iteration_completion_block = if is_final_iteration {
                ""
            } else {
                prompts::ITERATION_COMPLETION_BLOCK
            };
            let phase3_completion_guidance = if is_final_iteration {
                prompts::PHASE3_COMPLETION_GUIDANCE_FINAL
            } else {
                prompts::PHASE3_COMPLETION_GUIDANCE_NONFINAL
            };

            // Helper to build webhook context for a phase
            let webhook_ctx_for_phase =
                |phase: u8, settings: &runner::AgentSettings| -> crate::webhook::WebhookContext {
                    crate::webhook::WebhookContext {
                        runner: Some(format!("{:?}", settings.runner).to_lowercase()),
                        model: Some(settings.model.as_str().to_string()),
                        phase: Some(phase),
                        phase_count: Some(phases),
                        repo_root: Some(resolved.repo_root.display().to_string()),
                        branch: crate::git::current_branch(&resolved.repo_root).ok(),
                        commit: crate::session::get_git_head_commit(&resolved.repo_root),
                        ..Default::default()
                    }
                };

            let ci_gate_enabled = resolved.config.agent.ci_gate_enabled.unwrap_or(true);
            let ci_gate_status_for_result = |result: &Result<(), anyhow::Error>| -> Option<String> {
                if !ci_gate_enabled {
                    return Some("skipped".to_string());
                }

                match result {
                    Ok(()) => Some("passed".to_string()),
                    Err(err) => {
                        // Only report "failed" when we are confident the CI gate ran and failed.
                        // Other errors (runner issues, user interrupts, etc.) may prevent CI from
                        // running at all, so we omit `ci_gate` in those cases.
                        let msg = format!("{err:#}");
                        if msg.contains("CI failed:") {
                            Some("failed".to_string())
                        } else {
                            None
                        }
                    }
                }
            };

            match phases {
                2 => {
                    // Phase 1: Planning - use phase1 settings
                    let phase1_invocation = phases::PhaseInvocation {
                        resolved,
                        settings: &phase_matrix.phase1.to_agent_settings(),
                        bins,
                        task_id: &task_id,
                        base_prompt: &base_prompt,
                        policy: &policy,
                        output_handler: output_handler.clone(),
                        output_stream,
                        project_type,
                        git_revert_mode,
                        git_commit_push_enabled,
                        push_policy,
                        revert_prompt: revert_prompt.clone(),
                        iteration_context,
                        iteration_completion_block,
                        phase3_completion_guidance,
                        is_final_iteration,
                        allow_dirty_repo: is_followup || preexisting_dirty_allowed,
                        post_run_mode,
                        notify_on_complete: agent_overrides.notify_on_complete,
                        notify_sound: agent_overrides.notify_sound,
                        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                        no_progress: agent_overrides.no_progress.unwrap_or(false),
                        execution_timings: execution_timings.as_ref(),
                    };

                    // Phase 1 webhook events
                    let phase1_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let phase1_start = std::time::Instant::now();
                    let phase1_ctx =
                        webhook_ctx_for_phase(1, &phase_matrix.phase1.to_agent_settings());
                    crate::webhook::notify_phase_started(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase1_started_at,
                        phase1_ctx.clone(),
                    );

                    let plan_text = phases::execute_phase1_planning(&phase1_invocation, 2)?;

                    let phase1_completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let mut phase1_ctx_done = phase1_ctx;
                    phase1_ctx_done.duration_ms = Some(phase1_start.elapsed().as_millis() as u64);
                    crate::webhook::notify_phase_completed(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase1_completed_at,
                        phase1_ctx_done,
                    );

                    // Phase 2: Implementation - use phase2 settings (with follow-up effort applied)
                    let phase2_invocation = phases::PhaseInvocation {
                        resolved,
                        settings: &phase2_settings,
                        bins,
                        task_id: &task_id,
                        base_prompt: &base_prompt,
                        policy: &policy,
                        output_handler: output_handler.clone(),
                        output_stream,
                        project_type,
                        git_revert_mode,
                        git_commit_push_enabled,
                        push_policy,
                        revert_prompt: revert_prompt.clone(),
                        iteration_context,
                        iteration_completion_block,
                        phase3_completion_guidance,
                        is_final_iteration,
                        allow_dirty_repo: is_followup || preexisting_dirty_allowed,
                        post_run_mode,
                        notify_on_complete: agent_overrides.notify_on_complete,
                        notify_sound: agent_overrides.notify_sound,
                        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                        no_progress: agent_overrides.no_progress.unwrap_or(false),
                        execution_timings: execution_timings.as_ref(),
                    };

                    // Phase 2 webhook events
                    let phase2_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let phase2_start = std::time::Instant::now();
                    let phase2_ctx = webhook_ctx_for_phase(2, &phase2_settings);
                    crate::webhook::notify_phase_started(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase2_started_at,
                        phase2_ctx.clone(),
                    );

                    let phase2_result =
                        phases::execute_phase2_implementation(&phase2_invocation, 2, &plan_text);

                    let phase2_completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let mut phase2_ctx_done = phase2_ctx;
                    phase2_ctx_done.duration_ms = Some(phase2_start.elapsed().as_millis() as u64);
                    phase2_ctx_done.ci_gate = ci_gate_status_for_result(&phase2_result);
                    crate::webhook::notify_phase_completed(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase2_completed_at,
                        phase2_ctx_done,
                    );

                    phase2_result?;
                }
                3 => {
                    // Phase 1: Planning - use phase1 settings
                    let phase1_invocation = phases::PhaseInvocation {
                        resolved,
                        settings: &phase_matrix.phase1.to_agent_settings(),
                        bins,
                        task_id: &task_id,
                        base_prompt: &base_prompt,
                        policy: &policy,
                        output_handler: output_handler.clone(),
                        output_stream,
                        project_type,
                        git_revert_mode,
                        git_commit_push_enabled,
                        push_policy,
                        revert_prompt: revert_prompt.clone(),
                        iteration_context,
                        iteration_completion_block,
                        phase3_completion_guidance,
                        is_final_iteration,
                        allow_dirty_repo: is_followup || preexisting_dirty_allowed,
                        post_run_mode,
                        notify_on_complete: agent_overrides.notify_on_complete,
                        notify_sound: agent_overrides.notify_sound,
                        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                        no_progress: agent_overrides.no_progress.unwrap_or(false),
                        execution_timings: execution_timings.as_ref(),
                    };

                    // Phase 1 webhook events
                    let phase1_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let phase1_start = std::time::Instant::now();
                    let phase1_ctx =
                        webhook_ctx_for_phase(1, &phase_matrix.phase1.to_agent_settings());
                    crate::webhook::notify_phase_started(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase1_started_at,
                        phase1_ctx.clone(),
                    );

                    let plan_text = phases::execute_phase1_planning(&phase1_invocation, 3)?;

                    let phase1_completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let mut phase1_ctx_done = phase1_ctx;
                    phase1_ctx_done.duration_ms = Some(phase1_start.elapsed().as_millis() as u64);
                    crate::webhook::notify_phase_completed(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase1_completed_at,
                        phase1_ctx_done,
                    );

                    // Phase 2: Implementation - use phase2 settings (with follow-up effort applied)
                    let phase2_invocation = phases::PhaseInvocation {
                        resolved,
                        settings: &phase2_settings,
                        bins,
                        task_id: &task_id,
                        base_prompt: &base_prompt,
                        policy: &policy,
                        output_handler: output_handler.clone(),
                        output_stream,
                        project_type,
                        git_revert_mode,
                        git_commit_push_enabled,
                        push_policy,
                        revert_prompt: revert_prompt.clone(),
                        iteration_context,
                        iteration_completion_block,
                        phase3_completion_guidance,
                        is_final_iteration,
                        allow_dirty_repo: is_followup || preexisting_dirty_allowed,
                        post_run_mode,
                        notify_on_complete: agent_overrides.notify_on_complete,
                        notify_sound: agent_overrides.notify_sound,
                        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                        no_progress: agent_overrides.no_progress.unwrap_or(false),
                        execution_timings: execution_timings.as_ref(),
                    };

                    // Phase 2 webhook events
                    let phase2_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let phase2_start = std::time::Instant::now();
                    let phase2_ctx = webhook_ctx_for_phase(2, &phase2_settings);
                    crate::webhook::notify_phase_started(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase2_started_at,
                        phase2_ctx.clone(),
                    );

                    let phase2_result =
                        phases::execute_phase2_implementation(&phase2_invocation, 3, &plan_text);

                    let phase2_completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let mut phase2_ctx_done = phase2_ctx;
                    phase2_ctx_done.duration_ms = Some(phase2_start.elapsed().as_millis() as u64);
                    phase2_ctx_done.ci_gate = ci_gate_status_for_result(&phase2_result);
                    crate::webhook::notify_phase_completed(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase2_completed_at,
                        phase2_ctx_done,
                    );

                    phase2_result?;

                    // Phase 3: Review - use phase3 settings
                    let phase3_invocation = phases::PhaseInvocation {
                        resolved,
                        settings: &phase_matrix.phase3.to_agent_settings(),
                        bins,
                        task_id: &task_id,
                        base_prompt: &base_prompt,
                        policy: &policy,
                        output_handler: output_handler.clone(),
                        output_stream,
                        project_type,
                        git_revert_mode,
                        git_commit_push_enabled,
                        push_policy,
                        revert_prompt: revert_prompt.clone(),
                        iteration_context,
                        iteration_completion_block,
                        phase3_completion_guidance,
                        is_final_iteration,
                        allow_dirty_repo: is_followup || preexisting_dirty_allowed,
                        post_run_mode,
                        notify_on_complete: agent_overrides.notify_on_complete,
                        notify_sound: agent_overrides.notify_sound,
                        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                        no_progress: agent_overrides.no_progress.unwrap_or(false),
                        execution_timings: execution_timings.as_ref(),
                    };

                    // Phase 3 webhook events
                    let phase3_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let phase3_start = std::time::Instant::now();
                    let phase3_ctx =
                        webhook_ctx_for_phase(3, &phase_matrix.phase3.to_agent_settings());
                    crate::webhook::notify_phase_started(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase3_started_at,
                        phase3_ctx.clone(),
                    );

                    let phase3_result = phases::execute_phase3_review(&phase3_invocation);

                    let phase3_completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let mut phase3_ctx_done = phase3_ctx;
                    phase3_ctx_done.duration_ms = Some(phase3_start.elapsed().as_millis() as u64);
                    phase3_ctx_done.ci_gate = ci_gate_status_for_result(&phase3_result);
                    crate::webhook::notify_phase_completed(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase3_completed_at,
                        phase3_ctx_done,
                    );

                    phase3_result?;
                }
                1 => {
                    // Single-phase: use Phase 2 settings (with follow-up effort applied)
                    let single_invocation = phases::PhaseInvocation {
                        resolved,
                        settings: &phase2_settings,
                        bins,
                        task_id: &task_id,
                        base_prompt: &base_prompt,
                        policy: &policy,
                        output_handler: output_handler.clone(),
                        output_stream,
                        project_type,
                        git_revert_mode,
                        git_commit_push_enabled,
                        push_policy,
                        revert_prompt: revert_prompt.clone(),
                        iteration_context,
                        iteration_completion_block,
                        phase3_completion_guidance,
                        is_final_iteration,
                        allow_dirty_repo: is_followup || preexisting_dirty_allowed,
                        post_run_mode,
                        notify_on_complete: agent_overrides.notify_on_complete,
                        notify_sound: agent_overrides.notify_sound,
                        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                        no_progress: agent_overrides.no_progress.unwrap_or(false),
                        execution_timings: execution_timings.as_ref(),
                    };

                    // Single-phase (treated as phase 2) webhook events
                    let phase_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let phase_start = std::time::Instant::now();
                    let phase_ctx = webhook_ctx_for_phase(2, &phase2_settings);
                    crate::webhook::notify_phase_started(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase_started_at,
                        phase_ctx.clone(),
                    );

                    let phase_result = phases::execute_single_phase(&single_invocation);

                    let phase_completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
                    let mut phase_ctx_done = phase_ctx;
                    phase_ctx_done.duration_ms = Some(phase_start.elapsed().as_millis() as u64);
                    phase_ctx_done.ci_gate = ci_gate_status_for_result(&phase_result);
                    crate::webhook::notify_phase_completed(
                        &task_id,
                        &task.title,
                        &resolved.config.agent.webhook,
                        &phase_completed_at,
                        phase_ctx_done,
                    );

                    phase_result?;
                }
                _ => {
                    bail!(
                        "Invalid phases value: {} (expected 1, 2, or 3). \
                         This indicates a configuration error or internal inconsistency.",
                        phases
                    );
                }
            }
        }

        Ok(())
    })();

    match exec_result {
        Ok(()) => {
            log::info!("Task {task_id}: end");

            // Persist execution history after successful completion
            if post_run_mode != phases::PostRunMode::ParallelWorker
                && let Some(timings) = execution_timings
            {
                match execution_history_cli::try_record_execution_history_for_cli_run(
                    &resolved.repo_root,
                    &resolved.done_path,
                    &task_id,
                    phases,
                    timings.into_inner(),
                ) {
                    Ok(true) => {
                        log::debug!("Recorded execution history for {} (CLI mode)", task_id)
                    }
                    Ok(false) => log::debug!(
                        "Skipping execution history for {}: task not Done or timing payload unavailable.",
                        task_id
                    ),
                    Err(err) => log::warn!(
                        "Failed to record execution history for {}: {}",
                        task_id,
                        err
                    ),
                }
            }

            Ok(RunOutcome::Ran { task_id })
        }
        Err(err) => {
            // Keep task-level error concise; phase scopes will log detailed boundaries.
            log::error!("Task {task_id}: error");

            // Send failure notification
            let notify_on_complete = agent_overrides
                .notify_on_complete
                .or(resolved.config.agent.notification.notify_on_complete)
                .unwrap_or(true);
            let notify_on_fail = agent_overrides
                .notify_on_fail
                .or(resolved.config.agent.notification.notify_on_fail)
                .unwrap_or(true);
            let notify_on_loop_complete = resolved
                .config
                .agent
                .notification
                .notify_on_loop_complete
                .unwrap_or(true);
            // enabled acts as a global on/off switch - true if ANY notification type is enabled
            let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

            let notify_config = crate::notification::NotificationConfig {
                enabled,
                notify_on_complete,
                notify_on_fail,
                notify_on_loop_complete,
                suppress_when_active: resolved
                    .config
                    .agent
                    .notification
                    .suppress_when_active
                    .unwrap_or(true),
                sound_enabled: agent_overrides
                    .notify_sound
                    .or(resolved.config.agent.notification.sound_enabled)
                    .unwrap_or(false),
                sound_path: resolved.config.agent.notification.sound_path.clone(),
                timeout_ms: resolved
                    .config
                    .agent
                    .notification
                    .timeout_ms
                    .unwrap_or(8000),
            };
            let error_summary = format!("{:#}", err);
            crate::notification::notify_task_failed(
                &task_id,
                &task.title,
                &error_summary,
                &notify_config,
            );

            Err(err)
        }
    }
}

#[cfg(test)]
fn resolve_run_agent_settings(
    resolved: &config::Resolved,
    task: &crate::contracts::Task,
    overrides: &AgentOverrides,
) -> Result<runner::AgentSettings> {
    runner::resolve_agent_settings(
        overrides.runner.clone(),
        overrides.model.clone(),
        overrides.reasoning_effort,
        &overrides.runner_cli,
        task.agent.as_ref(),
        &resolved.config.agent,
    )
}

#[cfg(test)]
fn update_then_mark_doing_if_configured<U, M>(
    update_enabled: bool,
    updater: U,
    marker: M,
) -> Result<()>
where
    U: FnOnce() -> Result<()>,
    M: FnOnce() -> Result<()>,
{
    if update_enabled {
        updater()?;
    }
    marker()?;
    Ok(())
}

#[cfg(test)]
mod pre_run_update_order_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn update_then_mark_doing_calls_update_first_when_enabled() {
        let calls = Cell::new(Vec::<&'static str>::new());

        update_then_mark_doing_if_configured(
            true,
            || {
                let mut v = calls.take();
                v.push("update");
                calls.set(v);
                Ok(())
            },
            || {
                let mut v = calls.take();
                v.push("mark");
                calls.set(v);
                Ok(())
            },
        )
        .expect("ok");

        assert_eq!(calls.take(), vec!["update", "mark"]);
    }

    #[test]
    fn update_then_mark_doing_skips_update_when_disabled() {
        let update_calls = Cell::new(0usize);
        let mark_calls = Cell::new(0usize);

        update_then_mark_doing_if_configured(
            false,
            || {
                update_calls.set(update_calls.get() + 1);
                Ok(())
            },
            || {
                mark_calls.set(mark_calls.get() + 1);
                Ok(())
            },
        )
        .expect("ok");

        assert_eq!(update_calls.get(), 0);
        assert_eq!(mark_calls.get(), 1);
    }
}

/// Dry-run selection for `ralph run one --dry-run`.
///
/// Performs task selection without:
/// - Acquiring queue lock
/// - Marking task as doing
/// - Starting any runner session
/// - Modifying queue/done files
pub fn dry_run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    target_task_id: Option<&str>,
) -> Result<()> {
    use crate::queue::operations::{RunnableSelectionOptions, queue_runnability_report};

    // Load queue and done (no lock in dry-run mode).
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };

    let include_draft = agent_overrides.include_draft.unwrap_or(false);

    // Match run-time validation behavior without mutating anything.
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    // Try to select a task
    let selected = select_run_one_task_index(&queue_file, done_ref, target_task_id, include_draft)?;

    if let Some(idx) = selected {
        let task = &queue_file.tasks[idx];
        println!("Dry run: would run {} (status: {:?})", task.id, task.status);

        // Also show any blockers (in case task is runnable but has caveats)
        if task.status == TaskStatus::Doing {
            println!("  Note: Task is already in 'doing' status (resuming).");
        }
        return Ok(());
    }

    // No task selected - explain why
    println!("Dry run: no task would be run.");
    println!();

    // Count candidates
    let candidates: Vec<_> = queue_file
        .tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
        })
        .collect();

    if candidates.is_empty() {
        if include_draft {
            println!("No todo or draft tasks found.");
        } else {
            println!("No todo tasks found.");
        }
        return Ok(());
    }

    // Build runnability report to explain blockers
    let options = RunnableSelectionOptions::new(include_draft, true);
    match queue_runnability_report(&queue_file, done_ref, options) {
        Ok(report) => {
            if report.summary.runnable_candidates > 0 {
                // This shouldn't happen if selection returned None, but handle it
                println!(
                    "Warning: runnability report found {} runnable candidates but selection returned none.",
                    report.summary.runnable_candidates
                );
            } else {
                println!("Blockers preventing task execution:");

                if report.summary.blocked_by_dependencies > 0 {
                    println!(
                        "  - {} task(s) blocked by unmet dependencies",
                        report.summary.blocked_by_dependencies
                    );
                }
                if report.summary.blocked_by_schedule > 0 {
                    println!(
                        "  - {} task(s) blocked by future schedule",
                        report.summary.blocked_by_schedule
                    );
                }
                if report.summary.blocked_by_status_or_flags > 0 {
                    println!(
                        "  - {} task(s) blocked by status/flags (e.g., draft excluded)",
                        report.summary.blocked_by_status_or_flags
                    );
                }

                // Show first blocking task
                println!();
                for row in &report.tasks {
                    let is_candidate = row.status == TaskStatus::Todo
                        || (include_draft && row.status == TaskStatus::Draft);
                    if !is_candidate || row.runnable || row.reasons.is_empty() {
                        continue;
                    }

                    println!("First blocking task: {} (status: {:?})", row.id, row.status);
                    for reason in &row.reasons {
                        match reason {
                            crate::queue::operations::NotRunnableReason::UnmetDependencies { dependencies } => {
                                if dependencies.len() == 1 {
                                    match &dependencies[0] {
                                        crate::queue::operations::DependencyIssue::Missing { id } => {
                                            println!("  - Missing dependency: {}", id);
                                        }
                                        crate::queue::operations::DependencyIssue::NotComplete { id, status } => {
                                            println!("  - Dependency {} not complete (status: {})", id, status);
                                        }
                                    }
                                } else {
                                    println!("  - {} unmet dependencies", dependencies.len());
                                }
                            }
                            crate::queue::operations::NotRunnableReason::ScheduledStartInFuture { scheduled_start, seconds_until_runnable, .. } => {
                                let hours = seconds_until_runnable / 3600;
                                let minutes = (seconds_until_runnable % 3600) / 60;
                                if hours > 0 {
                                    println!("  - Scheduled for: {} (in {}h {}m)",
                                        scheduled_start, hours, minutes);
                                } else {
                                    println!("  - Scheduled for: {} (in {}m)",
                                        scheduled_start, minutes);
                                }
                            }
                            crate::queue::operations::NotRunnableReason::DraftExcluded => {
                                println!("  - Draft tasks excluded (use --include-draft)");
                            }
                            crate::queue::operations::NotRunnableReason::StatusNotRunnable { status } => {
                                println!("  - Status prevents running: {}", status);
                            }
                        }
                    }
                    break;
                }
            }
        }
        Err(e) => {
            println!("Could not generate runnability report: {}", e);
        }
    }

    println!();
    println!("Run 'ralph queue explain' for a full report.");

    Ok(())
}

/// Dry-run selection for `ralph run loop --dry-run`.
///
/// Reports the first selection only since subsequent tasks depend on outcomes.
pub fn dry_run_loop(resolved: &config::Resolved, agent_overrides: &AgentOverrides) -> Result<()> {
    println!("Dry run: simulating run loop (reporting first selection only).");
    println!("Note: Subsequent tasks depend on outcomes of earlier tasks.");
    println!();

    dry_run_one(resolved, agent_overrides, None)
}

#[cfg(test)]
mod tests;
