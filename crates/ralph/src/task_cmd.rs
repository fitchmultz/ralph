use crate::contracts::{Model, ProjectType, ReasoningEffort, Runner};
use crate::{config, gitutil, prompts, queue, runner, runutil, timeutil};
use anyhow::{bail, Context, Result};
use std::io::Read;

// TaskBuildOptions controls runner-driven task creation via .ralph/prompts/task_builder.md.
pub struct TaskBuildOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub force: bool,
}

// read_request_from_args_or_stdin joins any positional args, otherwise reads stdin.
pub fn read_request_from_args_or_stdin(args: &[String]) -> Result<String> {
    if !args.is_empty() {
        let joined = args.join(" ");
        let trimmed = joined.trim();
        if trimmed.is_empty() {
            bail!("Missing request: task build requires a request description. Pass arguments or pipe input to the command.");
        }
        return Ok(trimmed.to_string());
    }

    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("read stdin")?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        bail!("Missing request: task build requires a request description (pass arguments or pipe input to the command).");
    }
    Ok(trimmed.to_string())
}

pub fn build_task(resolved: &config::Resolved, opts: TaskBuildOptions) -> Result<()> {
    // Enforce the "repo is clean before any agent run" assumption.
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        &[".ralph/queue.yaml", ".ralph/done.yaml"],
    )?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task build", opts.force)?;

    if opts.request.trim().is_empty() {
        bail!("Missing request: task build requires a request description. Provide a non-empty request.");
    }

    let (before, repaired_before) =
        queue::load_queue_with_repair(&resolved.queue_path, &resolved.id_prefix, resolved.id_width)
            .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;
    if repaired_before {
        log::warn!(
            "Repaired queue YAML format issues in {}",
            resolved.queue_path.display()
        );
    }
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
    queue::validate_queue_set(&before, done_ref, &resolved.id_prefix, resolved.id_width)
        .context("validate queue set before task build")?;
    let before_ids = queue::task_id_set(&before);

    let template = prompts::load_task_builder_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt = prompts::render_task_builder_prompt(
        &template,
        &opts.request,
        &opts.hint_tags,
        &opts.hint_scope,
        project_type,
    )?;

    let bins = runner::resolve_binaries(&resolved.config.agent);
    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: opts.runner,
            bins,
            model: opts.model,
            reasoning_effort: opts.reasoning_effort,
            prompt: &prompt,
            timeout: None,
        },
        runutil::RunnerErrorMessages {
            log_label: "task builder",
            interrupted_msg: "Task builder interrupted: the agent run was canceled. Uncommitted changes were reverted to maintain a clean repo state.",
            timeout_msg: "Task builder timed out: the agent run exceeded the time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Task builder terminated: the agent was stopped by a signal. Uncommitted changes were reverted. Rerunning the command is recommended.",
            non_zero_msg: |code| {
                format!(
                    "Task builder failed: the agent exited with a non-zero code ({code}). Uncommitted changes were reverted. Rerunning the command is recommended after investigating the cause."
                )
            },
            other_msg: |err| {
                format!(
                    "Task builder failed: the agent could not be started or encountered an error. Uncommitted changes were reverted. Error: {:#}",
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
    .context("validate queue set after task build")
    {
        gitutil::revert_uncommitted(&resolved.repo_root)?;
        return Err(err);
    }

    let added = queue::added_tasks(&before_ids, &after);
    if !added.is_empty() {
        let added_ids: Vec<String> = added.iter().map(|(id, _)| id.clone()).collect();
        let now = timeutil::now_utc_rfc3339_or_fallback();
        let default_request = opts.request.clone();
        queue::backfill_missing_fields(&mut after, &added_ids, &default_request, &now);
        queue::save_queue(&resolved.queue_path, &after)
            .context("save queue with backfilled fields")?;
    }
    if added.is_empty() {
        log::info!("Task builder completed. No new tasks detected.");
    } else {
        log::info!("Task builder added {} task(s):", added.len());
        for (id, title) in added.iter().take(10) {
            log::info!("- {}: {}", id, title);
        }
        if added.len() > 10 {
            log::info!("...and {} more.", added.len() - 10);
        }
    }
    Ok(())
}
