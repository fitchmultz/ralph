use crate::contracts::{ClaudePermissionMode, Model, ProjectType, ReasoningEffort, Runner};
use crate::{config, gitutil, prompts, queue, runner, runutil, timeutil};
use anyhow::{Context, Result};

pub struct ScanOptions {
    pub focus: String,
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub force: bool,
}

pub fn run_scan(resolved: &config::Resolved, opts: ScanOptions) -> Result<()> {
    // Prevents catastrophic data loss if scan fails and reverts uncommitted changes.
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        &[".ralph/queue.yaml", ".ralph/done.yaml"],
    )?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "scan", opts.force)?;

    let before = match queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok((queue, repaired)) => {
            if repaired {
                log::warn!(
                    "Repaired queue YAML format issues in {}",
                    resolved.queue_path.display()
                );
            }
            queue
        }
        Err(err) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            return Err(err);
        }
    };
    let (done, repaired_done) = queue::load_queue_or_default_with_repair(
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    queue::warn_if_repaired(&resolved.done_path, repaired_done);
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    if let Err(err) =
        queue::validate_queue_set(&before, done_ref, &resolved.id_prefix, resolved.id_width)
            .context("validate queue set before scan")
    {
        gitutil::revert_uncommitted(&resolved.repo_root)?;
        return Err(err);
    }
    let before_ids = queue::task_id_set(&before);

    let template = prompts::load_scan_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt = prompts::render_scan_prompt(&template, &opts.focus, project_type)?;

    let bins = runner::resolve_binaries(&resolved.config.agent);
    // Two-pass mode disabled for scan (only generates findings, should not implement)
    let two_pass_plan = false;
    // Force BypassPermissions for scan (needs tool access for exploration)
    let permission_mode = Some(ClaudePermissionMode::BypassPermissions);

    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: opts.runner,
            bins,
            model: opts.model,
            reasoning_effort: opts.reasoning_effort,
            prompt: &prompt,
            timeout: None,
            two_pass_plan,
            permission_mode,
        },
        runutil::RunnerErrorMessages {
            log_label: "scan runner",
            interrupted_msg: "Scan runner interrupted: the agent run was canceled. Uncommitted changes were reverted to maintain a clean repo state.",
            timeout_msg: "Scan runner timed out: the agent run exceeded the time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Scan runner terminated: the agent was stopped by a signal. Uncommitted changes were reverted. Rerunning the command is recommended.",
            non_zero_msg: |code| {
                format!(
                    "Scan runner failed: the agent exited with a non-zero code ({code}). Uncommitted changes were reverted. Rerunning the command is recommended after investigating the cause."
                )
            },
            other_msg: |err| {
                format!(
                    "Scan runner failed: the agent could not be started or encountered an error. Uncommitted changes were reverted. Error: {:#}",
                    err
                )
            },
        },
    )?;

    let mut after = match queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok((queue, repaired)) => {
            if repaired {
                log::warn!(
                    "Repaired queue YAML format issues in {}",
                    resolved.queue_path.display()
                );
            }
            queue
        }
        Err(err) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            return Err(err);
        }
    };

    let (done_after, repaired_done_after) = queue::load_queue_or_default_with_repair(
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    queue::warn_if_repaired(&resolved.done_path, repaired_done_after);
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
        gitutil::revert_uncommitted(&resolved.repo_root)?;
        return Err(err);
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
