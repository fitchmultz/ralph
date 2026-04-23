//! Integration retry loop orchestration.
//!
//! Responsibilities:
//! - Drive continue-session retries for parallel integration.
//! - Persist retry artifacts when attempts fail or become blocked.
//!
//! Does not handle:
//! - Type definitions or prompt formatting internals.
//! - Queue/done or CI validation details.

use anyhow::Result;
use std::time::Duration;

use crate::commands::run::supervision::{ContinueSession, resume_continue_session};
use crate::config::Resolved;
use crate::runutil::sleep_with_cancellation;

use super::bookkeeping::finalize_bookkeeping_and_push;
use super::compliance::head_is_synced_to_remote;
use super::persistence::{
    clear_blocked_push_marker, write_blocked_push_marker, write_handoff_packet,
};
use super::prompt::{build_agent_integration_prompt, compose_block_reason};
use super::types::{IntegrationConfig, IntegrationOutcome, RemediationHandoff};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_integration_loop(
    resolved: &Resolved,
    task_id: &str,
    task_title: &str,
    config: &IntegrationConfig,
    phase_summary: &str,
    continue_session: &mut ContinueSession,
    on_resume: &mut dyn FnMut(&crate::runner::RunnerOutput, Duration) -> Result<()>,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<IntegrationOutcome> {
    let repo_root = &resolved.repo_root;
    clear_blocked_push_marker(repo_root);
    let mut previous_failure: Option<String> = None;

    for attempt_index in 0..config.max_attempts {
        let attempt = attempt_index + 1;
        log::info!(
            "Agent-owned integration attempt {}/{} for {}",
            attempt,
            config.max_attempts,
            task_id
        );

        let status_snapshot = crate::git::status_porcelain(repo_root).unwrap_or_default();
        let prompt = build_agent_integration_prompt(
            task_id,
            task_title,
            &config.target_branch,
            &resolved.queue_path,
            &resolved.done_path,
            attempt,
            config.max_attempts,
            phase_summary,
            &status_snapshot,
            config.ci_enabled,
            &config.ci_label,
            previous_failure.as_deref(),
        );

        let resumed = match resume_continue_session(resolved, continue_session, &prompt, plugins) {
            Ok(resume) => resume,
            Err(err) => {
                let reason = format!("integration continuation failed: {:#}", err);
                if attempt >= config.max_attempts {
                    if let Err(marker_err) = write_blocked_push_marker(
                        repo_root,
                        task_id,
                        &reason,
                        attempt,
                        config.max_attempts,
                    ) {
                        log::warn!("Failed to write blocked marker: {}", marker_err);
                    }
                    return Ok(IntegrationOutcome::BlockedPush { reason });
                }
                previous_failure = Some(reason);
                wait_before_retry(config, attempt_index as usize, task_id)?;
                continue;
            }
        };

        on_resume(&resumed.output, resumed.elapsed)?;

        let machine_attempt = finalize_bookkeeping_and_push(resolved, task_id, task_title, config)?;
        let compliance = machine_attempt.compliance;
        let (pushed, push_check_error) =
            match head_is_synced_to_remote(repo_root, &config.target_branch) {
                Ok(value) => (
                    machine_attempt.pushed && value,
                    machine_attempt.push_error.clone(),
                ),
                Err(err) => (false, Some(format!("push sync validation failed: {}", err))),
            };

        if compliance.all_passed() && pushed {
            log::info!(
                "Integration succeeded for {} on attempt {}/{}",
                task_id,
                attempt,
                config.max_attempts
            );
            return Ok(IntegrationOutcome::Success);
        }

        let reason = compose_block_reason(&compliance, pushed, push_check_error.as_deref());
        let mut handoff = RemediationHandoff::new(
            task_id,
            task_title,
            &config.target_branch,
            attempt,
            config.max_attempts,
        )
        .with_conflicts(compliance.conflict_files.clone())
        .with_git_status(crate::git::status_porcelain(repo_root).unwrap_or_default())
        .with_phase_summary(phase_summary.to_string())
        .with_task_intent(format!("Complete task {}: {}", task_id, task_title));

        if !compliance.ci_passed {
            handoff = handoff.with_ci_context(
                config.ci_label.clone(),
                compliance
                    .validation_error
                    .clone()
                    .unwrap_or_else(|| "CI gate validation failed".to_string()),
                1,
            );
        }

        if let Err(err) = write_handoff_packet(repo_root, task_id, attempt, &handoff) {
            log::warn!("Failed to persist remediation handoff packet: {}", err);
        }

        if attempt >= config.max_attempts {
            if let Err(marker_err) =
                write_blocked_push_marker(repo_root, task_id, &reason, attempt, config.max_attempts)
            {
                log::warn!("Failed to write blocked marker: {}", marker_err);
            }
            return Ok(IntegrationOutcome::BlockedPush { reason });
        }

        previous_failure = Some(reason);
        wait_before_retry(config, attempt_index as usize, task_id)?;
    }

    let reason = format!("integration exhausted {} attempts", config.max_attempts);
    if let Err(marker_err) = write_blocked_push_marker(
        repo_root,
        task_id,
        &reason,
        config.max_attempts,
        config.max_attempts,
    ) {
        log::warn!("Failed to write blocked marker: {}", marker_err);
    }
    Ok(IntegrationOutcome::BlockedPush { reason })
}

fn wait_before_retry(
    config: &IntegrationConfig,
    attempt_index: usize,
    task_id: &str,
) -> Result<()> {
    let delay = config.backoff_for_attempt(attempt_index);
    log::info!(
        "Integration retry backoff for {}: sleeping {}ms before next attempt",
        task_id,
        delay.as_millis()
    );
    sleep_with_cancellation(delay, None)
        .map_err(|_| anyhow::anyhow!("integration retry cancelled for {}", task_id))
}
