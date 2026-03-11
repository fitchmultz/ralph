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
//! - Webhook notifications beyond phase-level wrappers (see webhooks.rs).
//!
//! Invariants/assumptions:
//! - Phase count is validated to be 1, 2, or 3 before execution.
//! - Iteration settings have been resolved before calling execute.

use crate::agent::AgentOverrides;
use crate::commands::run::{
    RunEvent, RunEventHandler,
    iteration::apply_followup_reasoning_effort,
    phases::{self, PhaseInvocation, PostRunMode},
    supervision::PushPolicy,
};
use crate::config;
use crate::contracts::Task;
use crate::plugins::registry::PluginRegistry;
use crate::promptflow;
use crate::runutil::RevertPromptHandler;
use crate::{prompts, runner};
use anyhow::{Result, bail};

use super::orchestration::TaskExecutionSetup;

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
    run_event_handler: Option<RunEventHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    post_run_mode: PostRunMode,
    parallel_target_branch: Option<&str>,
    plugins: &PluginRegistry,
) -> Result<()> {
    PhaseExecutionContext {
        resolved,
        agent_overrides,
        task,
        task_id,
        setup,
        base_prompt,
        policy,
        output_handler,
        run_event_handler,
        output_stream,
        project_type,
        git_revert_mode,
        git_commit_push_enabled,
        push_policy,
        revert_prompt,
        post_run_mode,
        parallel_target_branch,
        plugins,
        ci_gate_enabled: resolved.config.agent.ci_gate_enabled(),
        webhook_config: &resolved.config.agent.webhook,
    }
    .execute()
}

struct PhaseExecutionContext<'a> {
    resolved: &'a config::Resolved,
    agent_overrides: &'a AgentOverrides,
    task: &'a Task,
    task_id: &'a str,
    setup: &'a TaskExecutionSetup<'a>,
    base_prompt: &'a str,
    policy: &'a promptflow::PromptPolicy,
    output_handler: Option<runner::OutputHandler>,
    run_event_handler: Option<RunEventHandler>,
    output_stream: runner::OutputStream,
    project_type: crate::contracts::ProjectType,
    git_revert_mode: crate::contracts::GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<RevertPromptHandler>,
    post_run_mode: PostRunMode,
    parallel_target_branch: Option<&'a str>,
    plugins: &'a PluginRegistry,
    ci_gate_enabled: bool,
    webhook_config: &'a crate::contracts::WebhookConfig,
}

struct IterationExecution<'a> {
    phase2_settings: runner::AgentSettings,
    iteration_context: &'a str,
    iteration_completion_block: &'a str,
    phase3_completion_guidance: &'a str,
    is_final_iteration: bool,
    is_followup_iteration: bool,
    allow_dirty_repo: bool,
}

impl<'a> PhaseExecutionContext<'a> {
    fn emit_phase_entered(&self, phase: crate::progress::ExecutionPhase) {
        if let Some(handler) = &self.run_event_handler {
            handler(RunEvent::PhaseEntered { phase });
        }
    }

    fn emit_phase_completed(&self, phase: crate::progress::ExecutionPhase) {
        if let Some(handler) = &self.run_event_handler {
            handler(RunEvent::PhaseCompleted { phase });
        }
    }

    fn execute(&self) -> Result<()> {
        for iteration_index in 1..=self.setup.iteration_settings.count {
            self.log_iteration(iteration_index);
            let iteration = self.build_iteration(iteration_index);

            match self.setup.phases {
                1 => self.execute_single_phase_iteration(&iteration)?,
                2 => self.execute_planned_iteration(&iteration, false)?,
                3 => self.execute_planned_iteration(&iteration, true)?,
                _ => {
                    bail!(
                        "Invalid phases value: {} (expected 1, 2, or 3). \
                         This indicates a configuration error or internal inconsistency.",
                        self.setup.phases
                    );
                }
            }
        }

        Ok(())
    }

    fn log_iteration(&self, iteration_index: u8) {
        let is_followup_iteration = iteration_index > 1;

        log::info!(
            "Task {}: iteration {iteration_index}/{}",
            self.task_id,
            self.setup.iteration_settings.count
        );

        if self.setup.iteration_settings.count > 1 {
            if is_followup_iteration {
                eprintln!();
            }
            eprintln!(
                "━━━ Iteration {iteration_index}/{} ━━━",
                self.setup.iteration_settings.count
            );
        }
    }

    fn build_iteration(&self, iteration_index: u8) -> IterationExecution<'static> {
        let is_followup_iteration = iteration_index > 1;
        let is_final_iteration = iteration_index == self.setup.iteration_settings.count;

        IterationExecution {
            phase2_settings: apply_followup_reasoning_effort(
                &self.setup.phase_matrix.phase2.to_agent_settings(),
                self.setup.iteration_settings.followup_reasoning_effort,
                is_followup_iteration,
            ),
            iteration_context: if is_followup_iteration {
                prompts::ITERATION_CONTEXT_REFINEMENT
            } else {
                ""
            },
            iteration_completion_block: if is_final_iteration {
                ""
            } else {
                prompts::ITERATION_COMPLETION_BLOCK
            },
            phase3_completion_guidance: if is_final_iteration {
                prompts::PHASE3_COMPLETION_GUIDANCE_FINAL
            } else {
                prompts::PHASE3_COMPLETION_GUIDANCE_NONFINAL
            },
            is_final_iteration,
            is_followup_iteration,
            allow_dirty_repo: is_followup_iteration || self.setup.preexisting_dirty_allowed,
        }
    }

    fn execute_planned_iteration(
        &self,
        iteration: &IterationExecution<'_>,
        include_review: bool,
    ) -> Result<()> {
        let phase1_settings = self.setup.phase_matrix.phase1.to_agent_settings();
        let plan_text = self.execute_phase1(iteration, &phase1_settings)?;

        self.execute_phase2(iteration, &iteration.phase2_settings, &plan_text)?;

        if include_review {
            let phase3_settings = self.setup.phase_matrix.phase3.to_agent_settings();
            self.execute_phase3(iteration, &phase3_settings)?;
        }

        Ok(())
    }

    fn execute_single_phase_iteration(&self, iteration: &IterationExecution<'_>) -> Result<()> {
        let invocation = self.build_phase_invocation(iteration, &iteration.phase2_settings);
        self.emit_phase_entered(crate::progress::ExecutionPhase::Implementation);

        let result = super::webhooks::execute_impl_phase_with_webhooks(
            2,
            self.setup.phases,
            self.task_id,
            &self.task.title,
            self.webhook_config,
            self.ci_gate_enabled,
            &iteration.phase2_settings,
            self.resolved,
            &invocation,
            phases::execute_single_phase,
        );
        if result.is_ok() {
            self.emit_phase_completed(crate::progress::ExecutionPhase::Implementation);
        }
        result
    }

    fn execute_phase1(
        &self,
        iteration: &IterationExecution<'_>,
        settings: &runner::AgentSettings,
    ) -> Result<String> {
        let invocation = self.build_phase_invocation(iteration, settings);
        self.emit_phase_entered(crate::progress::ExecutionPhase::Planning);
        let result = super::webhooks::execute_phase1_with_webhooks(
            self.setup.phases,
            self.task_id,
            &self.task.title,
            self.webhook_config,
            self.ci_gate_enabled,
            settings,
            self.resolved,
            &invocation,
        );
        if result.is_ok() {
            self.emit_phase_completed(crate::progress::ExecutionPhase::Planning);
        }
        result
    }

    fn execute_phase2(
        &self,
        iteration: &IterationExecution<'_>,
        settings: &runner::AgentSettings,
        plan_text: &str,
    ) -> Result<()> {
        let invocation = self.build_phase_invocation(iteration, settings);
        self.emit_phase_entered(crate::progress::ExecutionPhase::Implementation);

        let result = super::webhooks::execute_impl_phase_with_webhooks(
            2,
            self.setup.phases,
            self.task_id,
            &self.task.title,
            self.webhook_config,
            self.ci_gate_enabled,
            settings,
            self.resolved,
            &invocation,
            |phase_invocation| {
                phases::execute_phase2_implementation(
                    phase_invocation,
                    self.setup.phases,
                    plan_text,
                )
            },
        );
        if result.is_ok() {
            self.emit_phase_completed(crate::progress::ExecutionPhase::Implementation);
        }
        result
    }

    fn execute_phase3(
        &self,
        iteration: &IterationExecution<'_>,
        settings: &runner::AgentSettings,
    ) -> Result<()> {
        let invocation = self.build_phase_invocation(iteration, settings);
        self.emit_phase_entered(crate::progress::ExecutionPhase::Review);

        let result = super::webhooks::execute_impl_phase_with_webhooks(
            3,
            self.setup.phases,
            self.task_id,
            &self.task.title,
            self.webhook_config,
            self.ci_gate_enabled,
            settings,
            self.resolved,
            &invocation,
            phases::execute_phase3_review,
        );
        if result.is_ok() {
            self.emit_phase_completed(crate::progress::ExecutionPhase::Review);
        }
        result
    }

    fn build_phase_invocation<'b>(
        &'b self,
        iteration: &'b IterationExecution<'b>,
        settings: &'b runner::AgentSettings,
    ) -> PhaseInvocation<'b> {
        PhaseInvocation {
            resolved: self.resolved,
            settings,
            bins: self.setup.bins,
            task_id: self.task_id,
            task_title: Some(&self.task.title),
            base_prompt: self.base_prompt,
            policy: self.policy,
            output_handler: self.output_handler.clone(),
            output_stream: self.output_stream,
            project_type: self.project_type,
            git_revert_mode: self.git_revert_mode,
            git_commit_push_enabled: self.git_commit_push_enabled,
            push_policy: self.push_policy,
            revert_prompt: self.revert_prompt.clone(),
            iteration_context: iteration.iteration_context,
            iteration_completion_block: iteration.iteration_completion_block,
            phase3_completion_guidance: iteration.phase3_completion_guidance,
            is_final_iteration: iteration.is_final_iteration,
            is_followup_iteration: iteration.is_followup_iteration,
            allow_dirty_repo: iteration.allow_dirty_repo,
            post_run_mode: self.post_run_mode,
            parallel_target_branch: self.parallel_target_branch,
            notify_on_complete: self.agent_overrides.notify_on_complete,
            notify_sound: self.agent_overrides.notify_sound,
            lfs_check: self.agent_overrides.lfs_check.unwrap_or(false),
            no_progress: self.agent_overrides.no_progress.unwrap_or(false),
            execution_timings: self.setup.execution_timings.as_ref(),
            plugins: Some(self.plugins),
        }
    }
}
