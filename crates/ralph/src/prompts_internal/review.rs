//! Review prompt loading and rendering (code review, completion checklist, iteration checklist,
//! phase2 handoff).
//!
//! Responsibilities: load review-related templates, render task-scoped content, and apply
//! project-type guidance for code review prompts.
//! Not handled: worker phase prompt composition, queue updates, or RepoPrompt instruction injection.
//! Invariants/assumptions: required placeholders are present and task IDs are non-empty where needed.

use super::registry::{load_prompt_template, prompt_template, PromptTemplateId};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders,
};
use crate::contracts::{Config, ProjectType};
use anyhow::{bail, Result};

pub fn load_completion_checklist(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::CompletionChecklist)
}

pub fn load_code_review_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::CodeReview)
}

pub fn load_phase2_handoff_checklist(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::Phase2HandoffChecklist)
}

pub fn load_iteration_checklist(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::IterationChecklist)
}

pub fn render_completion_checklist(
    template: &str,
    task_id: &str,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::CompletionChecklist);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: completion checklist requires a non-empty task id.");
    }

    let expanded = super::expand_variables(template, config)?;
    let rendered = expanded.replace("{{TASK_ID}}", id);
    ensure_no_unresolved_placeholders(&rendered, template_meta.label)?;
    Ok(rendered)
}

pub fn render_phase2_handoff_checklist(template: &str, config: &Config) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::Phase2HandoffChecklist);
    let expanded = super::expand_variables(template, config)?;
    ensure_no_unresolved_placeholders(&expanded, template_meta.label)?;
    Ok(expanded)
}

pub fn render_iteration_checklist(
    template: &str,
    task_id: &str,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::IterationChecklist);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: iteration checklist requires a non-empty task id.");
    }

    let expanded = super::expand_variables(template, config)?;
    let rendered = expanded.replace("{{TASK_ID}}", id);
    ensure_no_unresolved_placeholders(&rendered, template_meta.label)?;
    Ok(rendered)
}

pub fn render_code_review_prompt(
    template: &str,
    task_id: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::CodeReview);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: code review prompt requires a non-empty task id.");
    }

    let expanded = super::expand_variables(template, config)?;
    let mut rendered = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );

    rendered = rendered.replace("{{TASK_ID}}", id);

    ensure_no_unresolved_placeholders(&rendered, template_meta.label)?;

    Ok(rendered)
}
