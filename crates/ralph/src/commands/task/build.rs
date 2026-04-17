//! Task building functionality for creating new tasks via runner invocation.
//!
//! Responsibilities:
//! - Build tasks using AI runners via .ralph/prompts/task_builder.md.
//! - Apply template hints and target contexts when specified.
//! - Validate queue state before and after runner execution.
//! - Position new tasks intelligently in the queue.
//! - Backfill missing task fields (request, timestamps) after creation.
//!
//! Not handled here:
//! - Task updating (see update/mod.rs).
//! - Refactor task generation (see refactor.rs).
//! - CLI argument parsing or command routing.
//! - Direct queue file manipulation outside of runner-driven changes.
//!
//! Invariants/assumptions:
//! - Queue file is the source of truth for task ordering.
//! - Runner execution produces valid task JSON output.
//! - Template loading and merging happens before prompt rendering.
//! - Lock acquisition is optional (controlled by acquire_lock parameter).

use super::{TaskBuildOptions, resolve_task_build_settings};
use crate::commands::run::PhaseType;
use crate::contracts::ProjectType;
use crate::{config, prompts, queue, runner, runutil, timeutil};
use anyhow::{Context, Result, bail};

pub fn build_task(resolved: &config::Resolved, opts: TaskBuildOptions) -> Result<()> {
    build_task_impl(resolved, opts, true)
}

pub fn build_task_without_lock(resolved: &config::Resolved, opts: TaskBuildOptions) -> Result<()> {
    build_task_impl(resolved, opts, false)
}

fn build_task_impl(
    resolved: &config::Resolved,
    mut opts: TaskBuildOptions,
    acquire_lock: bool,
) -> Result<()> {
    let _queue_lock = if acquire_lock {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "task",
            opts.force,
        )?)
    } else {
        None
    };

    if opts.request.trim().is_empty() {
        bail!("Missing request: task requires a request description. Provide a non-empty request.");
    }

    // Apply template if specified
    let mut template_context = String::new();
    if let Some(template_name) = opts.template_hint.clone() {
        // Use context-aware loading with validation
        let load_result = crate::template::load_template_with_context(
            &template_name,
            &resolved.repo_root,
            opts.template_target.as_deref(),
            opts.strict_templates,
        );

        match load_result {
            Ok(loaded) => {
                // Log any warnings from template validation
                for warning in &loaded.warnings {
                    log::warn!("Template '{}': {}", template_name, warning);
                }

                crate::template::merge_template_with_options(&loaded.task, &mut opts);
                template_context = crate::template::format_template_context(&loaded.task);
                log::info!("Using template '{}' for task creation", template_name);
            }
            Err(e) => {
                if opts.strict_templates {
                    bail!(
                        "Template '{}' failed strict validation: {}",
                        template_name,
                        e
                    );
                } else {
                    log::warn!("Failed to load template '{}': {}", template_name, e);
                }
            }
        }
    }

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    // Compute insertion strategy from pre-run queue state
    let insert_index = queue::suggest_new_task_insert_index(&before);

    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &before,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before task")?;
    let before_ids = queue::task_id_set(&before);

    let template = prompts::load_task_builder_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut prompt = prompts::render_task_builder_prompt(
        &template,
        &opts.request,
        &opts.hint_tags,
        &opts.hint_scope,
        project_type,
        &resolved.config,
    )?;

    // Append template context to prompt if available
    if !template_context.is_empty() {
        prompt.push_str("\n\n--- Template Suggestions ---\n");
        prompt.push_str(&template_context);
    }

    prompt = prompts::wrap_with_repoprompt_requirement(&prompt, opts.repoprompt_tool_injection);
    prompt = prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    let settings = resolve_task_build_settings(resolved, &opts)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);
    // Two-pass mode disabled for task (only generates task, should not implement)

    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    let _output = runutil::run_prompt_with_handling(
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
                output_handler: None,
                output_stream: runner::OutputStream::Terminal,
            },
            execution: runutil::RunnerExecutionContext {
                prompt: &prompt,
                phase_type: PhaseType::SinglePhase,
                session_id: None,
            },
            failure: runutil::RunnerFailureHandling {
                revert_on_error: false,
                git_revert_mode: resolved
                    .config
                    .agent
                    .git_revert_mode
                    .unwrap_or(crate::contracts::GitRevertMode::Ask),
                revert_prompt: None,
            },
            retry: runutil::RunnerRetryState {
                policy: retry_policy,
            },
        },
        runutil::RunnerErrorMessages {
            log_label: "task builder",
            interrupted_msg: "Task builder interrupted: the agent run was canceled.",
            timeout_msg: "Task builder timed out: the agent run exceeded the time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Task builder terminated: the agent was stopped by a signal. Review uncommitted changes before rerunning.",
            non_zero_msg: |code| {
                format!(
                    "Task builder failed: the agent exited with a non-zero code ({}). Review uncommitted changes before rerunning.",
                    code
                )
            },
            other_msg: |err| {
                format!(
                    "Task builder failed: the agent could not be started or encountered an error. Error: {:#}",
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
            return Err(err);
        }
    };

    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };
    queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after task")?;

    let added = queue::added_tasks(&before_ids, &after);
    if !added.is_empty() {
        let added_ids: Vec<String> = added.iter().map(|(id, _)| id.clone()).collect();

        // Enforce smart positioning deterministically
        queue::reposition_new_tasks(&mut after, &added_ids, insert_index);

        let now = timeutil::now_utc_rfc3339_or_fallback();
        let default_request = opts.request.clone();
        queue::backfill_missing_fields(&mut after, &added_ids, &default_request, &now);

        // Apply estimated_minutes if provided via --estimate flag
        if let Some(estimated) = opts.estimated_minutes {
            for task in &mut after.tasks {
                if added_ids.contains(&task.id) {
                    task.estimated_minutes = Some(estimated);
                }
            }
        }

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
