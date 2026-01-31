//! Worker prompt loading and rendering tests.
//!
//! Responsibilities: validate worker prompt loading, fallback behavior, and rendering.
//! Not handled: phase-specific worker prompts (see phases.rs), task builder, or scan prompts.
//! Invariants/assumptions: embedded defaults are available; temp directories are writable.

use super::*;

#[test]
fn render_worker_prompt_replaces_interactive_instructions() -> Result<()> {
    let template = "Hello\n{{INTERACTIVE_INSTRUCTIONS}}\n";
    let config = default_config();
    let rendered = render_worker_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(!rendered.contains("{{INTERACTIVE_INSTRUCTIONS}}"));
    Ok(())
}

#[test]
fn load_worker_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_prompt(dir.path())?;
    assert!(prompt.contains("# MISSION"));
    Ok(())
}

#[test]
fn load_worker_phase1_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase1_prompt(dir.path())?;
    assert!(prompt.contains("# PLANNING MODE"));
    Ok(())
}

#[test]
fn load_worker_phase2_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase2_prompt(dir.path())?;
    assert!(prompt.contains("# IMPLEMENTATION MODE"));
    Ok(())
}

#[test]
fn load_worker_phase3_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase3_prompt(dir.path())?;
    assert!(prompt.contains("# CODE REVIEW MODE"));
    Ok(())
}

#[test]
fn load_worker_single_phase_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_single_phase_prompt(dir.path())?;
    assert!(prompt.contains("single-pass execution mode"));
    Ok(())
}

#[test]
fn load_worker_prompt_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("worker.md"), "override")?;
    let prompt = load_worker_prompt(dir.path())?;
    assert_eq!(prompt, "override");
    Ok(())
}

#[test]
fn default_worker_prompt_excludes_completion_checklist() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_prompt(dir.path())?;
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    assert!(!prompt.contains("END-OF-TURN CHECKLIST"));
    Ok(())
}
