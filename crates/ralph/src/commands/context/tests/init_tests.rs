//! Init command tests.
//!
//! Purpose:
//! - Init command tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
fn init_creates_agents_md() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);
    fs::create_dir_all(resolved.repo_root.join("src"))?;

    let output_path = resolved.repo_root.join("AGENTS.md");
    let report = run_context_init(
        &resolved,
        ContextInitOptions {
            force: false,
            project_type_hint: None,
            output_path: output_path.clone(),
            interactive: false,
        },
    )?;

    assert_eq!(report.status, FileInitStatus::Created);
    assert!(output_path.exists());

    let content = fs::read_to_string(&output_path)?;
    assert!(content.contains("# Repository Guidelines"));
    assert!(content.contains("Non-Negotiables"));
    assert!(content.contains("Repository Map"));

    Ok(())
}

#[test]
fn init_skips_existing_without_force() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);

    let output_path = resolved.repo_root.join("AGENTS.md");
    fs::write(&output_path, "existing content")?;

    let report = run_context_init(
        &resolved,
        ContextInitOptions {
            force: false,
            project_type_hint: None,
            output_path: output_path.clone(),
            interactive: false,
        },
    )?;

    assert_eq!(report.status, FileInitStatus::Valid);
    let content = fs::read_to_string(&output_path)?;
    assert_eq!(content, "existing content");

    Ok(())
}

#[test]
fn init_overwrites_with_force() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);

    let output_path = resolved.repo_root.join("AGENTS.md");
    fs::write(&output_path, "existing content")?;

    let report = run_context_init(
        &resolved,
        ContextInitOptions {
            force: true,
            project_type_hint: None,
            output_path: output_path.clone(),
            interactive: false,
        },
    )?;

    assert_eq!(report.status, FileInitStatus::Created);
    let content = fs::read_to_string(&output_path)?;
    assert!(content.contains("# Repository Guidelines"));

    Ok(())
}
