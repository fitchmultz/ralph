//! Task scanning command that inspects repo state and updates the queue.

use crate::contracts::{
    ClaudePermissionMode, GitRevertMode, Model, ProjectType, ReasoningEffort, Runner,
};
use crate::{config, fsutil, gitutil, prompts, queue, runner, runutil, timeutil};
use anyhow::{Context, Result};

pub struct ScanOptions {
    pub focus: String,
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub force: bool,
    pub repoprompt_required: bool,
    pub git_revert_mode: GitRevertMode,
}

pub fn run_scan(resolved: &config::Resolved, opts: ScanOptions) -> Result<()> {
    // Prevents catastrophic data loss if scan fails and reverts uncommitted changes.
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        &[".ralph/queue.json", ".ralph/done.json"],
    )?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "scan", opts.force)?;

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;
    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    if let Err(err) =
        queue::validate_queue_set(&before, done_ref, &resolved.id_prefix, resolved.id_width)
            .context("validate queue set before scan")
    {
        let outcome = runutil::apply_git_revert_mode(
            &resolved.repo_root,
            opts.git_revert_mode,
            "Scan validation failure (pre-run)",
        )?;
        return Err(err).context(runutil::format_revert_failure_message(
            "Scan validation failed before run.",
            outcome,
        ));
    }
    let before_ids = queue::task_id_set(&before);

    let template = prompts::load_scan_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut prompt =
        prompts::render_scan_prompt(&template, &opts.focus, project_type, &resolved.config)?;

    prompt = prompts::wrap_with_repoprompt_requirement(&prompt, opts.repoprompt_required);

    let bins = runner::resolve_binaries(&resolved.config.agent);
    // Two-pass mode disabled for scan (only generates findings, should not implement)
    // Force BypassPermissions for scan (needs tool access for exploration)
    let permission_mode = Some(ClaudePermissionMode::BypassPermissions);

    let output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: opts.runner,
            bins,
            model: opts.model,
            reasoning_effort: opts.reasoning_effort,
            prompt: &prompt,
            timeout: None,
            permission_mode,
            revert_on_error: true,
            git_revert_mode: opts.git_revert_mode,
            output_handler: None,
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
            match fsutil::safeguard_text_dump("scan_error", &output.stdout) {
                Ok(path) => {
                    safeguard_msg = format!("\n(raw stdout saved to {})", path.display());
                }
                Err(e) => {
                    log::warn!("failed to save safeguard dump: {}", e);
                }
            }
            let outcome = runutil::apply_git_revert_mode(
                &resolved.repo_root,
                opts.git_revert_mode,
                "Scan queue read failure",
            )?;
            let context = format!(
                "{}{}",
                "Scan failed to reload queue after runner output.", safeguard_msg
            );
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
    if let Err(err) = queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .context("validate queue set after scan")
    {
        let mut safeguard_msg = String::new();
        match fsutil::safeguard_text_dump("scan_validation_error", &output.stdout) {
            Ok(path) => {
                safeguard_msg = format!("\n(raw stdout saved to {})", path.display());
            }
            Err(e) => {
                log::warn!("failed to save safeguard dump: {}", e);
            }
        }
        let outcome = runutil::apply_git_revert_mode(
            &resolved.repo_root,
            opts.git_revert_mode,
            "Scan validation failure (post-run)",
        )?;
        let context = format!("{}{}", "Scan validation failed after run.", safeguard_msg);
        return Err(err).context(runutil::format_revert_failure_message(&context, outcome));
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
