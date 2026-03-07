//! Task decomposition prompt loading and rendering.
//!
//! Responsibilities:
//! - Load the task decomposition prompt template.
//! - Render source context and planner limits into a strict JSON-planning prompt.
//!
//! Not handled here:
//! - Queue mutation or task materialization.
//! - Runner invocation or planner output parsing.
//!
//! Invariants/assumptions:
//! - Rendered prompts must resolve all required placeholders.
//! - Source request text may contain placeholder-like syntax safely.

use super::registry::{PromptTemplateId, load_prompt_template, prompt_template};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::commands::task::DecompositionChildPolicy;
use crate::contracts::{Config, ProjectType};
use anyhow::Result;

pub(crate) fn load_task_decompose_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::TaskDecompose)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_task_decompose_prompt(
    template: &str,
    source_mode: &str,
    source_request: &str,
    source_task_json: &str,
    attach_target_json: &str,
    max_depth: u8,
    max_children: usize,
    max_nodes: usize,
    child_policy: DecompositionChildPolicy,
    with_dependencies: bool,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::TaskDecompose);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let expanded = super::util::expand_variables(template, config)?;
    let base = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );

    let mut rendered = base.clone();
    rendered = rendered.replace("{{SOURCE_MODE}}", source_mode.trim());
    rendered = rendered.replace("{{SOURCE_REQUEST}}", source_request.trim());
    rendered = rendered.replace("{{SOURCE_TASK_JSON}}", source_task_json.trim());
    rendered = rendered.replace("{{ATTACH_TARGET_JSON}}", attach_target_json.trim());
    rendered = rendered.replace("{{MAX_DEPTH}}", &max_depth.to_string());
    rendered = rendered.replace("{{MAX_CHILDREN}}", &max_children.to_string());
    rendered = rendered.replace("{{MAX_NODES}}", &max_nodes.to_string());
    rendered = rendered.replace(
        "{{CHILD_POLICY}}",
        &format!("{child_policy:?}").to_ascii_lowercase(),
    );
    rendered = rendered.replace(
        "{{WITH_DEPENDENCIES}}",
        if with_dependencies { "true" } else { "false" },
    );

    let mut rendered_for_validation = base;
    rendered_for_validation = rendered_for_validation.replace(
        "{{SOURCE_MODE}}",
        escape_placeholder_like_text(source_mode.trim()).trim(),
    );
    rendered_for_validation = rendered_for_validation.replace(
        "{{SOURCE_REQUEST}}",
        escape_placeholder_like_text(source_request.trim()).trim(),
    );
    rendered_for_validation = rendered_for_validation.replace(
        "{{SOURCE_TASK_JSON}}",
        escape_placeholder_like_text(source_task_json.trim()).trim(),
    );
    rendered_for_validation = rendered_for_validation.replace(
        "{{ATTACH_TARGET_JSON}}",
        escape_placeholder_like_text(attach_target_json.trim()).trim(),
    );
    rendered_for_validation =
        rendered_for_validation.replace("{{MAX_DEPTH}}", &max_depth.to_string());
    rendered_for_validation =
        rendered_for_validation.replace("{{MAX_CHILDREN}}", &max_children.to_string());
    rendered_for_validation =
        rendered_for_validation.replace("{{MAX_NODES}}", &max_nodes.to_string());
    rendered_for_validation = rendered_for_validation.replace(
        "{{CHILD_POLICY}}",
        &format!("{child_policy:?}").to_ascii_lowercase(),
    );
    rendered_for_validation = rendered_for_validation.replace(
        "{{WITH_DEPENDENCIES}}",
        if with_dependencies { "true" } else { "false" },
    );

    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
