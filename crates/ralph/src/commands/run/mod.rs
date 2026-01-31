//! Run command orchestration and supervision.
//!
//! Responsibilities:
//! - Select runnable tasks and orchestrate run loop/one workflows.
//! - Coordinate queue bookkeeping, pre-run updates, and post-run supervision.
//! - Delegate phase-specific execution to `crate::commands::run::phases`.
//!
//! Not handled here:
//! - CLI argument parsing or config persistence.
//! - Runner process implementation details.
//! - Prompt template rendering outside run phases.
//!
//! Invariants/assumptions:
//! - Queue ordering is authoritative for task selection.
//! - Pre-run updates and CI gates honor config defaults unless overridden.
//! - Phase runners expect stream-json output for execution.

use crate::commands::task as task_cmd;
use crate::config;
use crate::contracts::{
    AgentConfig, GitRevertMode, ProjectType, ReasoningEffort, RunnerCliOptionsPatch, TaskStatus,
};
use crate::promptflow;
use crate::session::{self, SessionValidationResult};
use crate::{git, prompts, queue, runner, runutil, timeutil};
use anyhow::{bail, Context, Result};

mod logging;
mod phases;
mod selection;
mod supervision;

use selection::select_run_one_task_index;
use supervision::{find_task_status, post_run_supervise};

// Preserve existing `commands::run` unit tests which call phase 3 helpers directly.
#[allow(unused_imports)]
pub(crate) use phases::{apply_phase3_completion_signal, finalize_phase3_if_done};

// Re-export PhaseType for use by runner module.
pub(crate) use phases::PhaseType;

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
    /// Auto-resume without prompting (for --resume flag)
    pub auto_resume: bool,
    /// Starting completed count (for resumed sessions)
    pub starting_completed: u32,
}

pub fn run_loop(resolved: &config::Resolved, opts: RunLoopOptions) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let queue_file = queue::load_queue(&resolved.queue_path)?;

    // Handle session recovery
    let (resume_task_id, completed_count) = match session::check_session(
        &cache_dir,
        &queue_file,
        Some(session::DEFAULT_SESSION_TIMEOUT_HOURS),
    )? {
        SessionValidationResult::NoSession => (None, opts.starting_completed),
        SessionValidationResult::Valid(session) => {
            if opts.auto_resume {
                log::info!("Auto-resuming session for task {}", session.task_id);
                (Some(session.task_id), session.tasks_completed_in_loop)
            } else {
                match session::prompt_session_recovery(&session)? {
                    true => (Some(session.task_id), session.tasks_completed_in_loop),
                    false => {
                        session::clear_session(&cache_dir)?;
                        (None, opts.starting_completed)
                    }
                }
            }
        }
        SessionValidationResult::Stale { reason } => {
            log::info!("Stale session cleared: {}", reason);
            session::clear_session(&cache_dir)?;
            (None, opts.starting_completed)
        }
        SessionValidationResult::Timeout { hours } => {
            let session = session::load_session(&cache_dir)?.unwrap();
            match session::prompt_session_recovery_timeout(&session, hours)? {
                true => (Some(session.task_id), session.tasks_completed_in_loop),
                false => {
                    session::clear_session(&cache_dir)?;
                    (None, opts.starting_completed)
                }
            }
        }
    };

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let initial_todo_count = queue_file
        .tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
        })
        .count() as u32;

    if initial_todo_count == 0 && resume_task_id.is_none() {
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

    // Track loop completion stats for notification
    let mut tasks_attempted: usize = 0;
    let mut tasks_succeeded: usize = 0;
    let mut tasks_failed: usize = 0;

    // Track consecutive failures to prevent infinite loops
    let mut consecutive_failures: u32 = 0;
    const MAX_CONSECUTIVE_FAILURES: u32 = 50;

    // Use a mutable reference to allow modification inside the closure
    let mut completed = completed_count;

    let result = logging::with_scope(&label, || {
        loop {
            if opts.max_tasks != 0 && completed >= opts.max_tasks {
                log::info!("RunLoop: end (reached max task limit: {completed})");
                return Ok(());
            }

            match run_one(
                resolved,
                &opts.agent_overrides,
                opts.force,
                resume_task_id.as_deref(),
            ) {
                Ok(RunOutcome::NoTodo) => {
                    log::info!("RunLoop: end (no more todo tasks remaining)");
                    return Ok(());
                }
                Ok(RunOutcome::Ran { task_id: _ }) => {
                    completed += 1;
                    tasks_attempted += 1;
                    tasks_succeeded += 1;
                    consecutive_failures = 0; // Reset on success
                    log::info!("RunLoop: task-complete ({completed}/{initial_todo_count})");
                }
                Err(err) => {
                    if let Some(reason) = runutil::abort_reason(&err) {
                        match reason {
                            runutil::RunAbortReason::Interrupted => {
                                log::info!("RunLoop: aborting after interrupt");
                            }
                            runutil::RunAbortReason::UserRevert => {
                                log::info!("RunLoop: aborting after user-requested revert");
                            }
                        }
                        return Err(err);
                    }
                    completed += 1;
                    tasks_attempted += 1;
                    tasks_failed += 1;
                    consecutive_failures += 1;
                    log::error!("RunLoop: task failed: {:#}", err);

                    // Safety check: prevent infinite loops from rapid consecutive failures
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        log::error!("RunLoop: aborting after {MAX_CONSECUTIVE_FAILURES} consecutive failures");
                        return Err(anyhow::anyhow!(
                            "Run loop aborted after {} consecutive task failures. \
                             This usually indicates a systemic issue (e.g., repo dirty, \
                             runner misconfiguration, or interrupt flag stuck). \
                             Check logs above for root cause.",
                            MAX_CONSECUTIVE_FAILURES
                        ));
                    }
                    // Continue with next task even if one failed
                }
            }
        }
    });

    // Send loop completion notification
    if tasks_attempted > 0 {
        let notify_config = crate::notification::NotificationConfig {
            enabled: opts
                .agent_overrides
                .notify_on_complete
                .or(resolved.config.agent.notification.enabled)
                .unwrap_or(true),
            notify_on_complete: opts
                .agent_overrides
                .notify_on_complete
                .or(resolved.config.agent.notification.notify_on_complete)
                .unwrap_or(true),
            notify_on_fail: opts
                .agent_overrides
                .notify_on_fail
                .or(resolved.config.agent.notification.notify_on_fail)
                .unwrap_or(true),
            notify_on_loop_complete: resolved
                .config
                .agent
                .notification
                .notify_on_loop_complete
                .unwrap_or(true),
            suppress_when_active: resolved
                .config
                .agent
                .notification
                .suppress_when_active
                .unwrap_or(true),
            sound_enabled: opts
                .agent_overrides
                .notify_sound
                .or(resolved.config.agent.notification.sound_enabled)
                .unwrap_or(false),
            sound_path: resolved.config.agent.notification.sound_path.clone(),
            timeout_ms: resolved
                .config
                .agent
                .notification
                .timeout_ms
                .unwrap_or(8000),
        };
        crate::notification::notify_loop_complete(
            tasks_attempted,
            tasks_succeeded,
            tasks_failed,
            &notify_config,
        );
    }

    // Clear session on successful completion
    if result.is_ok() {
        if let Err(e) = session::clear_session(&cache_dir) {
            log::warn!("Failed to clear session on loop completion: {}", e);
        }
    }

    result
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
        None,
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
        None,
        output_handler,
        revert_prompt,
    )
    .map(|_| ())
}

pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    resume_task_id: Option<&str>,
) -> Result<RunOutcome> {
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        QueueLockMode::Acquire,
        None,
        resume_task_id,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_one_impl(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
    target_task_id: Option<&str>,
    resume_task_id: Option<&str>,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<RunOutcome> {
    // Reset the Ctrl+C interrupt flag at the start of each task.
    // This prevents the infinite loop bug where the interrupted flag persists
    // across task attempts when the user cancels during pre-run updates or phases.
    // The flag must be reset before any runner invocation (including task updater).
    let ctrlc = crate::runner::ctrlc_state();
    ctrlc
        .interrupted
        .store(false, std::sync::atomic::Ordering::SeqCst);

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
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

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

    // Determine effective target: explicit target > resume_task_id > normal selection
    let effective_target = if target_task_id.is_some() {
        target_task_id
    } else if let Some(resume_id) = resume_task_id {
        // Validate the resumed task before using it
        match validate_resumed_task(&queue_file, resume_id, &resolved.repo_root) {
            Ok(()) => Some(resume_id),
            Err(e) => {
                log::info!("Session resume failed: {e}");
                None
            }
        }
    } else {
        None
    };

    let task_idx =
        match select_run_one_task_index(&queue_file, done_ref, effective_target, include_draft)? {
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

    let mut task = queue_file.tasks[task_idx].clone();
    let task_id = task.id.trim().to_string();

    let iteration_settings = resolve_iteration_settings(&task, &resolved.config.agent)?;
    log::info!(
        "RunOne: selected {task_id} (phases={phases}, iterations={})",
        iteration_settings.count
    );

    // Resolve runner settings early so the pre-run task update uses the same settings as execution.
    let base_settings = resolve_run_agent_settings(resolved, &task, agent_overrides)?;

    // Require clean repo before the first iteration starts.
    let preexisting_dirty_allowed = git::repo_dirty_only_allowed_paths(
        &resolved.repo_root,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    // Optional pre-run task update: run once per task ID, immediately before we mark the task as doing.
    let update_task_before_run = agent_overrides
        .update_task_before_run
        .or(resolved.config.agent.update_task_before_run)
        .unwrap_or(false);

    let fail_on_prerun_update_error = agent_overrides
        .fail_on_prerun_update_error
        .or(resolved.config.agent.fail_on_prerun_update_error)
        .unwrap_or(false);

    if update_task_before_run {
        log::info!("Task {task_id}: pre-run update enabled; running task updater");
        let runner_cli_overrides = RunnerCliOptionsPatch {
            output_format: Some(base_settings.runner_cli.output_format),
            verbosity: Some(base_settings.runner_cli.verbosity),
            approval_mode: Some(base_settings.runner_cli.approval_mode),
            sandbox: Some(base_settings.runner_cli.sandbox),
            plan_mode: Some(base_settings.runner_cli.plan_mode),
            unsupported_option_policy: Some(base_settings.runner_cli.unsupported_option_policy),
        };
        let update_settings = task_cmd::TaskUpdateSettings {
            fields: "scope,evidence,plan,notes,tags,depends_on".to_string(),
            runner_override: Some(base_settings.runner),
            model_override: Some(base_settings.model.clone()),
            reasoning_effort_override: base_settings.reasoning_effort,
            runner_cli_overrides,
            force,
            repoprompt_tool_injection: policy.repoprompt_tool_injection,
            dry_run: false,
        };

        // Run pre-run task update, but don't fail if it errors - log warning and continue
        match task_cmd::update_task_without_lock(resolved, &task_id, &update_settings) {
            Ok(()) => {
                log::info!("Task {task_id}: pre-run update completed successfully");
            }
            Err(err) => {
                if runutil::abort_reason(&err).is_some() {
                    return Err(err);
                }
                if fail_on_prerun_update_error {
                    return Err(anyhow::anyhow!(
                        "Pre-run task update failed for {}: {}\n\n\
                         Troubleshooting:\n\
                         - Check runner configuration (agent.runner, agent.model)\n\
                         - Verify runner binary is on PATH\n\
                         - Run with --force to skip this check\n\
                         - Or set fail_on_prerun_update_error: false in config to warn only",
                        task_id,
                        err
                    ));
                }
                log::warn!(
                    "Task {task_id}: pre-run update failed (continuing with original task): {:#}",
                    err
                );
                log::debug!("Pre-run update error details: {:?}", err);
                // Continue with original task - don't fail the run
            }
        }

        // Reload the task so the execution prompt includes updated fields.
        // Use repair mechanism in case the update left malformed JSON.
        let updated_queue_file = queue::load_queue_with_repair(&resolved.queue_path)?;
        task = updated_queue_file
            .tasks
            .into_iter()
            .find(|t| t.id.trim() == task_id)
            .context("reload selected task after pre-run update")?;
    }

    // Mark the task as doing before running the agent.
    mark_task_doing(resolved, &task_id)?;

    // Save session state for crash recovery (before task execution)
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let session = create_session_for_task(
        &task_id,
        resolved,
        agent_overrides,
        iteration_settings.count,
    );
    if let Err(e) = session::save_session(&cache_dir, &session) {
        log::warn!("Failed to save session state: {}", e);
    }

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
        base_prompt = prompts::wrap_with_instruction_files(
            &resolved.repo_root,
            &base_prompt,
            &resolved.config,
        )?;

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
                notify_on_complete: agent_overrides.notify_on_complete,
                notify_sound: agent_overrides.notify_sound,
                lfs_check: agent_overrides.lfs_check.unwrap_or(false),
                no_progress: agent_overrides.no_progress.unwrap_or(false),
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

            // Send failure notification
            let notify_config = crate::notification::NotificationConfig {
                enabled: agent_overrides
                    .notify_on_complete
                    .or(resolved.config.agent.notification.enabled)
                    .unwrap_or(true),
                notify_on_complete: agent_overrides
                    .notify_on_complete
                    .or(resolved.config.agent.notification.notify_on_complete)
                    .unwrap_or(true),
                notify_on_fail: agent_overrides
                    .notify_on_fail
                    .or(resolved.config.agent.notification.notify_on_fail)
                    .unwrap_or(true),
                notify_on_loop_complete: resolved
                    .config
                    .agent
                    .notification
                    .notify_on_loop_complete
                    .unwrap_or(true),
                suppress_when_active: resolved
                    .config
                    .agent
                    .notification
                    .suppress_when_active
                    .unwrap_or(true),
                sound_enabled: agent_overrides
                    .notify_sound
                    .or(resolved.config.agent.notification.sound_enabled)
                    .unwrap_or(false),
                sound_path: resolved.config.agent.notification.sound_path.clone(),
                timeout_ms: resolved
                    .config
                    .agent
                    .notification
                    .timeout_ms
                    .unwrap_or(8000),
            };
            let error_summary = format!("{:#}", err);
            crate::notification::notify_task_failed(
                &task_id,
                &task.title,
                &error_summary,
                &notify_config,
            );

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
        &overrides.runner_cli,
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

/// Create a session state for the current task.
fn create_session_for_task(
    task_id: &str,
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    iterations_planned: u8,
) -> crate::contracts::SessionState {
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let git_commit = session::get_git_head_commit(&resolved.repo_root);

    // Resolve runner from overrides or config
    let runner = agent_overrides
        .runner
        .or(resolved.config.agent.runner)
        .unwrap_or(crate::contracts::Runner::Claude);

    // Resolve model string from overrides or config
    let model = agent_overrides
        .model
        .as_ref()
        .map(|m| m.as_str().to_string())
        .or_else(|| {
            resolved
                .config
                .agent
                .model
                .as_ref()
                .map(|m| m.as_str().to_string())
        })
        .unwrap_or_else(|| "sonnet".to_string());

    // Generate a simple session ID using timestamp and task ID
    let session_id = format!("{}-{}", now.replace([':', '.', '-'], ""), task_id);

    crate::contracts::SessionState::new(
        session_id,
        task_id.to_string(),
        now,
        iterations_planned,
        runner,
        model,
        0, // max_tasks - not tracked at this level
        git_commit,
    )
}

/// Validate that a resumed task exists and is in a runnable state.
/// Returns Ok(()) if valid, Err with message if not.
/// On validation failure, clears the session file.
fn validate_resumed_task(
    queue_file: &crate::contracts::QueueFile,
    task_id: &str,
    repo_root: &std::path::Path,
) -> Result<()> {
    let task = queue_file
        .tasks
        .iter()
        .find(|t| t.id.trim() == task_id)
        .ok_or_else(|| {
            let cache_dir = repo_root.join(".ralph/cache");
            if let Err(e) = session::clear_session(&cache_dir) {
                log::debug!("Failed to clear invalid session: {}", e);
            }
            anyhow::anyhow!("Task {} no longer exists in queue", task_id)
        })?;

    if task.status != TaskStatus::Doing {
        let cache_dir = repo_root.join(".ralph/cache");
        if let Err(e) = session::clear_session(&cache_dir) {
            log::debug!("Failed to clear invalid session: {}", e);
        }
        return Err(anyhow::anyhow!(
            "Task {} is not in Doing status (current: {})",
            task_id,
            task.status
        ));
    }

    Ok(())
}

#[cfg(test)]
fn update_then_mark_doing_if_configured<U, M>(
    update_enabled: bool,
    updater: U,
    marker: M,
) -> Result<()>
where
    U: FnOnce() -> Result<()>,
    M: FnOnce() -> Result<()>,
{
    if update_enabled {
        updater()?;
    }
    marker()?;
    Ok(())
}

#[cfg(test)]
mod pre_run_update_order_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn update_then_mark_doing_calls_update_first_when_enabled() {
        let calls = Cell::new(Vec::<&'static str>::new());

        update_then_mark_doing_if_configured(
            true,
            || {
                let mut v = calls.take();
                v.push("update");
                calls.set(v);
                Ok(())
            },
            || {
                let mut v = calls.take();
                v.push("mark");
                calls.set(v);
                Ok(())
            },
        )
        .expect("ok");

        assert_eq!(calls.take(), vec!["update", "mark"]);
    }

    #[test]
    fn update_then_mark_doing_skips_update_when_disabled() {
        let update_calls = Cell::new(0usize);
        let mark_calls = Cell::new(0usize);

        update_then_mark_doing_if_configured(
            false,
            || {
                update_calls.set(update_calls.get() + 1);
                Ok(())
            },
            || {
                mark_calls.set(mark_calls.get() + 1);
                Ok(())
            },
        )
        .expect("ok");

        assert_eq!(update_calls.get(), 0);
        assert_eq!(mark_calls.get(), 1);
    }
}

#[cfg(test)]
mod tests;
