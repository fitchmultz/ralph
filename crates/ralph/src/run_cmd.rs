//! Run command orchestration and supervision.
//!
//! This module owns task selection, queue bookkeeping, and post-run supervision.
//! Phase-specific prompt/runner execution lives in `run_cmd::phases`.

use crate::config;
use crate::contracts::{AgentConfig, GitRevertMode, ProjectType, ReasoningEffort, TaskStatus};
use crate::promptflow;
use crate::{gitutil, prompts, queue, runner, runutil, timeutil};
use anyhow::{bail, Context, Result};

mod logging;
mod phases;
mod selection;
mod supervision;

use selection::select_run_one_task_index;
use supervision::{find_task_status, post_run_supervise};

// Preserve existing `run_cmd.rs` unit tests which call `apply_phase3_completion_signal` directly.
#[allow(unused_imports)]
pub(crate) use phases::apply_phase3_completion_signal;

pub use crate::agent::AgentOverrides;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueueLockMode {
    Acquire,
    Held,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IterationSettings {
    count: u8,
    followup_reasoning_effort: Option<ReasoningEffort>,
}

pub enum RunOutcome {
    NoTodo,
    Ran { task_id: String },
}

pub struct RunLoopOptions {
    /// 0 means "no limit"
    pub max_tasks: u32,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
}

pub fn run_loop(resolved: &config::Resolved, opts: RunLoopOptions) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let initial_todo_count = queue_file
        .tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
        })
        .count() as u32;

    if initial_todo_count == 0 {
        // Keep this phrase stable; some tests look for it.
        if include_draft {
            log::info!("No todo or draft tasks found.");
        } else {
            log::info!("No todo tasks found.");
        }
        return Ok(());
    }

    let label = format!(
        "RunLoop (todo={initial_todo_count}, max_tasks={})",
        opts.max_tasks
    );

    logging::with_scope(&label, || {
        let mut completed = 0u32;

        loop {
            if opts.max_tasks != 0 && completed >= opts.max_tasks {
                log::info!("RunLoop: end (reached max task limit: {completed})");
                return Ok(());
            }

            match run_one(resolved, &opts.agent_overrides, opts.force)? {
                RunOutcome::NoTodo => {
                    log::info!("RunLoop: end (no more todo tasks remaining)");
                    return Ok(());
                }
                RunOutcome::Ran { task_id } => {
                    completed += 1;
                    log::info!(
                        "RunLoop: task-complete {task_id} ({completed}/{initial_todo_count})"
                    );
                }
            }
        }
    })
}

pub fn run_one_with_id(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    // Re-use run_one logic but target specific ID.
    // However, run_one finds the task based on status logic (Todo vs Doing).
    // run_one_with_id implies we selected a specific task.
    // We should probably adapt run_one_logic to take an optional task_id.
    // For now, let's just delegate to a shared implementation.
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        Some(task_id),
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

/// Run a specific task when the queue lock is already held by the caller.
pub fn run_one_with_id_locked(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Held,
        Some(task_id),
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
) -> Result<RunOutcome> {
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        None,
        None,
        None,
    )
}

fn run_one_impl(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
    target_task_id: Option<&str>,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<RunOutcome> {
    let _queue_lock = match lock_mode {
        QueueLockMode::Acquire => Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "run one",
            force,
        )?),
        QueueLockMode::Held => None,
    };
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )?;

    // Determine execution shape and policy
    let phases: u8 = agent_overrides
        .phases
        .or(resolved.config.agent.phases)
        .unwrap_or(2);
    if !(1..=3).contains(&phases) {
        bail!("Invalid phases value: {} (expected 1, 2, or 3)", phases);
    }

    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(agent_overrides, resolved);

    let git_revert_mode = agent_overrides
        .git_revert_mode
        .or(resolved.config.agent.git_revert_mode)
        .unwrap_or(GitRevertMode::Ask);

    let git_commit_push_enabled = agent_overrides
        .git_commit_push_enabled
        .or(resolved.config.agent.git_commit_push_enabled)
        .unwrap_or(true);

    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: repoprompt_flags.plan_required,
        repoprompt_tool_injection: repoprompt_flags.tool_injection,
    };

    // --- Task Selection ---
    // Prefer resuming a `doing` task (crash recovery), otherwise take first runnable `todo`.
    let include_draft = agent_overrides.include_draft.unwrap_or(false);
    let task_idx =
        match select_run_one_task_index(&queue_file, done_ref, target_task_id, include_draft)? {
            Some(idx) => idx,
            None => {
                let has_runnable = queue_file.tasks.iter().any(|t| {
                    t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
                });
                if has_runnable {
                    log::info!("All runnable tasks are blocked by unmet dependencies.");
                } else if include_draft {
                    log::info!("No todo or draft tasks found.");
                } else {
                    log::info!("No todo tasks found.");
                }
                return Ok(RunOutcome::NoTodo);
            }
        };

    let task = queue_file.tasks[task_idx].clone();
    let task_id = task.id.trim().to_string();

    let iteration_settings = resolve_iteration_settings(&task, &resolved.config.agent)?;
    log::info!(
        "RunOne: selected {task_id} (phases={phases}, iterations={})",
        iteration_settings.count
    );

    // Require clean repo before the first iteration starts.
    let preexisting_dirty_allowed = gitutil::repo_dirty_only_allowed_paths(
        &resolved.repo_root,
        gitutil::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        gitutil::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    // Mark the task as doing before running the agent.
    mark_task_doing(resolved, &task_id)?;

    // Resolve runner settings
    let base_settings = resolve_run_agent_settings(resolved, &task, agent_overrides)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    log::info!(
        "Task {task_id}: start (runner={runner:?}, model={model})",
        runner = base_settings.runner,
        model = base_settings.model.as_str()
    );

    let exec_result: Result<()> = (|| {
        // --- Prompt Construction ---
        let template = prompts::load_worker_prompt(&resolved.repo_root)?;
        let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
        let mut base_prompt =
            prompts::render_worker_prompt(&template, &task_id, project_type, &resolved.config)?;

        // Inject an authoritative task context block to prevent the agent from selecting
        // a different task (e.g., "first todo" or "lowest ID") after Ralph marks the
        // selected task as `doing`.
        let task_context = task_context_for_prompt(&task)?;
        base_prompt = format!("{task_context}\n\n---\n\n{base_prompt}");

        let output_stream = if output_handler.is_some() {
            runner::OutputStream::HandlerOnly
        } else {
            runner::OutputStream::Terminal
        };

        for iteration_index in 1..=iteration_settings.count {
            let is_followup = iteration_index > 1;
            let is_final_iteration = iteration_index == iteration_settings.count;

            log::info!(
                "Task {task_id}: iteration {iteration_index}/{}",
                iteration_settings.count
            );

            let settings = apply_followup_reasoning_effort(
                &base_settings,
                iteration_settings.followup_reasoning_effort,
                is_followup,
            );

            let iteration_context = if is_followup {
                prompts::ITERATION_CONTEXT_REFINEMENT
            } else {
                ""
            };
            let iteration_completion_block = if is_final_iteration {
                ""
            } else {
                prompts::ITERATION_COMPLETION_BLOCK
            };
            let phase3_completion_guidance = if is_final_iteration {
                prompts::PHASE3_COMPLETION_GUIDANCE_FINAL
            } else {
                prompts::PHASE3_COMPLETION_GUIDANCE_NONFINAL
            };

            let invocation = phases::PhaseInvocation {
                resolved,
                settings: &settings,
                bins,
                task_id: &task_id,
                base_prompt: &base_prompt,
                policy: &policy,
                output_handler: output_handler.clone(),
                output_stream,
                project_type,
                git_revert_mode,
                git_commit_push_enabled,
                revert_prompt: revert_prompt.clone(),
                iteration_context,
                iteration_completion_block,
                phase3_completion_guidance,
                is_final_iteration,
                allow_dirty_repo: is_followup || preexisting_dirty_allowed,
            };

            match phases {
                2 => {
                    let plan_text = phases::execute_phase1_planning(&invocation, 2)?;
                    phases::execute_phase2_implementation(&invocation, 2, &plan_text)?;
                }
                3 => {
                    let plan_text = phases::execute_phase1_planning(&invocation, 3)?;
                    phases::execute_phase2_implementation(&invocation, 3, &plan_text)?;
                    phases::execute_phase3_review(&invocation)?;
                }
                1 => {
                    phases::execute_single_phase(&invocation)?;
                }
                _ => unreachable!("phases must be validated to 1..=3"),
            }
        }

        Ok(())
    })();

    match exec_result {
        Ok(()) => {
            log::info!("Task {task_id}: end");
            Ok(RunOutcome::Ran { task_id })
        }
        Err(err) => {
            // Keep task-level error concise; phase scopes will log detailed boundaries.
            log::error!("Task {task_id}: error");
            Err(err)
        }
    }
}

fn resolve_run_agent_settings(
    resolved: &config::Resolved,
    task: &crate::contracts::Task,
    overrides: &AgentOverrides,
) -> Result<runner::AgentSettings> {
    runner::resolve_agent_settings(
        overrides.runner,
        overrides.model.clone(),
        overrides.reasoning_effort,
        task.agent.as_ref(),
        &resolved.config.agent,
    )
}

fn resolve_iteration_settings(
    task: &crate::contracts::Task,
    config_agent: &AgentConfig,
) -> Result<IterationSettings> {
    let count = task
        .agent
        .as_ref()
        .and_then(|agent| agent.iterations)
        .or(config_agent.iterations)
        .unwrap_or(1);

    if count == 0 {
        bail!(
            "Invalid iterations for task {}: iterations must be >= 1.",
            task.id.trim()
        );
    }

    let followup_reasoning_effort = task
        .agent
        .as_ref()
        .and_then(|agent| agent.followup_reasoning_effort)
        .or(config_agent.followup_reasoning_effort);

    Ok(IterationSettings {
        count,
        followup_reasoning_effort,
    })
}

fn apply_followup_reasoning_effort(
    base_settings: &runner::AgentSettings,
    followup_reasoning_effort: Option<ReasoningEffort>,
    is_followup: bool,
) -> runner::AgentSettings {
    if !is_followup {
        return base_settings.clone();
    }

    let mut settings = base_settings.clone();
    if let Some(effort) = followup_reasoning_effort {
        if settings.runner == crate::contracts::Runner::Codex {
            settings.reasoning_effort = Some(effort);
        } else {
            log::warn!(
                "Follow-up reasoning_effort configured, but runner {:?} does not support it; ignoring override.",
                settings.runner
            );
        }
    }
    settings
}

fn task_context_for_prompt(task: &crate::contracts::Task) -> Result<String> {
    let id = task.id.trim();
    let title = task.title.trim();
    let rendered =
        serde_json::to_string_pretty(task).context("serialize task JSON for prompt context")?;

    Ok(format!(
        r#"# CURRENT TASK (AUTHORITATIVE)

You MUST work on this exact task and no other task.
- Do NOT switch tasks based on queue order, "first todo", or "lowest ID".
- Ignore `.ralph/done.json` except as historical reference if explicitly needed.
- Do NOT change task status manually.

Task ID: {id}
Title: {title}

Raw task JSON (source of truth):
```json
{rendered}
```
"#,
    ))
}

fn mark_task_doing(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;
    queue::set_status(&mut queue_file, task_id, TaskStatus::Doing, &now, None)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;
    Ok(())
}

#[cfg(test)]
mod tests;
