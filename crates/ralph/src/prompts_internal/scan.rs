//! Scan prompt loading and rendering.
//!
//! Purpose:
//! - Scan prompt loading and rendering.
//!
//! Responsibilities: load the scan prompt template, render user focus and scan mode, and apply
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!   project-type guidance where enabled.
//!
//! Not handled: task creation logic, queue mutations, or phase-specific prompt composition.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions: required placeholders exist and empty user focus normalizes to "(none)".

use super::registry::{PromptTemplateId, load_prompt_template, prompt_template};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::cli::scan::ScanMode;
use crate::contracts::{Config, ProjectType, ScanPromptVersion};
use anyhow::Result;

/// Load scan prompt template based on version and mode.
pub(crate) fn load_scan_prompt(
    repo_root: &std::path::Path,
    version: ScanPromptVersion,
    mode: ScanMode,
) -> Result<String> {
    let template_id = match (version, mode) {
        (ScanPromptVersion::V1, ScanMode::Maintenance) => PromptTemplateId::ScanMaintenanceV1,
        (ScanPromptVersion::V1, ScanMode::Innovation) => PromptTemplateId::ScanInnovationV1,
        (ScanPromptVersion::V1, ScanMode::General) => PromptTemplateId::ScanGeneralV2,
        (ScanPromptVersion::V2, ScanMode::Maintenance) => PromptTemplateId::ScanMaintenanceV2,
        (ScanPromptVersion::V2, ScanMode::Innovation) => PromptTemplateId::ScanInnovationV2,
        (ScanPromptVersion::V2, ScanMode::General) => PromptTemplateId::ScanGeneralV2,
    };
    load_prompt_template(repo_root, template_id)
}

/// Render scan prompt with version-aware placeholder replacement.
pub(crate) fn render_scan_prompt(
    template: &str,
    user_focus: &str,
    mode: ScanMode,
    version: ScanPromptVersion,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_id = match (version, mode) {
        (ScanPromptVersion::V1, ScanMode::Maintenance) => PromptTemplateId::ScanMaintenanceV1,
        (ScanPromptVersion::V1, ScanMode::Innovation) => PromptTemplateId::ScanInnovationV1,
        (ScanPromptVersion::V1, ScanMode::General) => PromptTemplateId::ScanGeneralV2,
        (ScanPromptVersion::V2, ScanMode::Maintenance) => PromptTemplateId::ScanMaintenanceV2,
        (ScanPromptVersion::V2, ScanMode::Innovation) => PromptTemplateId::ScanInnovationV2,
        (ScanPromptVersion::V2, ScanMode::General) => PromptTemplateId::ScanGeneralV2,
    };
    let template_meta = prompt_template(template_id);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };

    let expanded_template = super::util::expand_variables(template, config)?;
    let base = apply_project_type_guidance_if_needed(
        &expanded_template,
        project_type,
        template_meta.project_type_guidance,
    );
    let rendered = base.replace("{{USER_FOCUS}}", focus);
    let safe_focus = escape_placeholder_like_text(focus);
    let rendered_for_validation = base.replace("{{USER_FOCUS}}", safe_focus.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
