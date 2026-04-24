//! Scan prompt rendering and loading tests.
//!
//! Purpose:
//! - Scan prompt rendering and loading tests.
//!
//! Responsibilities: validate scan prompt rendering for different modes and fallback behavior.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled: worker prompts, task builder, or phase-specific rendering.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions: maintenance mode includes code review guidance; innovation mode includes
//! feature discovery guidance.

use super::*;
use crate::contracts::ScanPromptVersion;

#[test]
fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
    let template = "{{PROJECT_TYPE_GUIDANCE}} {{USER_FOCUS}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "hello world",
        ScanMode::Maintenance,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("hello world"));
    assert!(!rendered.contains("{{USER_FOCUS}}"));
    assert!(!rendered.contains("{{MODE_GUIDANCE}}"));
    Ok(())
}

#[test]
fn render_scan_prompt_allows_placeholder_like_focus() -> Result<()> {
    let template = "{{PROJECT_TYPE_GUIDANCE}} {{USER_FOCUS}}";
    let config = default_config();
    let focus = "see {{config.agent.model}} here";
    let rendered = render_scan_prompt(
        template,
        focus,
        ScanMode::Maintenance,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains(focus));
    Ok(())
}

#[test]
fn render_scan_prompt_innovation_mode_includes_innovation_guidance() -> Result<()> {
    let template = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/prompts/scan_innovation_v1.md"
    ));
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Innovation,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("feature discovery"));
    assert!(rendered.contains("enhancement opportunities"));
    assert!(rendered.contains(r#"custom_fields: {"scan_agent": "scan-innovation"}"#));
    assert!(!rendered.contains(r#"agent: "scan-innovation""#));
    Ok(())
}

#[test]
fn render_scan_prompt_maintenance_mode_includes_maintenance_guidance() -> Result<()> {
    let template = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/prompts/scan_maintenance_v1.md"
    ));
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Maintenance,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("code review"));
    assert!(rendered.contains("MAINTENANCE TASK FILTER"));
    assert!(rendered.contains(r#"custom_fields: {"scan_agent": "scan-maintenance"}"#));
    assert!(!rendered.contains(r#"agent: "scan-maintenance""#));
    Ok(())
}

#[test]
fn default_scan_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let template = load_scan_prompt(dir.path(), ScanPromptVersion::V1, ScanMode::Innovation)?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Innovation,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("ralph queue next-id"));
    assert!(
        rendered.contains("ralph queue next is NOT an ID generator")
            || rendered.contains("returns the next queued task, not a new ID"),
        "prompt should clarify the difference between 'next' and 'next-id'"
    );
    Ok(())
}

#[test]
fn default_scan_prompt_mentions_count_flag_for_multi_task() -> Result<()> {
    let dir = TempDir::new()?;
    let template = load_scan_prompt(dir.path(), ScanPromptVersion::V1, ScanMode::Maintenance)?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Maintenance,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    // Should mention --count for generating multiple IDs
    assert!(
        rendered.contains("next-id --count"),
        "prompt should mention next-id --count"
    );
    // Should warn that next-id does not reserve IDs
    assert!(
        rendered.contains("does NOT reserve IDs") || rendered.contains("does not reserve IDs"),
        "prompt should warn that next-id does not reserve IDs"
    );
    Ok(())
}

#[test]
fn default_scan_prompt_innovation_mode_mentions_count_flag() -> Result<()> {
    let dir = TempDir::new()?;
    let template = load_scan_prompt(dir.path(), ScanPromptVersion::V1, ScanMode::Innovation)?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Innovation,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    // Innovation mode should also have the --count guidance
    assert!(
        rendered.contains("next-id --count"),
        "innovation mode prompt should mention next-id --count"
    );
    assert!(
        rendered.contains("does NOT reserve IDs") || rendered.contains("does not reserve IDs"),
        "innovation mode prompt should warn that next-id does not reserve IDs"
    );
    Ok(())
}

#[test]
fn render_scan_prompt_empty_focus_defaults_to_none() -> Result<()> {
    let template = "{{PROJECT_TYPE_GUIDANCE}}\n# FOCUS\n{{USER_FOCUS}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "   ",
        ScanMode::Maintenance,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("# FOCUS\n(none)"));
    Ok(())
}

#[test]
fn render_scan_prompt_replaces_project_type_guidance_placeholder() -> Result<()> {
    let template = "{{PROJECT_TYPE_GUIDANCE}} {{USER_FOCUS}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Maintenance,
        ScanPromptVersion::V1,
        ProjectType::Code,
        &config,
    )?;
    assert!(!rendered.contains("{{PROJECT_TYPE_GUIDANCE}}"));
    assert!(rendered.contains("## PROJECT TYPE: CODE"));
    Ok(())
}

#[test]
fn default_scan_prompt_v2_maintenance_requires_queue_validation_safety() -> Result<()> {
    let dir = TempDir::new()?;
    let template = load_scan_prompt(dir.path(), ScanPromptVersion::V2, ScanMode::Maintenance)?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Maintenance,
        ScanPromptVersion::V2,
        ProjectType::Code,
        &config,
    )?;
    assert!(
        rendered.contains("VALIDATION SAFETY RULES"),
        "maintenance v2 prompt must include validation safety rules"
    );
    assert!(
        rendered.contains("Run `ralph queue validate` before finishing"),
        "maintenance v2 prompt must require queue validation"
    );
    assert!(
        rendered.contains("depends_on`)") || rendered.contains("depends_on"),
        "maintenance v2 prompt should include dependency relationship safety"
    );
    Ok(())
}

#[test]
fn default_scan_prompt_v2_innovation_requires_queue_validation_safety() -> Result<()> {
    let dir = TempDir::new()?;
    let template = load_scan_prompt(dir.path(), ScanPromptVersion::V2, ScanMode::Innovation)?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Innovation,
        ScanPromptVersion::V2,
        ProjectType::Code,
        &config,
    )?;
    assert!(
        rendered.contains("VALIDATION SAFETY RULES"),
        "innovation v2 prompt must include validation safety rules"
    );
    assert!(
        rendered.contains("Run `ralph queue validate` before finishing"),
        "innovation v2 prompt must require queue validation"
    );
    assert!(
        rendered.contains("depends_on`)") || rendered.contains("depends_on"),
        "innovation v2 prompt should include dependency relationship safety"
    );
    Ok(())
}
