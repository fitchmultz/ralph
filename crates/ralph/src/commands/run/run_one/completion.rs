//! Run completion handling.
//!
//! Purpose:
//! - Run completion handling.
//!
//! Responsibilities:
//! - Handle run completion (success or failure).
//! - Record execution history for CLI runs.
//! - Send failure notifications.
//!
//! Not handled here:
//! - Context preparation (see context.rs).
//! - Phase execution (see phase_execution.rs).
//! - Webhook notifications (see webhooks.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Execution timings are only recorded for non-parallel worker runs.
//! - Notifications are sent on failure regardless of success.

use std::cell::RefCell;

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::Task;
use anyhow::Result;

use super::RunOutcome;
use crate::commands::run::{
    execution_history_cli, execution_timings::RunExecutionTimings, phases::PostRunMode,
};

/// Handle run completion (success or failure).
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_run_completion(
    exec_result: Result<()>,
    resolved: &config::Resolved,
    task: &Task,
    task_id: &str,
    phases: u8,
    post_run_mode: PostRunMode,
    execution_timings: Option<RefCell<RunExecutionTimings>>,
    agent_overrides: &AgentOverrides,
) -> Result<RunOutcome> {
    match exec_result {
        Ok(()) => {
            log::info!("Task {task_id}: end");

            if post_run_mode != PostRunMode::ParallelWorker
                && let Some(timings) = execution_timings
            {
                match execution_history_cli::try_record_execution_history_for_cli_run(
                    &resolved.repo_root,
                    &resolved.done_path,
                    task_id,
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

            Ok(RunOutcome::Ran {
                task_id: task_id.to_string(),
            })
        }
        Err(err) => {
            log::error!("Task {task_id}: error");

            let notify_config = crate::notification::build_notification_config(
                &resolved.config.agent.notification,
                &crate::notification::NotificationOverrides {
                    notify_on_complete: agent_overrides.notify_on_complete,
                    notify_on_fail: agent_overrides.notify_on_fail,
                    notify_sound: agent_overrides.notify_sound,
                },
            );
            let error_summary = format!("{:#}", err);
            crate::notification::notify_task_failed(
                task_id,
                &task.title,
                &error_summary,
                &notify_config,
            );

            Err(err)
        }
    }
}
