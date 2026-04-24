//! Task scanning command that inspects repo state and updates the queue.
//!
//! Purpose:
//! - Task scanning command that inspects repo state and updates the queue.
//!
//! Responsibilities:
//! - Validate queue state before/after scanning and persist updated tasks.
//! - Render scan prompts with repo context and dispatch runner execution.
//! - Enforce clean-repo and queue-lock safety around scan operations.
//!
//! Not handled here:
//! - CLI parsing or interactive UI wiring.
//! - Runner process implementation details or output parsing.
//! - Queue schema definitions or config persistence.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue/done files are the source of truth for task ordering and status.
//! - Runner execution requires stream-json output for parsing.
//! - Permission/approval defaults come from config unless overridden at CLI.

use crate::cli::scan::ScanMode;
use crate::commands::run::PhaseType;
use crate::contracts::{
    ClaudePermissionMode, GitRevertMode, Model, ProjectType, ReasoningEffort, Runner,
    RunnerCliOptionsPatch,
};
use crate::{config, fsutil, git, prompts, queue, runner, runutil, timeutil};
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag indicating if debug mode is enabled.
/// This is set by the CLI when `--debug` flag is used.
static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

/// Set the global debug mode flag.
pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::SeqCst);
}

/// Check if debug mode is enabled.
fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::SeqCst)
}
use anyhow::{Context, Result};

pub struct ScanOptions {
    pub focus: String,
    pub mode: ScanMode,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    pub git_revert_mode: GitRevertMode,
    /// How to handle queue locking (acquire vs already-held by caller).
    pub lock_mode: ScanLockMode,
    /// Optional output handler for streaming scan output.
    pub output_handler: Option<runner::OutputHandler>,
    /// Optional revert prompt handler for interactive UIs.
    pub revert_prompt: Option<runutil::RevertPromptHandler>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanLockMode {
    Acquire,
    Held,
}

#[derive(Debug, Clone)]
struct ScanRunnerSettings {
    runner: Runner,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    permission_mode: Option<ClaudePermissionMode>,
}

fn resolve_scan_runner_settings(
    resolved: &config::Resolved,
    opts: &ScanOptions,
) -> Result<ScanRunnerSettings> {
    let settings = runner::resolve_agent_settings(
        opts.runner_override.clone(),
        opts.model_override.clone(),
        opts.reasoning_effort_override,
        &opts.runner_cli_overrides,
        None,
        &resolved.config.agent,
    )?;

    Ok(ScanRunnerSettings {
        runner: settings.runner,
        model: settings.model,
        reasoning_effort: settings.reasoning_effort,
        runner_cli: settings.runner_cli,
        permission_mode: resolved.config.agent.claude_permission_mode,
    })
}

pub fn run_scan(resolved: &config::Resolved, opts: ScanOptions) -> Result<()> {
    // Prevents catastrophic data loss if scan fails and reverts uncommitted changes.
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    let _queue_lock = match opts.lock_mode {
        ScanLockMode::Acquire => Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "scan",
            opts.force,
        )?),
        ScanLockMode::Held => None,
    };

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;
    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    match queue::validate_queue_set(
        &before,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before scan")
    {
        Ok(warnings) => {
            queue::log_warnings(&warnings);
        }
        Err(err) => {
            let preface = format!("Scan validation failed before run.\n{err:#}");
            let outcome = runutil::apply_git_revert_mode_with_context(
                &resolved.repo_root,
                opts.git_revert_mode,
                runutil::RevertPromptContext::new("Scan validation failure (pre-run)", false)
                    .with_preface(preface),
                opts.revert_prompt.as_ref(),
            )?;
            return Err(err).context(runutil::format_revert_failure_message(
                "Scan validation failed before run.",
                outcome,
            ));
        }
    }
    let before_ids = queue::task_id_set(&before);

    let scan_version = resolved
        .config
        .agent
        .scan_prompt_version
        .unwrap_or_default();
    let template = prompts::load_scan_prompt(&resolved.repo_root, scan_version, opts.mode)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut prompt = prompts::render_scan_prompt(
        &template,
        &opts.focus,
        opts.mode,
        scan_version,
        project_type,
        &resolved.config,
    )?;

    prompt = prompts::wrap_with_repoprompt_requirement(&prompt, opts.repoprompt_tool_injection);
    prompt = prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    let settings = resolve_scan_runner_settings(resolved, &opts)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);
    // Two-pass mode disabled for scan (only generates findings, should not implement)

    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    let output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            settings: runutil::RunnerSettings {
                repo_root: &resolved.repo_root,
                runner_kind: settings.runner,
                bins,
                model: settings.model,
                reasoning_effort: settings.reasoning_effort,
                runner_cli: settings.runner_cli,
                timeout: None,
                permission_mode: settings.permission_mode,
                output_handler: opts.output_handler.clone(),
                output_stream: if opts.output_handler.is_some() {
                    runner::OutputStream::HandlerOnly
                } else {
                    runner::OutputStream::Terminal
                },
            },
            execution: runutil::RunnerExecutionContext {
                prompt: &prompt,
                phase_type: PhaseType::SinglePhase,
                session_id: None,
            },
            failure: runutil::RunnerFailureHandling {
                revert_on_error: true,
                git_revert_mode: opts.git_revert_mode,
                revert_prompt: opts.revert_prompt.clone(),
            },
            retry: runutil::RunnerRetryState {
                policy: retry_policy,
            },
        },
        runutil::RunnerErrorMessages {
            log_label: "scan runner",
            interrupted_msg: "Scan runner interrupted: the agent run was canceled.",
            timeout_msg: "Scan runner timed out: the agent run exceeded the time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Scan runner terminated: the agent was stopped by a signal. Rerunning the command is recommended.",
            non_zero_msg: |code| {
                format!(
                    "Scan runner failed: the agent exited with a non-zero code ({code}). Rerunning the command is recommended after investigating the cause."
                )
            },
            other_msg: |err| {
                format!(
                    "Scan runner failed: the agent could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    let mut after = match queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok(queue) => queue,
        Err(err) => {
            let mut safeguard_msg = String::new();
            match fsutil::safeguard_text_dump_redacted("scan_error", &output.stdout) {
                Ok(path) => {
                    let dump_type = if is_debug_mode() { "raw" } else { "redacted" };
                    safeguard_msg = format!("\n({dump_type} stdout saved to {})", path.display());
                }
                Err(e) => {
                    log::warn!("failed to save safeguard dump: {}", e);
                }
            }
            let context = format!(
                "{}{}",
                "Scan failed to reload queue after runner output.", safeguard_msg
            );
            let preface = format!("{context}\n{err:#}");
            let outcome = runutil::apply_git_revert_mode_with_context(
                &resolved.repo_root,
                opts.git_revert_mode,
                runutil::RevertPromptContext::new("Scan queue read failure", false)
                    .with_preface(preface),
                opts.revert_prompt.as_ref(),
            )?;
            return Err(err).context(runutil::format_revert_failure_message(&context, outcome));
        }
    };

    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };
    match queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after scan")
    {
        Ok(warnings) => {
            queue::log_warnings(&warnings);
        }
        Err(err) => {
            let mut safeguard_msg = String::new();
            match fsutil::safeguard_text_dump_redacted("scan_validation_error", &output.stdout) {
                Ok(path) => {
                    let dump_type = if is_debug_mode() { "raw" } else { "redacted" };
                    safeguard_msg = format!("\n({dump_type} stdout saved to {})", path.display());
                }
                Err(e) => {
                    log::warn!("failed to save safeguard dump: {}", e);
                }
            }
            let context = format!("{}{}", "Scan validation failed after run.", safeguard_msg);
            let preface = format!("{context}\n{err:#}");
            let outcome = runutil::apply_git_revert_mode_with_context(
                &resolved.repo_root,
                opts.git_revert_mode,
                runutil::RevertPromptContext::new("Scan validation failure (post-run)", false)
                    .with_preface(preface),
                opts.revert_prompt.as_ref(),
            )?;
            return Err(err).context(runutil::format_revert_failure_message(&context, outcome));
        }
    }

    let added = queue::added_tasks(&before_ids, &after);
    if !added.is_empty() {
        let added_ids: Vec<String> = added.iter().map(|(id, _)| id.clone()).collect();
        let now = timeutil::now_utc_rfc3339_or_fallback();
        let default_request = format!("scan: {}", opts.focus);
        queue::backfill_missing_fields(&mut after, &added_ids, &default_request, &now);
        queue::save_queue(&resolved.queue_path, &after)
            .context("save queue with backfilled fields")?;
    }
    if added.is_empty() {
        log::info!("Scan completed. No new tasks detected.");
    } else {
        log::info!("Scan added {} task(s):", added.len());
        for (id, title) in added.iter().take(15) {
            log::info!("- {}: {}", id, title);
        }
        if added.len() > 15 {
            log::info!("...and {} more.", added.len() - 15);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
