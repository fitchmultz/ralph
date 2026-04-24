//! Task-builder prompt preview builder.
//!
//! Purpose:
//! - Task-builder prompt preview builder.
//!
//! Responsibilities:
//! - Build task-builder prompt previews using production rendering helpers.
//! - Validate request input and optional explain headers.
//!
//! Not handled here:
//! - Worker or scan prompt preview logic.
//! - Template management commands.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task-builder previews require a non-empty request.
//! - Explain headers only wrap the rendered prompt.

use anyhow::{Result, bail};

use crate::contracts::ProjectType;
use crate::{config, prompts};

use super::source::task_builder_template_source;
use super::types::TaskBuilderPromptOptions;

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
    let prompt =
        prompts::wrap_with_repoprompt_requirement(&rendered, opts.repoprompt_tool_injection);
    let prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

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
        "- repoprompt_tool_injection: {}\n",
        opts.repoprompt_tool_injection
    ));
    header.push_str(&format!(
        "- task builder template source: {}\n",
        task_builder_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}
