//! Webhook notification helpers for run-one.
//!
//! Purpose:
//! - Webhook notification helpers for run-one.
//!
//! Responsibilities:
//! - Execute phase 1 (planning) with webhook notifications.
//! - Execute implementation phases (phase 2, 3, or single) with webhook notifications.
//! - Build webhook context with run metadata.
//!
//! Not handled here:
//! - Context preparation (see context.rs).
//! - Task setup (see execution_setup.rs).
//! - Phase execution logic (see phase_execution.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Webhook notifications are best-effort and do not fail the run on error.
//! - Planning phase does not have CI gate status.

use crate::config;
use crate::runner;
use anyhow::Result;

use crate::commands::run::phases::PhaseInvocation;

/// Execute phase 1 (planning) with webhook notifications.
/// Returns the plan text on success.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_phase1_with_webhooks(
    phase_count: u8,
    task_id: &str,
    task_title: &str,
    webhook_config: &crate::contracts::WebhookConfig,
    _ci_gate_enabled: bool,
    settings: &runner::AgentSettings,
    resolved: &config::Resolved,
    invocation: &PhaseInvocation<'_>,
) -> Result<String> {
    let started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let start = std::time::Instant::now();

    let ctx = crate::webhook::WebhookContext {
        runner: Some(format!("{:?}", settings.runner).to_lowercase()),
        model: Some(settings.model.as_str().to_string()),
        phase: Some(1),
        phase_count: Some(phase_count),
        repo_root: Some(resolved.repo_root.display().to_string()),
        branch: crate::git::current_branch(&resolved.repo_root).ok(),
        commit: crate::session::get_git_head_commit(&resolved.repo_root),
        ..Default::default()
    };

    crate::webhook::notify_phase_started(
        task_id,
        task_title,
        webhook_config,
        &started_at,
        ctx.clone(),
    );

    let result = crate::commands::run::phases::execute_phase1_planning(invocation, phase_count);

    let completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let mut ctx_done = ctx;
    ctx_done.duration_ms = Some(start.elapsed().as_millis() as u64);
    ctx_done.ci_gate = Some("skipped".to_string()); // Planning phase doesn't have CI gate

    crate::webhook::notify_phase_completed(
        task_id,
        task_title,
        webhook_config,
        &completed_at,
        ctx_done,
    );

    result
}

/// Execute implementation phase (phase 2, 3, or single) with webhook notifications.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_impl_phase_with_webhooks<F>(
    phase_num: u8,
    phase_count: u8,
    task_id: &str,
    task_title: &str,
    webhook_config: &crate::contracts::WebhookConfig,
    ci_gate_enabled: bool,
    settings: &runner::AgentSettings,
    resolved: &config::Resolved,
    invocation: &PhaseInvocation<'_>,
    phase_executor: F,
) -> Result<()>
where
    F: FnOnce(&PhaseInvocation<'_>) -> Result<()>,
{
    let started_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let start = std::time::Instant::now();

    let ctx = crate::webhook::WebhookContext {
        runner: Some(format!("{:?}", settings.runner).to_lowercase()),
        model: Some(settings.model.as_str().to_string()),
        phase: Some(phase_num),
        phase_count: Some(phase_count),
        repo_root: Some(resolved.repo_root.display().to_string()),
        branch: crate::git::current_branch(&resolved.repo_root).ok(),
        commit: crate::session::get_git_head_commit(&resolved.repo_root),
        ..Default::default()
    };

    crate::webhook::notify_phase_started(
        task_id,
        task_title,
        webhook_config,
        &started_at,
        ctx.clone(),
    );

    let result = phase_executor(invocation);

    let completed_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let mut ctx_done = ctx;
    ctx_done.duration_ms = Some(start.elapsed().as_millis() as u64);

    if ci_gate_enabled {
        ctx_done.ci_gate = match &result {
            Ok(()) => Some("passed".to_string()),
            Err(err) => {
                let msg = format!("{err:#}");
                if msg.contains("CI failed:") {
                    Some("failed".to_string())
                } else {
                    None
                }
            }
        };
    } else {
        ctx_done.ci_gate = Some("skipped".to_string());
    }

    crate::webhook::notify_phase_completed(
        task_id,
        task_title,
        webhook_config,
        &completed_at,
        ctx_done,
    );

    result
}
