//! Sequential run-loop lifecycle bookkeeping.
//!
//! Purpose:
//! - Sequential run-loop lifecycle bookkeeping.
//!
//! Responsibilities:
//! - Own loop-level counters, stop-signal state, notifications, and webhook lifecycle events.
//! - Provide explicit helpers for success/failure accounting and loop finalization.
//!
//! Not handled here:
//! - Per-task execution.
//! - Session recovery or queue waiting decisions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Loop webhooks bracket the entire run-loop attempt.
//! - Session progress persistence failures are warnings, not fatal loop errors.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use crate::config;
use crate::constants::limits::MAX_CONSECUTIVE_FAILURES;
use crate::{signal, webhook};

use super::types::{RunLoopOptions, RunLoopOutcome, RunLoopStats};

pub(super) struct LoopLifecycle {
    cache_dir: PathBuf,
    initial_todo_count: u32,
    completed: u32,
    stats: RunLoopStats,
    loop_start_time: Instant,
    loop_webhook_ctx: webhook::WebhookContext,
}

impl LoopLifecycle {
    pub(super) fn start(
        resolved: &config::Resolved,
        initial_todo_count: u32,
        completed: u32,
    ) -> Self {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        signal::clear_stop_signal_at_loop_start(&cache_dir);

        let loop_start_time = Instant::now();
        let loop_started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
        let loop_webhook_ctx = webhook::WebhookContext {
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

        Self {
            cache_dir,
            initial_todo_count,
            completed,
            stats: RunLoopStats::default(),
            loop_start_time,
            loop_webhook_ctx,
        }
    }

    pub(super) fn webhook_context(&self) -> &webhook::WebhookContext {
        &self.loop_webhook_ctx
    }

    pub(super) fn completed(&self) -> u32 {
        self.completed
    }

    pub(super) fn max_tasks_reached(&self, opts: &RunLoopOptions) -> bool {
        opts.max_tasks != 0 && self.completed >= opts.max_tasks
    }

    pub(super) fn stop_requested(&self) -> bool {
        signal::stop_signal_exists(&self.cache_dir)
    }

    pub(super) fn clear_stop_signal(&self) {
        if let Err(err) = signal::clear_stop_signal(&self.cache_dir) {
            log::warn!("Failed to clear stop signal: {}", err);
        }
    }

    pub(super) fn record_success(&mut self) {
        self.completed += 1;
        self.stats.tasks_attempted += 1;
        self.stats.tasks_succeeded += 1;
        self.stats.consecutive_failures = 0;
        self.persist_session_progress();

        if self.initial_todo_count == 0 {
            log::info!("RunLoop: task-complete (completed={})", self.completed);
        } else {
            log::info!(
                "RunLoop: task-complete ({}/{})",
                self.completed,
                self.initial_todo_count
            );
        }
    }

    pub(super) fn record_failure(&mut self, err: &anyhow::Error) -> Result<()> {
        self.completed += 1;
        self.stats.tasks_attempted += 1;
        self.stats.tasks_failed += 1;
        self.stats.consecutive_failures += 1;
        self.persist_session_progress();

        log::error!("RunLoop: task failed: {:#}", err);

        if self.stats.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            log::error!("RunLoop: aborting after {MAX_CONSECUTIVE_FAILURES} consecutive failures");
            return Err(anyhow::anyhow!(
                "Run loop aborted after {} consecutive task failures. This usually indicates a systemic issue (e.g., repo dirty, runner misconfiguration, or interrupt flag stuck). Check logs above for root cause.",
                MAX_CONSECUTIVE_FAILURES
            ));
        }

        Ok(())
    }

    pub(super) fn finish(
        self,
        resolved: &config::Resolved,
        opts: &RunLoopOptions,
        result: &Result<RunLoopOutcome>,
    ) {
        if self.stats.tasks_attempted > 0 {
            let notify_config = crate::notification::build_notification_config(
                &resolved.config.agent.notification,
                &crate::notification::NotificationOverrides {
                    notify_on_complete: opts.agent_overrides.notify_on_complete,
                    notify_on_fail: opts.agent_overrides.notify_on_fail,
                    notify_sound: opts.agent_overrides.notify_sound,
                },
            );
            crate::notification::notify_loop_complete(
                self.stats.tasks_attempted,
                self.stats.tasks_succeeded,
                self.stats.tasks_failed,
                &notify_config,
            );
        }

        let loop_stopped_at = crate::timeutil::now_utc_rfc3339_or_fallback();
        let loop_duration_ms = self.loop_start_time.elapsed().as_millis() as u64;
        let loop_note = match result {
            Ok(outcome) => Some(format!(
                "Outcome {:?}: {}/{} succeeded",
                outcome, self.stats.tasks_succeeded, self.stats.tasks_attempted
            )),
            Err(err) => Some(format!("Error: {}", err)),
        };
        webhook::notify_loop_stopped(
            &resolved.config.agent.webhook,
            &loop_stopped_at,
            webhook::WebhookContext {
                duration_ms: Some(loop_duration_ms),
                ..self.loop_webhook_ctx
            },
            loop_note.as_deref(),
        );

        let should_clear_session = matches!(
            result,
            Ok(RunLoopOutcome::Completed
                | RunLoopOutcome::NoCandidates { .. }
                | RunLoopOutcome::Blocked { .. })
        );
        if should_clear_session && let Err(err) = crate::session::clear_session(&self.cache_dir) {
            log::warn!("Failed to clear session on loop completion: {}", err);
        }
    }

    fn persist_session_progress(&self) {
        if let Err(err) = crate::session::increment_session_progress(&self.cache_dir) {
            log::warn!("Failed to persist session progress: {}", err);
        }
    }
}
