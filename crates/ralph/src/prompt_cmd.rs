//! Prompt inspection/preview commands.
//!
//! This module exists to make prompt compilation observable and auditable.
//! It renders the exact final prompt that would be sent to a runner for:
//! - worker (single-phase / phase1 / phase2)
//! - scan
//! - task builder
//!
//! The logic intentionally re-uses existing prompt rendering + wrappers so that
//! previews stay accurate as runtime behavior evolves.

use crate::config;
use crate::contracts::{ProjectType, TaskStatus};
use crate::promptflow::{self, PromptPolicy};
use crate::{prompts, queue};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const WORKER_OVERRIDE_PATH: &str = ".ralph/prompts/worker.md";
const SCAN_OVERRIDE_PATH: &str = ".ralph/prompts/scan.md";
const TASK_BUILDER_OVERRIDE_PATH: &str = ".ralph/prompts/task_builder.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    /// Show the prompt for phase 1 (planning).
    Phase1,
    /// Show the prompt for phase 2 (implementation). Requires plan text.
    Phase2,
    /// Show the prompt for phase 3 (code review).
    Phase3,
    /// Show the combined single-phase prompt (plan+implement).
    Single,
}

#[derive(Debug, Clone)]
pub struct WorkerPromptOptions {
    /// If None, we will attempt to pick the first todo task from the queue.
    pub task_id: Option<String>,
    pub mode: WorkerMode,
    /// RepoPrompt required decision already resolved (flags + config).
    pub repoprompt_required: bool,
    /// Total iteration count to simulate when rendering prompts.
    pub iterations: u8,
    /// 1-based iteration index to simulate when rendering prompts.
    pub iteration_index: u8,

    /// Optional explicit plan file for Phase 2.
    /// If omitted in Phase 2, we try the cached plan at `.ralph/cache/plans/{{TASK_ID}}.md`.
    pub plan_file: Option<PathBuf>,
    /// Optional inline plan override (takes precedence over plan_file/cache).
    pub plan_text: Option<String>,

    /// Print a small header explaining what was selected.
    pub explain: bool,
}

#[derive(Debug, Clone)]
pub struct ScanPromptOptions {
    pub focus: String,
    pub repoprompt_required: bool,
    pub explain: bool,
}

#[derive(Debug, Clone)]
pub struct TaskBuilderPromptOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub repoprompt_required: bool,
    pub explain: bool,
}

fn worker_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(WORKER_OVERRIDE_PATH).exists() {
        WORKER_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

fn scan_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(SCAN_OVERRIDE_PATH).exists() {
        SCAN_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

fn task_builder_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(TASK_BUILDER_OVERRIDE_PATH).exists() {
        TASK_BUILDER_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

/// Resolve a task id for worker prompt preview:
/// - If provided explicitly, use it.
/// - Else load queue and pick first todo.
/// - Else error with a clear message.
fn resolve_worker_task_id(resolved: &config::Resolved, task_id: Option<String>) -> Result<String> {
    if let Some(id) = task_id {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            bail!("--task-id was provided but is empty");
        }
        return Ok(trimmed.to_string());
    }

    // Best-effort: mirror runtime selection.
    // Runtime prefers resuming a `doing` task, otherwise the first runnable `todo`.
    if resolved.queue_path.exists() {
        let queue_file = queue::load_queue(&resolved.queue_path)
            .with_context(|| format!("read {}", resolved.queue_path.display()))?;

        if let Some(task) = queue_file
            .tasks
            .iter()
            .find(|t| t.status == TaskStatus::Doing)
        {
            return Ok(task.id.trim().to_string());
        }

        let done_file = if resolved.done_path.exists() {
            Some(
                queue::load_queue(&resolved.done_path)
                    .with_context(|| format!("read {}", resolved.done_path.display()))?,
            )
        } else {
            None
        };

        if let Some(task) = queue::next_runnable_task(&queue_file, done_file.as_ref()) {
            return Ok(task.id.trim().to_string());
        }
    }

    bail!(
        "No doing/todo tasks found to infer a worker task id. Provide --task-id (e.g., RQ-0001) to preview the worker prompt."
    );
}

/// Load plan text for Phase 2 prompt preview.
///
/// NOTE: This function is ONLY used by the `ralph prompt` command for preview/inspection.
/// Actual runtime execution (`ralph run`) extracts the plan directly from Phase 1 output
/// and will error if no plan exists. This function uses a placeholder when missing
/// to allow previewing Phase 2 prompts even when no cached plan exists.
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

    // For preview command only: if cache is missing, use placeholder instead of erroring.
    // Runtime execution will still error appropriately since it extracts plan from Phase 1 output.
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
        Err(err) => {
            log::warn!(
                "Phase 2 final response cache unavailable for {}: {}",
                task_id,
                err
            );
            "(Phase 2 final response unavailable; cache missing.)".to_string()
        }
    }
}

pub fn build_worker_prompt(
    resolved: &config::Resolved,
    opts: WorkerPromptOptions,
) -> Result<String> {
    let task_id = resolve_worker_task_id(resolved, opts.task_id)?;
    if opts.iterations == 0 {
        bail!("--iterations must be >= 1");
    }
    if opts.iteration_index == 0 {
        bail!("--iteration-index must be >= 1");
    }
    if opts.iteration_index > opts.iterations {
        bail!(
            "--iteration-index ({}) cannot exceed --iterations ({})",
            opts.iteration_index,
            opts.iterations
        );
    }

    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let base_prompt =
        prompts::render_worker_prompt(&template, &task_id, project_type, &resolved.config)?;

    let policy = PromptPolicy {
        require_repoprompt: opts.repoprompt_required,
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
        prompts::render_completion_checklist(&template, &task_id, &resolved.config)
    };

    let prompt = match opts.mode {
        WorkerMode::Phase1 => {
            let phase1_template = prompts::load_worker_phase1_prompt(&resolved.repo_root)?;
            promptflow::build_phase1_prompt(
                &phase1_template,
                &base_prompt,
                iteration_context,
                &task_id,
                total_phases,
                &policy,
                &resolved.config,
            )?
        }
        WorkerMode::Phase2 => {
            let plan_text = load_plan_text_for_phase2(
                &resolved.repo_root,
                &task_id,
                opts.plan_text,
                opts.plan_file,
            )?;
            if total_phases == 3 {
                let handoff_template = prompts::load_phase2_handoff_checklist(&resolved.repo_root)?;
                let handoff_checklist =
                    prompts::render_phase2_handoff_checklist(&handoff_template, &resolved.config)?;
                let phase2_template =
                    prompts::load_worker_phase2_handoff_prompt(&resolved.repo_root)?;
                promptflow::build_phase2_handoff_prompt(
                    &phase2_template,
                    &base_prompt,
                    &plan_text,
                    &handoff_checklist,
                    iteration_context,
                    iteration_completion_block,
                    &task_id,
                    total_phases,
                    &policy,
                    &resolved.config,
                )?
            } else {
                let completion_checklist = load_completion_checklist()?;
                let phase2_template = prompts::load_worker_phase2_prompt(&resolved.repo_root)?;
                promptflow::build_phase2_prompt(
                    &phase2_template,
                    &base_prompt,
                    &plan_text,
                    &completion_checklist,
                    iteration_context,
                    iteration_completion_block,
                    &task_id,
                    total_phases,
                    &policy,
                    &resolved.config,
                )?
            }
        }
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

    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (worker)\n\n");
    header.push_str(&format!("- task_id: {}\n", task_id));
    header.push_str(&format!(
        "- mode: {}\n",
        match opts.mode {
            WorkerMode::Phase1 => "phase1",
            WorkerMode::Phase2 => "phase2",
            WorkerMode::Phase3 => "phase3",
            WorkerMode::Single => "single",
        }
    ));
    header.push_str(&format!(
        "- repoprompt_required: {}\n",
        opts.repoprompt_required
    ));
    header.push_str(&format!(
        "- iteration: {}/{}\n",
        opts.iteration_index, opts.iterations
    ));
    header.push_str(&format!(
        "- worker template source: {}\n",
        worker_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}

pub fn build_scan_prompt(resolved: &config::Resolved, opts: ScanPromptOptions) -> Result<String> {
    let template = prompts::load_scan_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let rendered =
        prompts::render_scan_prompt(&template, &opts.focus, project_type, &resolved.config)?;
    let prompt = prompts::wrap_with_repoprompt_requirement(&rendered, opts.repoprompt_required);

    if !opts.explain {
        return Ok(prompt);
    }

    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (scan)\n\n");
    header.push_str(&format!(
        "- focus: {}\n",
        if opts.focus.trim().is_empty() {
            "(none)"
        } else {
            opts.focus.trim()
        }
    ));
    header.push_str(&format!(
        "- repoprompt_required: {}\n",
        opts.repoprompt_required
    ));
    header.push_str(&format!(
        "- scan template source: {}\n",
        scan_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}

pub fn build_task_builder_prompt(
    resolved: &config::Resolved,
    opts: TaskBuilderPromptOptions,
) -> Result<String> {
    let request = opts.request.trim();
    if request.is_empty() {
        bail!("Missing request: task builder prompt preview requires a non-empty request.");
    }

    let template = prompts::load_task_builder_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let rendered = prompts::render_task_builder_prompt(
        &template,
        request,
        &opts.hint_tags,
        &opts.hint_scope,
        project_type,
        &resolved.config,
    )?;
    let prompt = prompts::wrap_with_repoprompt_requirement(&rendered, opts.repoprompt_required);

    if !opts.explain {
        return Ok(prompt);
    }

    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (task builder)\n\n");
    header.push_str(&format!("- request: {}\n", request));
    header.push_str(&format!(
        "- hint_tags: {}\n",
        if opts.hint_tags.trim().is_empty() {
            "(empty)"
        } else {
            opts.hint_tags.trim()
        }
    ));
    header.push_str(&format!(
        "- hint_scope: {}\n",
        if opts.hint_scope.trim().is_empty() {
            "(empty)"
        } else {
            opts.hint_scope.trim()
        }
    ));
    header.push_str(&format!(
        "- repoprompt_required: {}\n",
        opts.repoprompt_required
    ));
    header.push_str(&format!(
        "- task builder template source: {}\n",
        task_builder_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}
