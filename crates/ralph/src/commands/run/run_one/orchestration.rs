//! Core run-one orchestration.
//!
//! Responsibilities:
//! - Implement `run_one_impl`: lock, load/validate queues, select task,
//!   prompt assembly, iteration loop, phase execution, and post-run bookkeeping.
//!
//! Not handled here:
//! - Individual phase implementation details (see `phases`).
//! - Parallel run loop orchestration (see `parallel`).
//!
//! Invariants/assumptions:
//! - Callers pass the correct `QueueLockMode` for their context.
//! - Selection and resume behavior must remain behavior-identical.

use std::cell::RefCell;

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::{AgentConfig, GitRevertMode, ProjectType, Task, TaskStatus};
use crate::promptflow;
use crate::queue::RunnableSelectionOptions;
use crate::session::{self};
use crate::{git, prompts, queue, runner, runutil};
use anyhow::{Context, Result, bail};

use super::QueueLockMode;
use super::RunOutcome;
use crate::commands::run::{
    context::{mark_task_doing, task_context_for_prompt},
    execution_history_cli,
    execution_timings::RunExecutionTimings,
    iteration::{IterationSettings, apply_followup_reasoning_effort, resolve_iteration_settings},
    phases::{self, PostRunMode},
    run_session::create_session_for_task,
    supervision::PushPolicy,
};
use crate::plugins::registry::PluginRegistry;

/// Context prepared before task execution.
pub(crate) struct RunOneContext {
    pub queue_file: crate::contracts::QueueFile,
    pub done: crate::contracts::QueueFile,
    pub git_revert_mode: GitRevertMode,
    pub git_commit_push_enabled: bool,
    pub push_policy: PushPolicy,
    pub post_run_mode: PostRunMode,
    pub policy: promptflow::PromptPolicy,
}

/// Setup for task execution after selection.
pub(crate) struct TaskExecutionSetup<'a> {
    pub phases: u8,
    pub iteration_settings: IterationSettings,
    pub phase_matrix: runner::PhaseSettingsMatrix,
    pub preexisting_dirty_allowed: bool,
    pub plugin_registry: PluginRegistry,
    pub bins: runner::RunnerBinaries<'a>,
    pub execution_timings: Option<RefCell<RunExecutionTimings>>,
}

/// Prepare the context for run-one execution.
///
/// Handles Ctrl+C state, lock acquisition, queue loading/validation,
/// and configuration resolution.
fn prepare_run_one_context(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
) -> Result<RunOneContext> {
    // Handle Ctrl+C state initialization and pre-run interrupt detection.
    let ctrlc = crate::runner::ctrlc_state()
        .map_err(|e| anyhow::anyhow!("Ctrl-C handler initialization failed: {}", e))?;

    if ctrlc.interrupted.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(runutil::RunAbort::new(
            runutil::RunAbortReason::Interrupted,
            "Ctrl+C was pressed before task execution started",
        )
        .into());
    }

    ctrlc
        .interrupted
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let _queue_lock = match lock_mode {
        QueueLockMode::Acquire | QueueLockMode::AcquireAllowUpstream => Some(
            queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?,
        ),
        QueueLockMode::Held => None,
    };

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        Some(&done),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

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

    let push_policy = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PushPolicy::AllowCreateUpstream,
        QueueLockMode::Acquire | QueueLockMode::Held => PushPolicy::RequireUpstream,
    };

    let post_run_mode = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PostRunMode::ParallelWorker,
        QueueLockMode::Acquire | QueueLockMode::Held => PostRunMode::Normal,
    };

    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: repoprompt_flags.plan_required,
        repoprompt_tool_injection: repoprompt_flags.tool_injection,
    };

    Ok(RunOneContext {
        queue_file,
        done,
        git_revert_mode,
        git_commit_push_enabled,
        push_policy,
        post_run_mode,
        policy,
    })
}

/// Setup task execution after a task has been selected.
///
/// Resolves phase count, iteration settings, phase matrix,
/// validates repo state, loads plugins, marks task doing, and creates session.
fn setup_task_execution<'a>(
    resolved: &'a config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    post_run_mode: PostRunMode,
    force: bool,
) -> Result<TaskExecutionSetup<'a>> {
    let phases = resolve_task_phase_count(agent_overrides, task, &resolved.config.agent)?;

    let iteration_settings = resolve_iteration_settings(task, &resolved.config.agent)?;
    log::info!(
        "RunOne: selected {} (phases={}, iterations={})",
        task.id.trim(),
        phases,
        iteration_settings.count
    );

    let (phase_matrix, phase_warnings) = runner::resolve_phase_settings_matrix(
        agent_overrides,
        &resolved.config.agent,
        task.agent.as_ref(),
        phases,
    )?;

    if phase_warnings.unused_phase1 {
        log::warn!(
            "Task {}: Phase 1 overrides specified but will not be used (phases < 2)",
            task.id.trim()
        );
    }
    if phase_warnings.unused_phase2 {
        log::warn!(
            "Task {}: Phase 2 overrides specified but will not be used (phases < 2 or single-phase mode)",
            task.id.trim()
        );
    }
    if phase_warnings.unused_phase3 {
        log::warn!(
            "Task {}: Phase 3 overrides specified but will not be used (phases < 3)",
            task.id.trim()
        );
    }

    log::info!("Task {}: Resolved phase settings:", task.id.trim());
    if phases >= 2 {
        log::info!(
            "  Phase 1 (Planning): runner={:?}, model={}",
            phase_matrix.phase1.runner,
            phase_matrix.phase1.model.as_str()
        );
    }
    log::info!(
        "  Phase 2 (Implementation): runner={:?}, model={}",
        phase_matrix.phase2.runner,
        phase_matrix.phase2.model.as_str()
    );
    if phases >= 3 {
        log::info!(
            "  Phase 3 (Review): runner={:?}, model={}",
            phase_matrix.phase3.runner,
            phase_matrix.phase3.model.as_str()
        );
    }

    let preexisting_dirty_allowed = git::repo_dirty_only_allowed_paths(
        &resolved.repo_root,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    let plugin_registry = PluginRegistry::load(&resolved.repo_root, &resolved.config)
        .context("load plugin registry")?;

    if !plugin_registry.discovered().is_empty() {
        let exec = crate::plugins::processor_executor::ProcessorExecutor::new(
            &resolved.repo_root,
            &plugin_registry,
        );
        exec.validate_task(task)
            .context("processor validate_task hook failed")?;
    }

    if matches!(post_run_mode, PostRunMode::ParallelWorker) {
        log::info!(
            "Task {}: parallel worker mode skips mark_task_doing to avoid queue writes",
            task.id.trim()
        );
    } else {
        mark_task_doing(resolved, &task.id)?;
    }

    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let session = create_session_for_task(
        &task.id,
        resolved,
        agent_overrides,
        iteration_settings.count,
        Some(&phase_matrix),
    );
    if let Err(e) = session::save_session(&cache_dir, &session) {
        log::warn!("Failed to save session state: {}", e);
    }

    let bins = runner::resolve_binaries(&resolved.config.agent);

    log::info!("Task {}: start", task.id.trim());

    let execution_timings: Option<RefCell<RunExecutionTimings>> =
        if post_run_mode == PostRunMode::ParallelWorker {
            None
        } else {
            Some(RefCell::new(RunExecutionTimings::default()))
        };

    Ok(TaskExecutionSetup {
        phases,
        iteration_settings,
        phase_matrix,
        preexisting_dirty_allowed,
        plugin_registry,
        bins,
        execution_timings,
    })
}

/// Build the base prompt for task execution.
fn build_base_prompt(resolved: &config::Resolved, task: &Task, task_id: &str) -> Result<String> {
    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut base_prompt =
        prompts::render_worker_prompt(&template, task_id, project_type, &resolved.config)?;
    base_prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &base_prompt, &resolved.config)?;

    let task_context = task_context_for_prompt(task)?;
    base_prompt = format!("{task_context}\n\n---\n\n{base_prompt}");

    Ok(base_prompt)
}

/// Execute phase 1 (planning) with webhook notifications.
/// Returns the plan text on success.
#[allow(clippy::too_many_arguments)]
fn execute_phase1_with_webhooks(
    phase_count: u8,
    task_id: &str,
    task_title: &str,
    webhook_config: &crate::contracts::WebhookConfig,
    _ci_gate_enabled: bool,
    settings: &runner::AgentSettings,
    resolved: &config::Resolved,
    invocation: &phases::PhaseInvocation<'_>,
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

    let result = phases::execute_phase1_planning(invocation, phase_count);

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
fn execute_impl_phase_with_webhooks<F>(
    phase_num: u8,
    phase_count: u8,
    task_id: &str,
    task_title: &str,
    webhook_config: &crate::contracts::WebhookConfig,
    ci_gate_enabled: bool,
    settings: &runner::AgentSettings,
    resolved: &config::Resolved,
    invocation: &phases::PhaseInvocation<'_>,
    phase_executor: F,
) -> Result<()>
where
    F: FnOnce(&phases::PhaseInvocation<'_>) -> Result<()>,
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

/// Build a PhaseInvocation with common fields populated.
#[allow(clippy::too_many_arguments)]
fn build_phase_invocation<'a>(
    resolved: &'a config::Resolved,
    settings: &'a runner::AgentSettings,
    bins: runner::RunnerBinaries<'a>,
    task_id: &'a str,
    base_prompt: &'a str,
    policy: &'a promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: ProjectType,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    iteration_context: &'a str,
    iteration_completion_block: &'a str,
    phase3_completion_guidance: &'a str,
    is_final_iteration: bool,
    allow_dirty_repo: bool,
    post_run_mode: PostRunMode,
    agent_overrides: &AgentOverrides,
    execution_timings: Option<&'a RefCell<RunExecutionTimings>>,
    plugins: &'a PluginRegistry,
) -> phases::PhaseInvocation<'a> {
    phases::PhaseInvocation {
        resolved,
        settings,
        bins,
        task_id,
        base_prompt,
        policy,
        output_handler,
        output_stream,
        project_type,
        git_revert_mode,
        git_commit_push_enabled,
        push_policy,
        revert_prompt,
        iteration_context,
        iteration_completion_block,
        phase3_completion_guidance,
        is_final_iteration,
        allow_dirty_repo,
        post_run_mode,
        notify_on_complete: agent_overrides.notify_on_complete,
        notify_sound: agent_overrides.notify_sound,
        lfs_check: agent_overrides.lfs_check.unwrap_or(false),
        no_progress: agent_overrides.no_progress.unwrap_or(false),
        execution_timings,
        plugins: Some(plugins),
    }
}

/// Execute iteration phases based on phase count.
#[allow(clippy::too_many_arguments)]
fn execute_iteration_phases(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    task_id: &str,
    phases: u8,
    iteration_settings: &IterationSettings,
    phase_matrix: &runner::PhaseSettingsMatrix,
    base_prompt: &str,
    policy: &promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: ProjectType,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    preexisting_dirty_allowed: bool,
    post_run_mode: PostRunMode,
    bins: runner::RunnerBinaries<'_>,
    execution_timings: Option<&RefCell<RunExecutionTimings>>,
    plugins: &PluginRegistry,
) -> Result<()> {
    let ci_gate_enabled = resolved.config.agent.ci_gate_enabled.unwrap_or(true);
    let webhook_config = &resolved.config.agent.webhook;

    for iteration_index in 1..=iteration_settings.count {
        let is_followup = iteration_index > 1;
        let is_final_iteration = iteration_index == iteration_settings.count;

        log::info!(
            "Task {task_id}: iteration {iteration_index}/{}",
            iteration_settings.count
        );

        let phase2_settings = apply_followup_reasoning_effort(
            &phase_matrix.phase2.to_agent_settings(),
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

        let allow_dirty = is_followup || preexisting_dirty_allowed;

        match phases {
            2 => {
                let phase1_settings = phase_matrix.phase1.to_agent_settings();
                let phase1_invocation = build_phase_invocation(
                    resolved,
                    &phase1_settings,
                    bins,
                    task_id,
                    base_prompt,
                    policy,
                    output_handler.clone(),
                    output_stream,
                    project_type,
                    git_revert_mode,
                    git_commit_push_enabled,
                    push_policy,
                    revert_prompt.clone(),
                    iteration_context,
                    iteration_completion_block,
                    phase3_completion_guidance,
                    is_final_iteration,
                    allow_dirty,
                    post_run_mode,
                    agent_overrides,
                    execution_timings,
                    plugins,
                );

                let plan_text = execute_phase1_with_webhooks(
                    phases,
                    task_id,
                    &task.title,
                    webhook_config,
                    ci_gate_enabled,
                    &phase1_settings,
                    resolved,
                    &phase1_invocation,
                )?;

                let phase2_invocation = build_phase_invocation(
                    resolved,
                    &phase2_settings,
                    bins,
                    task_id,
                    base_prompt,
                    policy,
                    output_handler.clone(),
                    output_stream,
                    project_type,
                    git_revert_mode,
                    git_commit_push_enabled,
                    push_policy,
                    revert_prompt.clone(),
                    iteration_context,
                    iteration_completion_block,
                    phase3_completion_guidance,
                    is_final_iteration,
                    allow_dirty,
                    post_run_mode,
                    agent_overrides,
                    execution_timings,
                    plugins,
                );

                execute_impl_phase_with_webhooks(
                    2,
                    phases,
                    task_id,
                    &task.title,
                    webhook_config,
                    ci_gate_enabled,
                    &phase2_settings,
                    resolved,
                    &phase2_invocation,
                    |inv| phases::execute_phase2_implementation(inv, phases, &plan_text),
                )?;
            }
            3 => {
                let phase1_settings = phase_matrix.phase1.to_agent_settings();
                let phase1_invocation = build_phase_invocation(
                    resolved,
                    &phase1_settings,
                    bins,
                    task_id,
                    base_prompt,
                    policy,
                    output_handler.clone(),
                    output_stream,
                    project_type,
                    git_revert_mode,
                    git_commit_push_enabled,
                    push_policy,
                    revert_prompt.clone(),
                    iteration_context,
                    iteration_completion_block,
                    phase3_completion_guidance,
                    is_final_iteration,
                    allow_dirty,
                    post_run_mode,
                    agent_overrides,
                    execution_timings,
                    plugins,
                );

                let plan_text = execute_phase1_with_webhooks(
                    phases,
                    task_id,
                    &task.title,
                    webhook_config,
                    ci_gate_enabled,
                    &phase1_settings,
                    resolved,
                    &phase1_invocation,
                )?;

                let phase2_invocation = build_phase_invocation(
                    resolved,
                    &phase2_settings,
                    bins,
                    task_id,
                    base_prompt,
                    policy,
                    output_handler.clone(),
                    output_stream,
                    project_type,
                    git_revert_mode,
                    git_commit_push_enabled,
                    push_policy,
                    revert_prompt.clone(),
                    iteration_context,
                    iteration_completion_block,
                    phase3_completion_guidance,
                    is_final_iteration,
                    allow_dirty,
                    post_run_mode,
                    agent_overrides,
                    execution_timings,
                    plugins,
                );

                execute_impl_phase_with_webhooks(
                    2,
                    phases,
                    task_id,
                    &task.title,
                    webhook_config,
                    ci_gate_enabled,
                    &phase2_settings,
                    resolved,
                    &phase2_invocation,
                    |inv| phases::execute_phase2_implementation(inv, phases, &plan_text),
                )?;

                let phase3_settings = phase_matrix.phase3.to_agent_settings();
                let phase3_invocation = build_phase_invocation(
                    resolved,
                    &phase3_settings,
                    bins,
                    task_id,
                    base_prompt,
                    policy,
                    output_handler.clone(),
                    output_stream,
                    project_type,
                    git_revert_mode,
                    git_commit_push_enabled,
                    push_policy,
                    revert_prompt.clone(),
                    iteration_context,
                    iteration_completion_block,
                    phase3_completion_guidance,
                    is_final_iteration,
                    allow_dirty,
                    post_run_mode,
                    agent_overrides,
                    execution_timings,
                    plugins,
                );

                execute_impl_phase_with_webhooks(
                    3,
                    phases,
                    task_id,
                    &task.title,
                    webhook_config,
                    ci_gate_enabled,
                    &phase3_settings,
                    resolved,
                    &phase3_invocation,
                    phases::execute_phase3_review,
                )?;
            }
            1 => {
                let single_invocation = build_phase_invocation(
                    resolved,
                    &phase2_settings,
                    bins,
                    task_id,
                    base_prompt,
                    policy,
                    output_handler.clone(),
                    output_stream,
                    project_type,
                    git_revert_mode,
                    git_commit_push_enabled,
                    push_policy,
                    revert_prompt.clone(),
                    iteration_context,
                    iteration_completion_block,
                    phase3_completion_guidance,
                    is_final_iteration,
                    allow_dirty,
                    post_run_mode,
                    agent_overrides,
                    execution_timings,
                    plugins,
                );

                execute_impl_phase_with_webhooks(
                    2,
                    phases,
                    task_id,
                    &task.title,
                    webhook_config,
                    ci_gate_enabled,
                    &phase2_settings,
                    resolved,
                    &single_invocation,
                    phases::execute_single_phase,
                )?;
            }
            _ => {
                bail!(
                    "Invalid phases value: {} (expected 1, 2, or 3). \
                     This indicates a configuration error or internal inconsistency.",
                    phases
                );
            }
        }
    }

    Ok(())
}

/// Handle run completion (success or failure).
#[allow(clippy::too_many_arguments)]
fn handle_run_completion(
    exec_result: Result<()>,
    resolved: &config::Resolved,
    task: &Task,
    task_id: &str,
    phases: u8,
    post_run_mode: PostRunMode,
    execution_timings: Option<RefCell<RunExecutionTimings>>,
    agent_overrides: &AgentOverrides,
) -> Result<RunOutcome> {
    match exec_result {
        Ok(()) => {
            log::info!("Task {task_id}: end");

            if post_run_mode != PostRunMode::ParallelWorker
                && let Some(timings) = execution_timings
            {
                match execution_history_cli::try_record_execution_history_for_cli_run(
                    &resolved.repo_root,
                    &resolved.done_path,
                    task_id,
                    phases,
                    timings.into_inner(),
                ) {
                    Ok(true) => {
                        log::debug!("Recorded execution history for {} (CLI mode)", task_id)
                    }
                    Ok(false) => log::debug!(
                        "Skipping execution history for {}: task not Done or timing payload unavailable.",
                        task_id
                    ),
                    Err(err) => log::warn!(
                        "Failed to record execution history for {}: {}",
                        task_id,
                        err
                    ),
                }
            }

            Ok(RunOutcome::Ran {
                task_id: task_id.to_string(),
            })
        }
        Err(err) => {
            log::error!("Task {task_id}: error");

            let notify_config = crate::notification::build_notification_config(
                &resolved.config.agent.notification,
                &crate::notification::NotificationOverrides {
                    notify_on_complete: agent_overrides.notify_on_complete,
                    notify_on_fail: agent_overrides.notify_on_fail,
                    notify_sound: agent_overrides.notify_sound,
                },
            );
            let error_summary = format!("{:#}", err);
            crate::notification::notify_task_failed(
                task_id,
                &task.title,
                &error_summary,
                &notify_config,
            );

            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_one_impl(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
    target_task_id: Option<&str>,
    resume_task_id: Option<&str>,
    output_handler: Option<runner::OutputHandler>,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<RunOutcome> {
    // 1. Prepare context (lock, queue, config)
    let ctx = prepare_run_one_context(resolved, agent_overrides, force, lock_mode)?;

    // 2. Select task
    let include_draft = agent_overrides.include_draft.unwrap_or(false);
    let selection = select_task_for_run(
        &ctx.queue_file,
        Some(&ctx.done),
        target_task_id,
        resume_task_id,
        &resolved.repo_root,
        include_draft,
    )?;

    let task = match selection {
        SelectTaskResult::NoCandidates => return Ok(RunOutcome::NoCandidates),
        SelectTaskResult::Blocked { summary } => return Ok(RunOutcome::Blocked { summary }),
        SelectTaskResult::Selected { task } => *task,
    };
    let task_id = task.id.trim().to_string();

    // 3. Setup execution
    let setup = setup_task_execution(resolved, agent_overrides, &task, ctx.post_run_mode, force)?;

    // 4. Build prompt
    let base_prompt = build_base_prompt(resolved, &task, &task_id)?;

    // 5. Execute phases
    let output_stream = if output_handler.is_some() {
        runner::OutputStream::HandlerOnly
    } else {
        runner::OutputStream::Terminal
    };

    let exec_result = execute_iteration_phases(
        resolved,
        agent_overrides,
        &task,
        &task_id,
        setup.phases,
        &setup.iteration_settings,
        &setup.phase_matrix,
        &base_prompt,
        &ctx.policy,
        output_handler.clone(),
        output_stream,
        resolved.config.project_type.unwrap_or(ProjectType::Code),
        ctx.git_revert_mode,
        ctx.git_commit_push_enabled,
        ctx.push_policy,
        revert_prompt,
        setup.preexisting_dirty_allowed,
        ctx.post_run_mode,
        setup.bins,
        setup.execution_timings.as_ref(),
        &setup.plugin_registry,
    );

    // 6. Handle completion
    handle_run_completion(
        exec_result,
        resolved,
        &task,
        &task_id,
        setup.phases,
        ctx.post_run_mode,
        setup.execution_timings,
        agent_overrides,
    )
}

/// Result of task selection.
pub(crate) enum SelectTaskResult {
    /// A task was selected for execution.
    Selected {
        /// The selected task (boxed to avoid large enum variant).
        task: Box<Task>,
    },
    /// No candidates available (no todo/draft tasks in queue).
    NoCandidates,
    /// Tasks exist but all are blocked by dependencies or schedule.
    Blocked {
        /// Summary of why tasks are blocked.
        summary: crate::queue::operations::QueueRunnabilitySummary,
    },
}

/// Build a summary for blocked tasks when no runnable tasks are found.
fn build_blocked_summary(
    queue_file: &crate::contracts::QueueFile,
    done_ref: Option<&crate::contracts::QueueFile>,
    candidates: &[Task],
    include_draft: bool,
) -> crate::queue::operations::QueueRunnabilitySummary {
    let options = RunnableSelectionOptions::new(include_draft, true);
    match crate::queue::operations::queue_runnability_report(queue_file, done_ref, options) {
        Ok(report) => {
            if report.summary.blocked_by_schedule > 0 && report.summary.blocked_by_dependencies > 0
            {
                log::info!(
                    "All runnable tasks are blocked by unmet dependencies and future schedule ({} deps, {} scheduled).",
                    report.summary.blocked_by_dependencies,
                    report.summary.blocked_by_schedule
                );
            } else if report.summary.blocked_by_schedule > 0 {
                log::info!(
                    "All runnable tasks are blocked by future schedule ({} scheduled).",
                    report.summary.blocked_by_schedule
                );
            } else if report.summary.blocked_by_dependencies > 0 {
                log::info!(
                    "All runnable tasks are blocked by unmet dependencies ({} blocked).",
                    report.summary.blocked_by_dependencies
                );
            } else if report.summary.blocked_by_status_or_flags > 0 {
                log::info!(
                    "All tasks are blocked by status or flags ({} blocked).",
                    report.summary.blocked_by_status_or_flags
                );
            } else {
                log::info!("All runnable tasks are blocked.");
            }
            log::info!("Run 'ralph queue explain' for details.");
            report.summary.clone()
        }
        Err(e) => {
            log::info!(
                "No runnable tasks found (failed to analyze blockers: {}).",
                e
            );
            log::info!("Run 'ralph queue explain' for details.");
            crate::queue::operations::QueueRunnabilitySummary {
                total_active: queue_file.tasks.len(),
                candidates_total: candidates.len(),
                runnable_candidates: 0,
                blocked_by_dependencies: candidates.len(),
                blocked_by_schedule: 0,
                blocked_by_status_or_flags: 0,
            }
        }
    }
}

/// Select a task for execution.
fn select_task_for_run(
    queue_file: &crate::contracts::QueueFile,
    done_ref: Option<&crate::contracts::QueueFile>,
    target_task_id: Option<&str>,
    resume_task_id: Option<&str>,
    repo_root: &std::path::Path,
    include_draft: bool,
) -> Result<SelectTaskResult> {
    use crate::commands::run::run_session::validate_resumed_task;
    use crate::commands::run::selection::select_run_one_task_index;

    let effective_target = if target_task_id.is_some() {
        target_task_id
    } else if let Some(resume_id) = resume_task_id {
        match validate_resumed_task(queue_file, resume_id, repo_root) {
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
        match select_run_one_task_index(queue_file, done_ref, effective_target, include_draft)? {
            Some(idx) => idx,
            None => {
                let candidates: Vec<_> = queue_file
                    .tasks
                    .iter()
                    .filter(|t| {
                        t.status == TaskStatus::Todo
                            || (include_draft && t.status == TaskStatus::Draft)
                    })
                    .cloned()
                    .collect();

                if candidates.is_empty() {
                    if include_draft {
                        log::info!("No todo or draft tasks found.");
                    } else {
                        log::info!("No todo tasks found.");
                    }
                    return Ok(SelectTaskResult::NoCandidates);
                }

                let summary =
                    build_blocked_summary(queue_file, done_ref, &candidates, include_draft);
                return Ok(SelectTaskResult::Blocked { summary });
            }
        };

    let task = queue_file.tasks[task_idx].clone();
    Ok(SelectTaskResult::Selected {
        task: Box::new(task),
    })
}

fn resolve_task_phase_count(
    agent_overrides: &AgentOverrides,
    task: &Task,
    config_agent: &AgentConfig,
) -> Result<u8> {
    let phases = agent_overrides
        .phases
        .or(task.agent.as_ref().and_then(|agent| agent.phases))
        .or(config_agent.phases)
        .unwrap_or(2);

    if !(1..=3).contains(&phases) {
        bail!("Invalid phases value: {} (expected 1, 2, or 3)", phases);
    }

    Ok(phases)
}

#[cfg(test)]
mod tests {
    use super::resolve_task_phase_count;
    use crate::agent::AgentOverrides;
    use crate::contracts::{AgentConfig, Task, TaskAgent};

    #[test]
    fn resolve_task_phase_count_uses_cli_over_task_and_config() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "test".to_string(),
            ..Default::default()
        };
        task.agent = Some(TaskAgent {
            phases: Some(2),
            ..Default::default()
        });
        let config = AgentConfig {
            phases: Some(3),
            ..Default::default()
        };
        let overrides = AgentOverrides {
            phases: Some(1),
            ..Default::default()
        };

        let phases = resolve_task_phase_count(&overrides, &task, &config).expect("phases");
        assert_eq!(phases, 1);
    }

    #[test]
    fn resolve_task_phase_count_uses_task_when_cli_not_set() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "test".to_string(),
            ..Default::default()
        };
        task.agent = Some(TaskAgent {
            phases: Some(2),
            ..Default::default()
        });
        let config = AgentConfig {
            phases: Some(3),
            ..Default::default()
        };

        let phases =
            resolve_task_phase_count(&AgentOverrides::default(), &task, &config).expect("phases");
        assert_eq!(phases, 2);
    }

    #[test]
    fn resolve_task_phase_count_rejects_invalid_task_phase_value() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "test".to_string(),
            ..Default::default()
        };
        task.agent = Some(TaskAgent {
            phases: Some(4),
            ..Default::default()
        });

        let err =
            resolve_task_phase_count(&AgentOverrides::default(), &task, &AgentConfig::default())
                .expect_err("expected invalid phases error");
        assert!(err.to_string().contains("Invalid phases value: 4"));
    }
}
