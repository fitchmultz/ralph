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
use crate::contracts::ProjectType;
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

    /// Optional explicit plan file for Phase 2.
    /// If omitted in Phase 2, we try the cached plan at `.ralph/cache/plans/<TASK_ID>.md`.
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

    // Best-effort: pick first todo task from queue to preview "as it would run".
    if resolved.queue_path.exists() {
        let queue_file = queue::load_queue(&resolved.queue_path)
            .with_context(|| format!("read {}", resolved.queue_path.display()))?;
        if let Some(task) = queue::next_todo_task(&queue_file) {
            return Ok(task.id.trim().to_string());
        }
    }

    bail!(
        "No todo tasks found to infer a worker task id. Provide --task-id (e.g., RQ-0001) to preview the worker prompt."
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

pub fn build_worker_prompt(
    resolved: &config::Resolved,
    opts: WorkerPromptOptions,
) -> Result<String> {
    let task_id = resolve_worker_task_id(resolved, opts.task_id)?;

    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let base_prompt = prompts::render_worker_prompt(&template, project_type, &resolved.config)?;

    let policy = PromptPolicy {
        require_repoprompt: opts.repoprompt_required,
    };

    let prompt = match opts.mode {
        WorkerMode::Phase1 => promptflow::build_phase1_prompt(&base_prompt, &task_id, &policy),
        WorkerMode::Phase2 => {
            let plan_text = load_plan_text_for_phase2(
                &resolved.repo_root,
                &task_id,
                opts.plan_text,
                opts.plan_file,
            )?;
            promptflow::build_phase2_prompt(&plan_text, &policy)
        }
        WorkerMode::Single => {
            promptflow::build_single_phase_prompt(&base_prompt, &task_id, &policy)
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
            WorkerMode::Single => "single",
        }
    ));
    header.push_str(&format!(
        "- repoprompt_required: {}\n",
        opts.repoprompt_required
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

/// Helper used by CLI to match rp flag semantics consistently:
/// - --rp-on => true
/// - --rp-off => false
/// - neither => config.agent.require_repoprompt (default false)
pub fn resolve_rp_required(rp_on: bool, rp_off: bool, resolved: &config::Resolved) -> bool {
    if rp_on {
        return true;
    }
    if rp_off {
        return false;
    }
    resolved.config.agent.require_repoprompt.unwrap_or(false)
}
