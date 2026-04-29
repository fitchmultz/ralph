//! Worker prompt preview builder.
//!
//! Purpose:
//! - Worker prompt preview builder.
//!
//! Responsibilities:
//! - Resolve worker preview task selection and iteration settings.
//! - Build single-phase and phase-specific worker prompt previews.
//! - Load preview-only cached plan/final-response inputs when needed.
//!
//! Not handled here:
//! - CLI argument parsing.
//! - Template management commands.
//! - Actual runner invocation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview rendering mirrors runtime prompt composition as closely as possible.
//! - Phase 2 previews may use a placeholder plan when cache is missing.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::contracts::ProjectType;
use crate::promptflow::{self, PromptPolicy};
use crate::{config, prompts, queue};

use super::source::worker_template_source;
use super::types::{WorkerMode, WorkerPromptOptions};

pub fn build_worker_prompt(
    resolved: &config::Resolved,
    opts: WorkerPromptOptions,
) -> Result<String> {
    let task_id = resolve_worker_task_id(resolved, opts.task_id.clone())?;
    validate_iterations(opts.iterations, opts.iteration_index)?;

    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let base_prompt =
        prompts::render_worker_prompt(&template, &task_id, project_type, &resolved.config)?;
    let base_prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &base_prompt, &resolved.config)?;

    let policy = PromptPolicy {
        repoprompt_plan_required: opts.repoprompt_plan_required,
        repoprompt_tool_injection: opts.repoprompt_tool_injection,
    };
    let is_followup = opts.iteration_index > 1;
    let is_final_iteration = opts.iteration_index == opts.iterations;
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

    let configured_phases = resolved.config.agent.phases.unwrap_or(2);
    let total_phases = match opts.mode {
        WorkerMode::Phase3 => 3,
        WorkerMode::Single => 1,
        _ => configured_phases.clamp(2, 3),
    };

    let load_completion_checklist = || -> Result<String> {
        let template = prompts::load_completion_checklist(&resolved.repo_root)?;
        prompts::render_completion_checklist(&template, &task_id, &resolved.config, false)
    };

    let prompt = match opts.mode {
        WorkerMode::Phase1 => {
            let phase1_template = prompts::load_worker_phase1_prompt(&resolved.repo_root)?;
            promptflow::build_phase1_prompt(
                &phase1_template,
                &base_prompt,
                iteration_context,
                promptflow::PHASE1_TASK_REFRESH_REQUIRED_INSTRUCTION,
                &task_id,
                total_phases,
                &policy,
                &resolved.config,
            )?
        }
        WorkerMode::Phase2 => build_phase2_prompt(
            resolved,
            &task_id,
            &base_prompt,
            &policy,
            &opts,
            total_phases,
            iteration_context,
            iteration_completion_block,
            &load_completion_checklist,
        )?,
        WorkerMode::Phase3 => {
            let review_template = prompts::load_code_review_prompt(&resolved.repo_root)?;
            let review_body = prompts::render_code_review_prompt(
                &review_template,
                &task_id,
                project_type,
                &resolved.config,
            )?;
            let completion_checklist = load_completion_checklist()?;
            let phase3_template = prompts::load_worker_phase3_prompt(&resolved.repo_root)?;
            let phase2_final_response =
                load_phase2_final_response_for_phase3(&resolved.repo_root, &task_id);
            promptflow::build_phase3_prompt(
                &phase3_template,
                &base_prompt,
                &review_body,
                &phase2_final_response,
                &task_id,
                &completion_checklist,
                iteration_context,
                iteration_completion_block,
                phase3_completion_guidance,
                total_phases,
                &policy,
                &resolved.config,
            )?
        }
        WorkerMode::Single => {
            let completion_checklist = load_completion_checklist()?;
            let single_template = prompts::load_worker_single_phase_prompt(&resolved.repo_root)?;
            promptflow::build_single_phase_prompt(
                &single_template,
                &base_prompt,
                &completion_checklist,
                iteration_context,
                iteration_completion_block,
                &task_id,
                &policy,
                &resolved.config,
            )?
        }
    };

    if !opts.explain {
        return Ok(prompt);
    }

    Ok(format!(
        "{}{}",
        explain_header(&resolved.repo_root, &task_id, &opts),
        prompt
    ))
}

pub(super) fn resolve_worker_task_id(
    resolved: &config::Resolved,
    task_id: Option<String>,
) -> Result<String> {
    if let Some(id) = task_id {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            bail!("--task-id was provided but is empty");
        }
        return Ok(trimmed.to_string());
    }

    if resolved.queue_path.exists() {
        let queue_file = queue::load_queue(&resolved.queue_path)
            .with_context(|| format!("read {}", resolved.queue_path.display()))?;

        let done_file = if resolved.done_path.exists() {
            Some(
                queue::load_queue(&resolved.done_path)
                    .with_context(|| format!("read {}", resolved.done_path.display()))?,
            )
        } else {
            None
        };

        let options = queue::operations::RunnableSelectionOptions::new(false, true);
        if let Some(index) =
            queue::operations::select_runnable_task_index(&queue_file, done_file.as_ref(), options)
            && let Some(task) = queue_file.tasks.get(index)
        {
            return Ok(task.id.trim().to_string());
        }
    }

    bail!(
        "No doing/todo tasks found to infer a worker task id. Provide --task-id (e.g., RQ-0001) to preview the worker prompt."
    )
}

fn validate_iterations(iterations: u8, iteration_index: u8) -> Result<()> {
    if iterations == 0 {
        bail!("--iterations must be >= 1");
    }
    if iteration_index == 0 {
        bail!("--iteration-index must be >= 1");
    }
    if iteration_index > iterations {
        bail!(
            "--iteration-index ({}) cannot exceed --iterations ({})",
            iteration_index,
            iterations
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_phase2_prompt(
    resolved: &config::Resolved,
    task_id: &str,
    base_prompt: &str,
    policy: &PromptPolicy,
    opts: &WorkerPromptOptions,
    total_phases: u8,
    iteration_context: &str,
    iteration_completion_block: &str,
    load_completion_checklist: &dyn Fn() -> Result<String>,
) -> Result<String> {
    let plan_text = load_plan_text_for_phase2(
        &resolved.repo_root,
        task_id,
        opts.plan_text.clone(),
        opts.plan_file.clone(),
    )?;
    if total_phases == 3 {
        let handoff_template = prompts::load_phase2_handoff_checklist(&resolved.repo_root)?;
        let handoff_checklist =
            prompts::render_phase2_handoff_checklist(&handoff_template, task_id, &resolved.config)?;
        let phase2_template = prompts::load_worker_phase2_handoff_prompt(&resolved.repo_root)?;
        promptflow::build_phase2_handoff_prompt(
            &phase2_template,
            base_prompt,
            &plan_text,
            &handoff_checklist,
            iteration_context,
            iteration_completion_block,
            task_id,
            total_phases,
            policy,
            &resolved.config,
        )
    } else {
        let completion_checklist = load_completion_checklist()?;
        let phase2_template = prompts::load_worker_phase2_prompt(&resolved.repo_root)?;
        promptflow::build_phase2_prompt(
            &phase2_template,
            base_prompt,
            &plan_text,
            &completion_checklist,
            iteration_context,
            iteration_completion_block,
            task_id,
            total_phases,
            policy,
            &resolved.config,
        )
    }
}

fn explain_header(repo_root: &Path, task_id: &str, opts: &WorkerPromptOptions) -> String {
    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (worker)\n\n");
    header.push_str(&format!("- task_id: {}\n", task_id));
    header.push_str(&format!("- mode: {}\n", worker_mode_label(opts.mode)));
    header.push_str(&format!(
        "- repoprompt_plan_required: {}\n",
        opts.repoprompt_plan_required
    ));
    header.push_str(&format!(
        "- repoprompt_tool_injection: {}\n",
        opts.repoprompt_tool_injection
    ));
    header.push_str(&format!(
        "- iteration: {}/{}\n",
        opts.iteration_index, opts.iterations
    ));
    header.push_str(&format!(
        "- worker template source: {}\n",
        worker_template_source(repo_root)
    ));
    header.push_str("\n---\n\n");
    header
}

fn worker_mode_label(mode: WorkerMode) -> &'static str {
    match mode {
        WorkerMode::Phase1 => "phase1",
        WorkerMode::Phase2 => "phase2",
        WorkerMode::Phase3 => "phase3",
        WorkerMode::Single => "single",
    }
}

fn load_plan_text_for_phase2(
    repo_root: &Path,
    task_id: &str,
    plan_text: Option<String>,
    plan_file: Option<PathBuf>,
) -> Result<String> {
    if let Some(text) = plan_text {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            bail!("--plan-text was provided but is empty");
        }
        return Ok(trimmed.to_string());
    }

    if let Some(path) = plan_file {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read plan file {}", path.display()))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("Plan file is empty: {}", path.display());
        }
        return Ok(trimmed.to_string());
    }

    match promptflow::read_plan_cache(repo_root, task_id) {
        Ok(plan) => Ok(plan),
        Err(_) => {
            let cache_path = promptflow::plan_cache_path(repo_root, task_id);
            Ok(format!(
                "*No plan file found*\n\nNo plan file was found at {}. Please proceed with implementation based on the task requirements.",
                cache_path.display()
            ))
        }
    }
}

fn load_phase2_final_response_for_phase3(repo_root: &Path, task_id: &str) -> String {
    match promptflow::read_phase2_final_response_cache(repo_root, task_id) {
        Ok(text) => text,
        Err(error) => {
            log::warn!(
                "Phase 2 final response cache unavailable for {}: {}",
                task_id,
                error
            );
            "(Phase 2 final response unavailable; cache missing.)".to_string()
        }
    }
}
