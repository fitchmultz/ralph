//! Scan prompt rendering and loading tests.
//!
//! Responsibilities: validate scan prompt rendering for different modes and fallback behavior.
//! Not handled: worker prompts, task builder, or phase-specific rendering.
//! Invariants/assumptions: maintenance mode includes code review guidance; innovation mode includes
//! feature discovery guidance.

use super::*;

#[test]
fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
    let template = "FOCUS:\n{{USER_FOCUS}}\nMODE:\n{{MODE_GUIDANCE}}\n";
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
    let template = "FOCUS:\n{{USER_FOCUS}}\nMODE:\n{{MODE_GUIDANCE}}\n";
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
    let template = "FOCUS:\n{{USER_FOCUS}}\nMODE:\n{{MODE_GUIDANCE}}\n";
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
    assert!(!rendered.contains("{{MODE_GUIDANCE}}"));
    Ok(())
}

#[test]
fn render_scan_prompt_maintenance_mode_includes_maintenance_guidance() -> Result<()> {
    let template = "FOCUS:\n{{USER_FOCUS}}\nMODE:\n{{MODE_GUIDANCE}}\n";
    let config = default_config();
    let rendered = render_scan_prompt(
        template,
        "",
        ScanMode::Maintenance,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("code review"));
    assert!(rendered.contains("code hygiene"));
    assert!(!rendered.contains("{{MODE_GUIDANCE}}"));
    Ok(())
}

#[test]
fn default_scan_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_scan_prompt(dir.path())?;
    assert!(prompt.contains("ralph queue next-id"));
    assert!(!prompt.contains("ralph queue next` for each new task ID"));
    Ok(())
}
