//! Scan prompt rendering and loading tests.
//!
//! Responsibilities: validate scan prompt rendering for different modes and fallback behavior.
//! Not handled: worker prompts, task builder, or phase-specific rendering.
//! Invariants/assumptions: maintenance mode includes code review guidance; innovation mode includes
//! feature discovery guidance.

use super::*;

#[test]
fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
    let template = "{{MODE_GUIDANCE}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "hello world",
        ScanMode::Maintenance,
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
    let template = "{{MODE_GUIDANCE}}";
    let config = default_config();
    let focus = "see {{config.agent.model}} here";
    let rendered = render_scan_prompt(
        template,
        focus,
        ScanMode::Maintenance,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains(focus));
    Ok(())
}

#[test]
fn render_scan_prompt_innovation_mode_includes_innovation_guidance() -> Result<()> {
    let template = "{{MODE_GUIDANCE}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Innovation,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("feature discovery"));
    assert!(rendered.contains("enhancement opportunities"));
    assert!(rendered.contains(r#"custom_fields: {"scan_agent": "scan-innovation"}"#));
    assert!(!rendered.contains(r#"agent: "scan-innovation""#));
    assert!(!rendered.contains("{{MODE_GUIDANCE}}"));
    Ok(())
}

#[test]
fn render_scan_prompt_maintenance_mode_includes_maintenance_guidance() -> Result<()> {
    let template = "{{MODE_GUIDANCE}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Maintenance,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("code review"));
    assert!(rendered.contains("MAINTENANCE TASK FILTER"));
    assert!(rendered.contains(r#"custom_fields: {"scan_agent": "scan-maintenance"}"#));
    assert!(!rendered.contains(r#"agent: "scan-maintenance""#));
    assert!(!rendered.contains("{{MODE_GUIDANCE}}"));
    Ok(())
}

#[test]
fn default_scan_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let template = load_scan_prompt(dir.path())?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Innovation,
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
    let template = load_scan_prompt(dir.path())?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Maintenance,
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
    let template = load_scan_prompt(dir.path())?;
    let config = default_config();
    let rendered = render_scan_prompt(
        &template,
        "",
        ScanMode::Innovation,
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
    let template = "{{MODE_GUIDANCE}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "   ",
        ScanMode::Maintenance,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("# FOCUS\n(none)"));
    Ok(())
}

#[test]
fn render_scan_prompt_replaces_project_type_guidance_placeholder() -> Result<()> {
    let template = "{{MODE_GUIDANCE}}";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Maintenance,
        ProjectType::Code,
        &config,
    )?;
    assert!(!rendered.contains("{{PROJECT_TYPE_GUIDANCE}}"));
    assert!(rendered.contains("## PROJECT TYPE: CODE"));
    Ok(())
}
