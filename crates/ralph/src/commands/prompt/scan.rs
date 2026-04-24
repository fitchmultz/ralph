//! Scan prompt preview builder.
//!
//! Purpose:
//! - Scan prompt preview builder.
//!
//! Responsibilities:
//! - Build scan prompt previews using production scan rendering.
//! - Add optional explain headers describing selected sources and flags.
//!
//! Not handled here:
//! - Worker or task-builder prompt construction.
//! - Template management operations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Scan prompt rendering uses the configured scan prompt version.
//! - Explain headers never mutate the core rendered prompt content.

use anyhow::Result;

use crate::contracts::ProjectType;
use crate::{config, prompts};

use super::source::scan_template_source;
use super::types::ScanPromptOptions;

pub fn build_scan_prompt(resolved: &config::Resolved, opts: ScanPromptOptions) -> Result<String> {
    let scan_version = resolved
        .config
        .agent
        .scan_prompt_version
        .unwrap_or_default();
    let template = prompts::load_scan_prompt(&resolved.repo_root, scan_version, opts.mode)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let rendered = prompts::render_scan_prompt(
        &template,
        &opts.focus,
        opts.mode,
        scan_version,
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
        "- repoprompt_tool_injection: {}\n",
        opts.repoprompt_tool_injection
    ));
    header.push_str(&format!(
        "- scan template source: {}\n",
        scan_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}
