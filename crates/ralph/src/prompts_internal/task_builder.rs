//! Task builder prompt loading and rendering.
//!
//! Responsibilities: load the task builder template and render user request, tags, and scope.
//! Not handled: task creation, queue mutations, or phase-specific prompt composition.
//! Invariants/assumptions: required placeholders exist and user request is non-empty.

use super::registry::{PromptTemplateId, load_prompt_template, prompt_template};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::contracts::{Config, ProjectType};
use anyhow::{Result, bail};

pub(crate) fn load_task_builder_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::TaskBuilder)
}

pub(crate) fn render_task_builder_prompt(
    template: &str,
    user_request: &str,
    hint_tags: &str,
    hint_scope: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::TaskBuilder);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let request = user_request.trim();
    if request.is_empty() {
        bail!(
            "Missing request: user request must be non-empty. Provide a descriptive request for the task builder."
        );
    }

    let expanded = super::util::expand_variables(template, config)?;
    let base = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );
    let mut rendered = base.clone();
    rendered = rendered.replace("{{USER_REQUEST}}", request);
    rendered = rendered.replace("{{HINT_TAGS}}", hint_tags.trim());
    rendered = rendered.replace("{{HINT_SCOPE}}", hint_scope.trim());
    rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    let safe_request = escape_placeholder_like_text(request);
    let safe_hint_tags = escape_placeholder_like_text(hint_tags.trim());
    let safe_hint_scope = escape_placeholder_like_text(hint_scope.trim());
    let mut rendered_for_validation = base;
    rendered_for_validation =
        rendered_for_validation.replace("{{USER_REQUEST}}", safe_request.trim());
    rendered_for_validation =
        rendered_for_validation.replace("{{HINT_TAGS}}", safe_hint_tags.trim());
    rendered_for_validation =
        rendered_for_validation.replace("{{HINT_SCOPE}}", safe_hint_scope.trim());
    rendered_for_validation = rendered_for_validation.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
