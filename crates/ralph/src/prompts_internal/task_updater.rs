//! Task updater prompt loading and rendering.

use super::util::{
    ensure_no_unresolved_placeholders, escape_placeholder_like_text, load_prompt_with_fallback,
    project_type_guidance,
};
use crate::contracts::{Config, ProjectType};
use anyhow::{bail, Result};

const TASK_UPDATER_PROMPT_REL_PATH: &str = ".ralph/prompts/task_updater.md";

const DEFAULT_TASK_UPDATER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/task_updater.md"
));

pub fn load_task_updater_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        TASK_UPDATER_PROMPT_REL_PATH,
        DEFAULT_TASK_UPDATER_PROMPT,
        "task updater",
    )
}

pub fn render_task_updater_prompt(
    template: &str,
    task_id: &str,
    fields: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    if !template.contains("{{TASK_ID}}") {
        bail!("Template error: task updater prompt template is missing required '{{TASK_ID}}' placeholder. Ensure template in .ralph/prompts/task_updater.md includes this placeholder.");
    }
    if !template.contains("{{FIELDS_TO_UPDATE}}") {
        bail!("Template error: task updater prompt template is missing required '{{FIELDS_TO_UPDATE}}' placeholder. Ensure template includes this placeholder.");
    }

    if task_id.trim().is_empty() {
        bail!("Missing task ID: task ID must be non-empty. Provide a valid task ID for the task updater.");
    }

    let expanded = super::expand_variables(template, config)?;
    let guidance = project_type_guidance(project_type);
    let mut rendered = if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    };
    rendered = rendered.replace("{{TASK_ID}}", task_id.trim());
    rendered = rendered.replace("{{FIELDS_TO_UPDATE}}", fields.trim());
    let safe_task_id = escape_placeholder_like_text(task_id.trim());
    let safe_fields = escape_placeholder_like_text(fields.trim());
    let mut rendered_for_validation = if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    };
    rendered_for_validation = rendered_for_validation.replace("{{TASK_ID}}", safe_task_id.trim());
    rendered_for_validation =
        rendered_for_validation.replace("{{FIELDS_TO_UPDATE}}", safe_fields.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, "task updater")?;
    Ok(rendered)
}
