//! Parallel run-loop shutdown and finalization.
//!
//! Purpose:
//! - Parallel run-loop shutdown and finalization.
//!
//! Responsibilities:
//! - Emit final notifications/webhooks.
//! - Clear stop-signal state and decide whether to surface interrupt errors.
//!
//! Not handled here:
//! - Active worker orchestration.
//! - Preflight/bootstrap.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Called exactly once after the orchestration loop finishes.

use anyhow::Result;

use crate::commands::run::RunLoopOutcome;
use crate::config;
use crate::{runutil, signal};

use super::preflight::PreparedParallelRun;

pub(super) fn finalize_parallel_run(
    resolved: &config::Resolved,
    opts: &crate::commands::run::parallel::ParallelRunOptions,
    prepared: &mut PreparedParallelRun,
    loop_result: Result<RunLoopOutcome>,
) -> Result<RunLoopOutcome> {
    if prepared.interrupted || loop_result.is_err() {
        let loop_stopped_at = crate::timeutil::now_utc_rfc3339_or_fallback();
        let loop_duration_ms = prepared.loop_start_time.elapsed().as_millis() as u64;
        let loop_note = if prepared.interrupted {
            Some("Parallel run interrupted by Ctrl+C".to_string())
        } else {
            loop_result.as_ref().err().map(|err| err.to_string())
        };
        crate::webhook::notify_loop_stopped(
            &resolved.config.agent.webhook,
            &loop_stopped_at,
            crate::webhook::WebhookContext {
                duration_ms: Some(loop_duration_ms),
                ..prepared.loop_webhook_ctx.clone()
            },
            loop_note.as_deref(),
        );

        if prepared.interrupted {
            return Err(runutil::RunAbort::new(
                runutil::RunAbortReason::Interrupted,
                "Parallel run interrupted by Ctrl+C",
            )
            .into());
        }

        return loop_result;
    }

    prepared.guard.mark_completed();

    if (prepared.stop_requested || signal::stop_signal_exists(&prepared.cache_dir))
        && let Err(err) = signal::clear_stop_signal(&prepared.cache_dir)
    {
        log::warn!("Failed to clear stop signal: {}", err);
    }

    if prepared.stats.attempted() > 0 {
        let notify_config = crate::notification::build_notification_config(
            &resolved.config.agent.notification,
            &crate::notification::NotificationOverrides {
                notify_on_complete: opts.agent_overrides.notify_on_complete,
                notify_on_fail: opts.agent_overrides.notify_on_fail,
                notify_sound: opts.agent_overrides.notify_sound,
            },
        );
        crate::notification::notify_loop_complete(
            prepared.stats.attempted(),
            prepared.stats.succeeded(),
            prepared.stats.failed(),
            &notify_config,
        );
    }

    let loop_stopped_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let loop_duration_ms = prepared.loop_start_time.elapsed().as_millis() as u64;
    let loop_note = Some(format!(
        "Parallel run completed: {}/{} succeeded, {} failed",
        prepared.stats.succeeded(),
        prepared.stats.attempted(),
        prepared.stats.failed()
    ));
    crate::webhook::notify_loop_stopped(
        &resolved.config.agent.webhook,
        &loop_stopped_at,
        crate::webhook::WebhookContext {
            duration_ms: Some(loop_duration_ms),
            ..prepared.loop_webhook_ctx.clone()
        },
        loop_note.as_deref(),
    );

    loop_result
}
