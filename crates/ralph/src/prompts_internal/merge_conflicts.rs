//! Merge conflict prompt loading and rendering.
//!
//! Responsibilities: load the merge conflict template and render the conflicted file list.
//! Not handled: running conflict resolution or invoking runners (see `commands::run::parallel`).
//! Invariants/assumptions: required placeholders exist and conflict file list is non-empty.

use super::registry::{PromptTemplateId, load_prompt_template, prompt_template};
use super::util::{
    ensure_no_unresolved_placeholders, ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::contracts::Config;
use anyhow::{Result, bail};

pub(crate) fn load_merge_conflict_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::MergeConflicts)
}

pub(crate) fn render_merge_conflict_prompt(
    template: &str,
    conflict_files: &[String],
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::MergeConflicts);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let formatted = format_conflict_list(conflict_files)?;
    let expanded = super::util::expand_variables(template, config)?;
    let rendered = expanded.replace("{{CONFLICT_FILES}}", &formatted);

    let safe_files: Vec<String> = conflict_files
        .iter()
        .map(|item| escape_placeholder_like_text(item))
        .collect();
    let safe_formatted = format_conflict_list(&safe_files)?;
    let rendered_for_validation = expanded.replace("{{CONFLICT_FILES}}", &safe_formatted);
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;

    Ok(rendered)
}

fn format_conflict_list(conflict_files: &[String]) -> Result<String> {
    let entries: Vec<String> = conflict_files
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| format!("- {}", item))
        .collect();
    if entries.is_empty() {
        bail!("Conflict file list must be non-empty.");
    }
    Ok(entries.join("\n"))
}
