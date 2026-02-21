//! Phase execution for run-one.
//!
//! Responsibilities:
//! - Execute iteration phases based on phase count (1, 2, or 3 phases).
//! - Build phase invocations with common fields populated.
//! - Apply followup reasoning effort for multi-iteration runs.
//!
//! Not handled here:
//! - Context preparation (see context.rs).
//! - Task setup (see execution_setup.rs).
//! - Webhook notifications (see webhooks.rs).
//!
//! Invariants/assumptions:
//! - Phase count is validated to be 1, 2, or 3 before execution.
//! - Iteration settings have been resolved before calling execute.

use std::cell::RefCell;

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::Task;
use crate::{prompts, runner};
use anyhow::{Result, bail};

use super::orchestration::TaskExecutionSetup;
use crate::commands::run::{
    iteration::apply_followup_reasoning_effort,
    phases::{self, PhaseInvocation, PostRunMode},
    supervision::PushPolicy,
};
use crate::plugins::registry::PluginRegistry;
use crate::promptflow;
use crate::runutil::RevertPromptHandler;

/// Execute iteration phases based on phase count.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_iteration_phases(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    task_id: &str,
    setup: &TaskExecutionSetup,
    base_prompt: &str,
    policy: &promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    post_run_mode: PostRunMode,
    plugins: &PluginRegistry,
) -> Result<()> {
    let ci_gate_enabled = resolved.config.agent.ci_gate_enabled.unwrap_or(true);
    let webhook_config = &resolved.config.agent.webhook;

    for iteration_index in 1..=setup.iteration_settings.count {
        let is_followup = iteration_index > 1;
        let is_final_iteration = iteration_index == setup.iteration_settings.count;

        // Log for structured logging (visible in log files)
        log::info!(
            "Task {task_id}: iteration {iteration_index}/{}",
            setup.iteration_settings.count
        );

        // Print to stderr for guaranteed user visibility (not buried in runner output)
        if setup.iteration_settings.count > 1 {
            if is_followup {
                eprintln!(); // Blank line separator between iterations
            }
            eprintln!(
                "━━━ Iteration {iteration_index}/{} ━━━",
                setup.iteration_settings.count
            );
        }

        let phase2_settings = apply_followup_reasoning_effort(
            &setup.phase_matrix.phase2.to_agent_settings(),
            setup.iteration_settings.followup_reasoning_effort,
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

        let allow_dirty = is_followup || setup.preexisting_dirty_allowed;

        match setup.phases {
            2 => {
                execute_two_phase_iteration(
                    resolved,
                    agent_overrides,
                    task,
                    task_id,
                    &phase2_settings,
                    setup,
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
                    is_followup,
                    allow_dirty,
                    post_run_mode,
                    setup.execution_timings.as_ref(),
                    plugins,
                    ci_gate_enabled,
                    webhook_config,
                )?;
            }
            3 => {
                execute_three_phase_iteration(
                    resolved,
                    agent_overrides,
                    task,
                    task_id,
                    &phase2_settings,
                    setup,
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
                    is_followup,
                    allow_dirty,
                    post_run_mode,
                    setup.execution_timings.as_ref(),
                    plugins,
                    ci_gate_enabled,
                    webhook_config,
                )?;
            }
            1 => {
                execute_single_phase_iteration(
                    resolved,
                    agent_overrides,
                    task,
                    task_id,
                    &phase2_settings,
                    setup,
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
                    is_followup,
                    allow_dirty,
                    post_run_mode,
                    setup.execution_timings.as_ref(),
                    plugins,
                    ci_gate_enabled,
                    webhook_config,
                )?;
            }
            _ => {
                bail!(
                    "Invalid phases value: {} (expected 1, 2, or 3). \
                     This indicates a configuration error or internal inconsistency.",
                    setup.phases
                );
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_two_phase_iteration(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    task_id: &str,
    phase2_settings: &runner::AgentSettings,
    setup: &TaskExecutionSetup,
    base_prompt: &str,
    policy: &promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    iteration_context: &str,
    iteration_completion_block: &str,
    phase3_completion_guidance: &str,
    is_final_iteration: bool,
    is_followup_iteration: bool,
    allow_dirty: bool,
    post_run_mode: PostRunMode,
    execution_timings: Option<
        &RefCell<crate::commands::run::execution_timings::RunExecutionTimings>,
    >,
    plugins: &PluginRegistry,
    ci_gate_enabled: bool,
    webhook_config: &crate::contracts::WebhookConfig,
) -> Result<()> {
    let phase1_settings = setup.phase_matrix.phase1.to_agent_settings();
    let phase1_invocation = build_phase_invocation(
        resolved,
        &phase1_settings,
        setup.bins,
        task_id,
        Some(&task.title),
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
        is_followup_iteration,
        allow_dirty,
        post_run_mode,
        agent_overrides,
        execution_timings,
        plugins,
    );

    let plan_text = super::webhooks::execute_phase1_with_webhooks(
        setup.phases,
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
        phase2_settings,
        setup.bins,
        task_id,
        Some(&task.title),
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
        is_followup_iteration,
        allow_dirty,
        post_run_mode,
        agent_overrides,
        execution_timings,
        plugins,
    );

    super::webhooks::execute_impl_phase_with_webhooks(
        2,
        setup.phases,
        task_id,
        &task.title,
        webhook_config,
        ci_gate_enabled,
        phase2_settings,
        resolved,
        &phase2_invocation,
        |inv| phases::execute_phase2_implementation(inv, setup.phases, &plan_text),
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_three_phase_iteration(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    task_id: &str,
    phase2_settings: &runner::AgentSettings,
    setup: &TaskExecutionSetup,
    base_prompt: &str,
    policy: &promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    iteration_context: &str,
    iteration_completion_block: &str,
    phase3_completion_guidance: &str,
    is_final_iteration: bool,
    is_followup_iteration: bool,
    allow_dirty: bool,
    post_run_mode: PostRunMode,
    execution_timings: Option<
        &RefCell<crate::commands::run::execution_timings::RunExecutionTimings>,
    >,
    plugins: &PluginRegistry,
    ci_gate_enabled: bool,
    webhook_config: &crate::contracts::WebhookConfig,
) -> Result<()> {
    // Phase 1: Planning
    let phase1_settings = setup.phase_matrix.phase1.to_agent_settings();
    let phase1_invocation = build_phase_invocation(
        resolved,
        &phase1_settings,
        setup.bins,
        task_id,
        Some(&task.title),
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
        is_followup_iteration,
        allow_dirty,
        post_run_mode,
        agent_overrides,
        execution_timings,
        plugins,
    );

    let plan_text = super::webhooks::execute_phase1_with_webhooks(
        setup.phases,
        task_id,
        &task.title,
        webhook_config,
        ci_gate_enabled,
        &phase1_settings,
        resolved,
        &phase1_invocation,
    )?;

    // Phase 2: Implementation
    let phase2_invocation = build_phase_invocation(
        resolved,
        phase2_settings,
        setup.bins,
        task_id,
        Some(&task.title),
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
        is_followup_iteration,
        allow_dirty,
        post_run_mode,
        agent_overrides,
        execution_timings,
        plugins,
    );

    super::webhooks::execute_impl_phase_with_webhooks(
        2,
        setup.phases,
        task_id,
        &task.title,
        webhook_config,
        ci_gate_enabled,
        phase2_settings,
        resolved,
        &phase2_invocation,
        |inv| phases::execute_phase2_implementation(inv, setup.phases, &plan_text),
    )?;

    // Phase 3: Review
    let phase3_settings = setup.phase_matrix.phase3.to_agent_settings();
    let phase3_invocation = build_phase_invocation(
        resolved,
        &phase3_settings,
        setup.bins,
        task_id,
        Some(&task.title),
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
        is_followup_iteration,
        allow_dirty,
        post_run_mode,
        agent_overrides,
        execution_timings,
        plugins,
    );

    super::webhooks::execute_impl_phase_with_webhooks(
        3,
        setup.phases,
        task_id,
        &task.title,
        webhook_config,
        ci_gate_enabled,
        &phase3_settings,
        resolved,
        &phase3_invocation,
        phases::execute_phase3_review,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_single_phase_iteration(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    task_id: &str,
    phase2_settings: &runner::AgentSettings,
    setup: &TaskExecutionSetup,
    base_prompt: &str,
    policy: &promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    iteration_context: &str,
    iteration_completion_block: &str,
    phase3_completion_guidance: &str,
    is_final_iteration: bool,
    is_followup_iteration: bool,
    allow_dirty: bool,
    post_run_mode: PostRunMode,
    execution_timings: Option<
        &RefCell<crate::commands::run::execution_timings::RunExecutionTimings>,
    >,
    plugins: &PluginRegistry,
    ci_gate_enabled: bool,
    webhook_config: &crate::contracts::WebhookConfig,
) -> Result<()> {
    let single_invocation = build_phase_invocation(
        resolved,
        phase2_settings,
        setup.bins,
        task_id,
        Some(&task.title),
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
        is_followup_iteration,
        allow_dirty,
        post_run_mode,
        agent_overrides,
        execution_timings,
        plugins,
    );

    super::webhooks::execute_impl_phase_with_webhooks(
        2,
        setup.phases,
        task_id,
        &task.title,
        webhook_config,
        ci_gate_enabled,
        phase2_settings,
        resolved,
        &single_invocation,
        phases::execute_single_phase,
    )
}

/// Build a PhaseInvocation with common fields populated.
#[allow(clippy::too_many_arguments)]
fn build_phase_invocation<'a>(
    resolved: &'a config::Resolved,
    settings: &'a runner::AgentSettings,
    bins: runner::RunnerBinaries<'a>,
    task_id: &'a str,
    task_title: Option<&'a str>,
    base_prompt: &'a str,
    policy: &'a promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    iteration_context: &'a str,
    iteration_completion_block: &'a str,
    phase3_completion_guidance: &'a str,
    is_final_iteration: bool,
    is_followup_iteration: bool,
    allow_dirty_repo: bool,
    post_run_mode: PostRunMode,
    agent_overrides: &AgentOverrides,
    execution_timings: Option<
        &'a RefCell<crate::commands::run::execution_timings::RunExecutionTimings>,
    >,
    plugins: &'a PluginRegistry,
) -> PhaseInvocation<'a> {
    PhaseInvocation {
        resolved,
        settings,
        bins,
        task_id,
        task_title,
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
        is_followup_iteration,
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
