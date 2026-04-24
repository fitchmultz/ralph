//! Prompt template management facade.
//!
//! Purpose:
//! - Prompt template management facade.
//!
//! Responsibilities:
//! - Expose prompt export, sync, diff, and version-tracking helpers.
//! - Centralize stable digest computation and version-file persistence.
//!
//! Not handled here:
//! - Prompt rendering or variable expansion.
//! - CLI argument parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Exported prompts live under `.ralph/prompts/`.
//! - Prompt version tracking is stored under `.ralph/cache/prompt_versions.json`.
//! - Digests use normalized-content SHA-256 with the `sha256:` prefix.

mod hash;
mod storage;
mod sync;
mod templates;

#[cfg(test)]
mod tests;

pub(crate) use hash::compute_hash;
#[allow(unused_imports)]
pub(crate) use storage::{
    PROMPT_VERSION_SCHEMA, PromptVersionInfo, TemplateVersion, load_version_info, save_version_info,
};
pub(crate) use sync::{check_sync_status, export_template, generate_diff};
pub(crate) use templates::{
    SyncStatus, all_template_ids, get_effective_content, get_embedded_content, list_templates,
    parse_template_name, template_file_name,
};

pub(crate) fn sync_template(
    repo_root: &std::path::Path,
    id: crate::prompts_internal::registry::PromptTemplateId,
    force: bool,
    ralph_version: &str,
) -> anyhow::Result<(bool, SyncStatus)> {
    sync::sync_template(repo_root, id, force, ralph_version)
}
