//! Scan prompt loading and rendering.
//!
//! Responsibilities: load the scan prompt template, render user focus, and apply project-type
//! guidance where enabled.
//! Not handled: task creation logic, queue mutations, or phase-specific prompt composition.
//! Invariants/assumptions: required placeholders exist and empty user focus normalizes to "(none)".

use super::registry::{load_prompt_template, prompt_template, PromptTemplateId};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::contracts::{Config, ProjectType};
use anyhow::Result;

pub fn load_scan_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::Scan)
}

pub fn render_scan_prompt(
    template: &str,
    user_focus: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::Scan);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };

    let expanded = super::expand_variables(template, config)?;
    let base = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );
    let rendered = base.replace("{{USER_FOCUS}}", focus);
    let safe_focus = escape_placeholder_like_text(focus);
    let rendered_for_validation = base.replace("{{USER_FOCUS}}", safe_focus.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
