//! Code review, iteration, and completion checklist tests.
//!
//! Responsibilities: validate code review prompt rendering and checklist loading.
//! Not handled: worker prompts, phase rendering, or variable expansion.
//! Invariants/assumptions: embedded defaults contain expected markers; overrides take precedence.

use super::*;

#[test]
fn render_code_review_prompt_replaces_placeholders() -> Result<()> {
    let template = "ID={{TASK_ID}}\n";
    let config = default_config();
    let rendered = render_code_review_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PROJECT TYPE: CODE"));
    Ok(())
}

#[test]
fn render_code_review_prompt_allows_placeholder_like_text() -> Result<()> {
    let template = "ID={{TASK_ID}}\nSome text with {{TASK_ID}} in it\n";
    let config = default_config();
    let rendered = render_code_review_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("Some text with RQ-0001 in it"));
    Ok(())
}

#[test]
fn render_code_review_prompt_fails_missing_task_id() -> Result<()> {
    let template = "{{TASK_ID}}\n";
    let config = default_config();
    let result = render_code_review_prompt(template, "", ProjectType::Code, &config);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("task id"));
    Ok(())
}

#[test]
fn load_completion_checklist_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let checklist = load_completion_checklist(dir.path())?;
    assert!(checklist.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    Ok(())
}

#[test]
fn load_iteration_checklist_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let checklist = load_iteration_checklist(dir.path())?;
    assert!(checklist.contains("ITERATION CHECKLIST"));
    Ok(())
}

#[test]
fn load_phase2_handoff_checklist_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let checklist = load_phase2_handoff_checklist(dir.path())?;
    assert!(checklist.contains("PHASE 2 HANDOFF CHECKLIST"));
    assert!(!checklist.contains("follow-ups Phase 3 must close"));
    assert!(checklist.contains("BLOCKERS (should be empty)"));
    Ok(())
}

#[test]
fn load_completion_checklist_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("completion_checklist.md"), "override")?;
    let checklist = load_completion_checklist(dir.path())?;
    assert_eq!(checklist, "override");
    Ok(())
}

#[test]
fn load_iteration_checklist_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("iteration_checklist.md"), "override")?;
    let checklist = load_iteration_checklist(dir.path())?;
    assert_eq!(checklist, "override");
    Ok(())
}

#[test]
fn load_phase2_handoff_checklist_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("phase2_handoff_checklist.md"), "override")?;
    let checklist = load_phase2_handoff_checklist(dir.path())?;
    assert_eq!(checklist, "override");
    Ok(())
}
