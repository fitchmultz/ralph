//! Run command orchestration and supervision.
//!
//! This module owns task selection, queue bookkeeping, and post-run supervision.
//! Phase-specific prompt/runner execution lives in `run_cmd::phases`.

use crate::config;
use crate::contracts::{GitRevertMode, ProjectType, TaskStatus};
use crate::promptflow;
use crate::{gitutil, prompts, queue, runner, timeutil};
use anyhow::{bail, Context, Result};

mod logging;
mod phases;
mod selection;
mod supervision;

use selection::select_run_one_task_index;
use supervision::{find_task_status, post_run_supervise, run_make_ci};

// Preserve existing `run_cmd.rs` unit tests which call `apply_phase3_completion_signal` directly.
#[allow(unused_imports)]
pub(crate) use phases::apply_phase3_completion_signal;

pub use crate::agent::AgentOverrides;

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

    let initial_todo_count = queue_file
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Todo)
        .count() as u32;

    if initial_todo_count == 0 {
        // Keep this phrase stable; some tests look for it.
        log::info!("No todo tasks found.");
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
        Some(task_id),
        output_handler,
    )
    .map(|_| ())
}

pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
) -> Result<RunOutcome> {
    run_one_impl(resolved, agent_overrides, force, None, None)
}

fn run_one_impl(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    target_task_id: Option<&str>,
    output_handler: Option<runner::OutputHandler>,
) -> Result<RunOutcome> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?;
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

    let rp_required = agent_overrides
        .repoprompt_required
        .or(resolved.config.agent.require_repoprompt)
        .unwrap_or(false);

    let git_revert_mode = agent_overrides
        .git_revert_mode
        .or(resolved.config.agent.git_revert_mode)
        .unwrap_or(GitRevertMode::Ask);

    let policy = promptflow::PromptPolicy {
        require_repoprompt: rp_required,
    };

    // --- Task Selection ---
    // Prefer resuming a `doing` task (crash recovery), otherwise take first runnable `todo`.
    let task_idx = match select_run_one_task_index(&queue_file, done_ref, target_task_id)? {
        Some(idx) => idx,
        None => {
            let has_todo = queue_file
                .tasks
                .iter()
                .any(|t| t.status == TaskStatus::Todo);
            if has_todo {
                log::info!("All todo tasks are blocked by unmet dependencies.");
            } else {
                log::info!("No todo tasks found.");
            }
            return Ok(RunOutcome::NoTodo);
        }
    };

    let task = queue_file.tasks[task_idx].clone();
    let task_id = task.id.trim().to_string();

    log::info!("RunOne: selected {task_id} (phases={phases})");

    // Require clean repo
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        &[".ralph/queue.json", ".ralph/done.json"],
    )?;

    // Mark the task as doing before running the agent.
    mark_task_doing(resolved, &task_id)?;

    // Resolve runner settings
    let settings = resolve_run_agent_settings(resolved, &task, agent_overrides)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    log::info!(
        "Task {task_id}: start (runner={runner:?}, model={model})",
        runner = settings.runner,
        model = settings.model.as_str()
    );

    let exec_result: Result<()> = (|| {
        // --- Prompt Construction ---
        let template = prompts::load_worker_prompt(&resolved.repo_root)?;
        let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
        let mut base_prompt =
            prompts::render_worker_prompt(&template, project_type, &resolved.config)?;

        // Inject an authoritative task context block to prevent the agent from selecting
        // a different task (e.g., "first todo" or "lowest ID") after Ralph marks the
        // selected task as `doing`.
        let task_context = task_context_for_prompt(&task)?;
        base_prompt = format!("{task_context}\n\n---\n\n{base_prompt}");

        let invocation = phases::PhaseInvocation {
            resolved,
            settings: &settings,
            bins,
            task_id: &task_id,
            base_prompt: &base_prompt,
            policy: &policy,
            output_handler: output_handler.clone(),
            project_type,
            git_revert_mode,
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
- Ralph has already set this task to `doing`. Do NOT change task status manually.

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
