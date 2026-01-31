//! Scan prompt loading and rendering.
//!
//! Responsibilities: load the scan prompt template, render user focus and scan mode, and apply
//! project-type guidance where enabled.
//! Not handled: task creation logic, queue mutations, or phase-specific prompt composition.
//! Invariants/assumptions: required placeholders exist and empty user focus normalizes to "(none)".

use super::registry::{load_prompt_template, prompt_template, PromptTemplateId};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::cli::scan::ScanMode;
use crate::contracts::{Config, ProjectType};
use anyhow::Result;

/// Mode-specific guidance for maintenance scan mode (default).
const MAINTENANCE_MODE_GUIDANCE: &str = r#"You are a scan-only agent for this repository.

Perform an agentic code review to find bugs, workflow gaps, design flaws, high-leverage reliability and UX fixes, flaky behavior, and safety issues.

Focus on: correctness, security, performance regressions, reliability, repo rule violations, inconsistent or incomplete behavior, and maintainability problems that create real risk.

Prioritize correctness and safety over new features."#;

/// Mode-specific guidance for innovation scan mode.
const INNOVATION_MODE_GUIDANCE: &str = r#"You are a scan-only agent for this repository.

Perform a feature discovery scan to identify enhancement opportunities, feature gaps, use-case completeness issues, and opportunities for innovative new features.

Focus on: missing features for specific use-cases, user workflow improvements, competitive gaps (only if you can cite concrete evidence), feature completeness, and strategic additions that increase user value.

Prioritize new capabilities and user value over maintenance tasks."#;

pub(crate) fn load_scan_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::Scan)
}

pub(crate) fn render_scan_prompt(
    template: &str,
    user_focus: &str,
    mode: ScanMode,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::Scan);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };

    // Select mode-specific guidance
    let mode_guidance = match mode {
        ScanMode::Maintenance => MAINTENANCE_MODE_GUIDANCE,
        ScanMode::Innovation => INNOVATION_MODE_GUIDANCE,
    };

    let expanded = super::util::expand_variables(template, config)?;
    let base = apply_project_type_guidance_if_needed(
        &expanded,
        project_type,
        template_meta.project_type_guidance,
    );
    let rendered = base
        .replace("{{MODE_GUIDANCE}}", mode_guidance)
        .replace("{{USER_FOCUS}}", focus);
    let safe_focus = escape_placeholder_like_text(focus);
    let safe_mode_guidance = escape_placeholder_like_text(mode_guidance);
    let rendered_for_validation = base
        .replace("{{MODE_GUIDANCE}}", safe_mode_guidance.trim())
        .replace("{{USER_FOCUS}}", safe_focus.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
