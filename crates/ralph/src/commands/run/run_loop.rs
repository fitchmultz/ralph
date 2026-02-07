//! Run loop orchestration.
//!
//! Responsibilities:
//! - Orchestrate the sequential run loop (`run_loop`).
//! - Handle session recovery and graceful stop signals.
//! - Track task completion statistics and send notifications.
//!
//! Not handled here:
//! - Individual task execution (see `run_one`).
//! - Parallel run loop (see `parallel`).
//! - Phase execution details (see `phases`).
//!
//! Invariants/assumptions:
//! - Queue lock contention errors are non-retriable to prevent infinite loops.
//! - Session timeout uses configured hours (defaults to 24 hours).

use crate::agent::AgentOverrides;
use crate::config;
use crate::constants::limits::MAX_CONSECUTIVE_FAILURES;
use crate::contracts::{ParallelMergeWhen, TaskStatus};
use crate::session::{self, SessionValidationResult};
use crate::signal;
use crate::{queue, runutil, webhook};
use anyhow::Result;

use super::queue_lock::{clear_stale_queue_lock_for_resume, is_queue_lock_already_held_error};
use super::run_one::{RunOutcome, run_one};

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
    /// Wait when blocked by dependencies/schedule instead of exiting.
    pub wait_when_blocked: bool,
    /// Poll interval in milliseconds while waiting (default: 1000).
    pub wait_poll_ms: u64,
    /// Timeout in seconds for waiting (0 = no timeout).
    pub wait_timeout_seconds: u64,
    /// Notify when queue becomes unblocked.
    pub notify_when_unblocked: bool,
    /// Wait when queue is empty instead of exiting (continuous mode).
    pub wait_when_empty: bool,
    /// Poll interval in milliseconds while waiting on an empty queue (default: 30000).
    pub empty_poll_ms: u64,
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
        return super::parallel::run_loop_parallel(
            resolved,
            super::parallel::ParallelRunOptions {
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
        if !opts.wait_when_empty {
            return Ok(());
        }
        // In continuous mode, continue into the loop to wait for work
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
    webhook::notify_loop_started(
        &resolved.config.agent.webhook,
        &loop_started_at,
        loop_webhook_ctx.clone(),
    );

    let result = super::logging::with_scope(&label, || {
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
                Ok(RunOutcome::NoCandidates) => {
                    if opts.wait_when_empty {
                        // Enter wait loop for new tasks
                        match wait_for_work(
                            resolved,
                            include_draft,
                            WaitMode::EmptyAllowed,
                            opts.wait_poll_ms,
                            opts.empty_poll_ms,
                            0, // No timeout for empty wait
                            opts.notify_when_unblocked,
                            &loop_webhook_ctx,
                        )? {
                            WaitExit::RunnableAvailable { .. } => {
                                log::info!("RunLoop: new runnable tasks detected; continuing");
                                continue;
                            }
                            WaitExit::NoCandidates => {
                                // Should not happen in EmptyAllowed mode, but handle gracefully
                                continue;
                            }
                            WaitExit::TimedOut => {
                                log::info!("RunLoop: end (wait timeout reached)");
                                return Ok(());
                            }
                            WaitExit::StopRequested => {
                                log::info!("RunLoop: end (stop signal received)");
                                return Ok(());
                            }
                        }
                    } else {
                        log::info!("RunLoop: end (no more todo tasks remaining)");
                        return Ok(());
                    }
                }
                Ok(RunOutcome::Blocked { summary }) => {
                    if opts.wait_when_blocked || opts.wait_when_empty {
                        // Determine wait mode based on flags
                        let mode = if opts.wait_when_empty {
                            WaitMode::EmptyAllowed
                        } else {
                            WaitMode::BlockedOnly
                        };
                        // Wait for a runnable task to become available
                        match wait_for_work(
                            resolved,
                            include_draft,
                            mode,
                            opts.wait_poll_ms,
                            opts.empty_poll_ms,
                            opts.wait_timeout_seconds,
                            opts.notify_when_unblocked,
                            &loop_webhook_ctx,
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
                                continue;
                            }
                            WaitExit::NoCandidates => {
                                log::info!("RunLoop: end (queue became empty while waiting)");
                                return Ok(());
                            }
                            WaitExit::TimedOut => {
                                log::info!("RunLoop: end (wait timeout reached)");
                                return Ok(());
                            }
                            WaitExit::StopRequested => {
                                log::info!("RunLoop: end (stop signal received)");
                                return Ok(());
                            }
                        }
                    } else {
                        // Not in wait mode - exit with helpful message
                        log::info!(
                            "RunLoop: end (blocked: ready={} deps={} sched={}). \
                             Use --wait-when-blocked to wait for dependencies/schedules.",
                            summary.runnable_candidates,
                            summary.blocked_by_dependencies,
                            summary.blocked_by_schedule
                        );
                        return Ok(());
                    }
                }
                Ok(RunOutcome::Ran { task_id: _ }) => {
                    completed += 1;
                    tasks_attempted += 1;
                    tasks_succeeded += 1;
                    consecutive_failures = 0; // Reset on success
                    if initial_todo_count == 0 {
                        log::info!("RunLoop: task-complete (completed={completed})");
                    } else {
                        log::info!("RunLoop: task-complete ({completed}/{initial_todo_count})");
                    }
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
    webhook::notify_loop_stopped(
        &resolved.config.agent.webhook,
        &loop_stopped_at,
        webhook::WebhookContext {
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

/// Wait mode for the wait loop.
#[derive(Debug)]
enum WaitMode {
    /// Blocked-only mode: exit if queue becomes empty while waiting.
    BlockedOnly,
    /// Empty-allowed mode: keep waiting even if queue is empty.
    EmptyAllowed,
}

/// Exit reason from the wait loop.
enum WaitExit {
    /// A runnable task became available.
    RunnableAvailable {
        summary: crate::queue::operations::QueueRunnabilitySummary,
    },
    /// Queue became empty while waiting (only in BlockedOnly mode).
    NoCandidates,
    /// Wait timeout reached.
    TimedOut,
    /// Stop signal was received.
    StopRequested,
}

/// Internal file watcher for queue changes.
struct QueueFileWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
}

impl QueueFileWatcher {
    fn new(resolved: &config::Resolved) -> anyhow::Result<Self> {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )?;

        // Watch the `.ralph` directory so queue/done changes are seen
        let ralph_dir = resolved.repo_root.join(".ralph");
        if ralph_dir.exists() {
            watcher.watch(&ralph_dir, RecursiveMode::NonRecursive)?;
        }

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    fn recv_timeout(&self, dur: std::time::Duration) -> Result<(), ()> {
        match self.rx.recv_timeout(dur) {
            Ok(_) => Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(()),
            Err(_) => Err(()),
        }
    }
}

/// Wait for runnable tasks with notify-based wake and poll fallback.
///
/// Supports both blocked-wait and empty-wait modes.
#[allow(clippy::too_many_arguments)]
fn wait_for_work(
    resolved: &config::Resolved,
    include_draft: bool,
    mode: WaitMode,
    blocked_poll_ms: u64,
    empty_poll_ms: u64,
    timeout_seconds: u64,
    notify_when_unblocked: bool,
    loop_webhook_ctx: &crate::webhook::WebhookContext,
) -> Result<WaitExit> {
    use std::time::{Duration, Instant};

    let cache_dir = resolved.repo_root.join(".ralph/cache");

    // Clamp poll intervals
    let blocked_poll_ms = blocked_poll_ms.max(50);
    let empty_poll_ms = empty_poll_ms.max(50);

    let start = Instant::now();
    let tick = Duration::from_millis(250);

    // Initialize Ctrl+C handler check
    let ctrlc = crate::runner::ctrlc_state().ok();

    // Best-effort file watcher
    let watcher = QueueFileWatcher::new(resolved).ok();
    if watcher.is_none() {
        log::debug!("File watcher setup failed, using poll-only mode");
    }

    let poll_ms = match mode {
        WaitMode::BlockedOnly => blocked_poll_ms,
        WaitMode::EmptyAllowed => empty_poll_ms,
    };

    log::info!(
        "Waiting for runnable tasks (mode={:?}, poll={}ms, timeout={}s)...",
        mode,
        poll_ms,
        if timeout_seconds == 0 {
            "none".to_string()
        } else {
            timeout_seconds.to_string()
        }
    );

    let mut last_eval = Instant::now();
    let mut pending_event = true; // Force initial eval

    loop {
        // Check for timeout
        if timeout_seconds != 0 {
            let elapsed = start.elapsed().as_secs();
            if elapsed >= timeout_seconds {
                return Ok(WaitExit::TimedOut);
            }
        }

        // Check for stop signal
        if signal::stop_signal_exists(&cache_dir) {
            if let Err(e) = signal::clear_stop_signal(&cache_dir) {
                log::warn!("Failed to clear stop signal: {}", e);
            }
            return Ok(WaitExit::StopRequested);
        }

        // Check for Ctrl+C
        if ctrlc
            .as_ref()
            .is_some_and(|c| c.interrupted.load(std::sync::atomic::Ordering::SeqCst))
        {
            return Err(runutil::RunAbort::new(
                runutil::RunAbortReason::Interrupted,
                "Ctrl+C pressed while waiting for runnable tasks",
            )
            .into());
        }

        // Wait for tick or file event
        if let Some(ref w) = watcher {
            let _ = w.recv_timeout(tick);
            pending_event = true;
        } else {
            std::thread::sleep(tick);
        }

        // Decide whether to re-evaluate queue
        let poll_dur = Duration::from_millis(poll_ms);
        if pending_event || last_eval.elapsed() >= poll_dur {
            pending_event = false;
            last_eval = Instant::now();

            // Load queue and done files
            let queue_file = match queue::load_queue(&resolved.queue_path) {
                Ok(q) => q,
                Err(e) => {
                    log::warn!("Failed to load queue while waiting: {}; will retry", e);
                    continue;
                }
            };

            let done = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done)
            };

            // Generate runnability report
            let options = queue::RunnableSelectionOptions::new(include_draft, true);
            let report = match crate::queue::operations::queue_runnability_report(
                &queue_file,
                done_ref,
                options,
            ) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!(
                        "Failed to generate runnability report while waiting: {}; will retry",
                        e
                    );
                    continue;
                }
            };

            // Check exit conditions
            if report.summary.candidates_total == 0 {
                match mode {
                    WaitMode::BlockedOnly => {
                        return Ok(WaitExit::NoCandidates);
                    }
                    WaitMode::EmptyAllowed => {
                        // Keep waiting for new tasks
                        continue;
                    }
                }
            }

            if report.summary.runnable_candidates > 0 {
                // Queue became unblocked!
                if notify_when_unblocked {
                    notify_queue_unblocked(&report.summary, resolved, loop_webhook_ctx);
                }
                return Ok(WaitExit::RunnableAvailable {
                    summary: report.summary,
                });
            }

            // Still blocked - continue waiting
        }
    }
}

/// Send notifications when queue becomes unblocked.
fn notify_queue_unblocked(
    summary: &crate::queue::operations::QueueRunnabilitySummary,
    resolved: &config::Resolved,
    loop_webhook_ctx: &crate::webhook::WebhookContext,
) {
    // Build summary note
    let note = format!(
        "ready={} blocked_deps={} blocked_schedule={}",
        summary.runnable_candidates, summary.blocked_by_dependencies, summary.blocked_by_schedule
    );

    // Desktop notification
    let notify_config = crate::notification::NotificationConfig {
        enabled: true,
        notify_on_complete: false,
        notify_on_fail: false,
        notify_on_loop_complete: false,
        suppress_when_active: resolved
            .config
            .agent
            .notification
            .suppress_when_active
            .unwrap_or(true),
        sound_enabled: resolved
            .config
            .agent
            .notification
            .sound_enabled
            .unwrap_or(false),
        sound_path: resolved.config.agent.notification.sound_path.clone(),
        timeout_ms: resolved
            .config
            .agent
            .notification
            .timeout_ms
            .unwrap_or(8000),
    };

    #[cfg(feature = "notifications")]
    {
        use notify_rust::{Notification, Timeout};
        if let Err(e) = Notification::new()
            .summary("Ralph: tasks runnable")
            .body(&note)
            .timeout(Timeout::Milliseconds(notify_config.timeout_ms))
            .show()
        {
            log::debug!("Failed to show unblocked notification: {}", e);
        }

        if notify_config.sound_enabled {
            let _ = crate::notification::play_completion_sound(notify_config.sound_path.as_deref());
        }
    }

    // Webhook notification
    let timestamp = crate::timeutil::now_utc_rfc3339_or_fallback();
    let payload = crate::webhook::WebhookPayload {
        event: "queue_unblocked".to_string(),
        timestamp,
        task_id: None,
        task_title: None,
        previous_status: Some("blocked".to_string()),
        current_status: Some("runnable".to_string()),
        note: Some(note),
        context: loop_webhook_ctx.clone(),
    };
    crate::webhook::send_webhook_payload(payload, &resolved.config.agent.webhook);
}
