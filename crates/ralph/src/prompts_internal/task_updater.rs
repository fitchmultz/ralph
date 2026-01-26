//! Task updater prompt loading and rendering.
//!
//! Responsibilities: load the task updater template and render task IDs and field updates.
//! Not handled: task persistence, queue mutations, or phase-specific prompt composition.
//! Invariants/assumptions: required placeholders exist and task IDs are non-empty.

use super::registry::{load_prompt_template, prompt_template, PromptTemplateId};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::contracts::{Config, ProjectType};
use anyhow::{bail, Result};

pub fn load_task_updater_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::TaskUpdater)
}

pub fn render_task_updater_prompt(
    template: &str,
    task_id: &str,
    fields: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::TaskUpdater);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    if task_id.trim().is_empty() {
        bail!("Missing task ID: task ID must be non-empty. Provide a valid task ID for the task updater.");
    }

    let expanded = super::expand_variables(template, config)?;
    let base = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );
    let mut rendered = base.clone();
    rendered = rendered.replace("{{TASK_ID}}", task_id.trim());
    rendered = rendered.replace("{{FIELDS_TO_UPDATE}}", fields.trim());
    let safe_task_id = escape_placeholder_like_text(task_id.trim());
    let safe_fields = escape_placeholder_like_text(fields.trim());
    let mut rendered_for_validation = base;
    rendered_for_validation = rendered_for_validation.replace("{{TASK_ID}}", safe_task_id.trim());
    rendered_for_validation =
        rendered_for_validation.replace("{{FIELDS_TO_UPDATE}}", safe_fields.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
