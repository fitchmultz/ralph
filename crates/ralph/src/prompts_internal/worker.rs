//! Worker prompt loading and rendering.
//!
//! Responsibilities: load the base worker template and render task-scoped instructions with
//! project guidance.
//! Not handled: phase-specific prompts, checklist prompts, or RepoPrompt block assembly.
//! Invariants/assumptions: task IDs are non-empty when rendering and templates use `{{...}}` tokens.

use super::registry::{load_prompt_template, prompt_template, PromptTemplateId};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders,
};
use crate::contracts::{Config, ProjectType};
use anyhow::Result;

pub fn load_worker_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::Worker)
}

pub fn render_worker_prompt(
    template: &str,
    task_id: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::Worker);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let id = task_id.trim();
    if id.is_empty() {
        anyhow::bail!("Missing task id: worker prompt requires a non-empty task id.");
    }

    let expanded = super::expand_variables(template, config)?;
    let mut rendered = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );
    rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    rendered = rendered.replace("{{TASK_ID}}", id);
    ensure_no_unresolved_placeholders(&rendered, template_meta.label)?;
    Ok(rendered)
}
