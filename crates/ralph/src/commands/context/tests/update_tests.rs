//! Update command tests.
//!
//! Purpose:
//! - Update command tests.
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
fn update_fails_when_file_missing() {
    let dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&dir);

    let result = run_context_update(
        &resolved,
        ContextUpdateOptions {
            sections: vec!["troubleshooting".to_string()],
            file: None,
            interactive: false,
            dry_run: false,
            output_path: resolved.repo_root.join("AGENTS.md"),
        },
    );

    assert!(result.is_err());
}

#[test]
fn update_returns_sections_updated() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);

    fs::write(
        resolved.repo_root.join("AGENTS.md"),
        "# Repository Guidelines\n\n## Non-Negotiables\n\nRules.\n",
    )?;

    fs::write(
        resolved.repo_root.join("update.md"),
        "## Non-Negotiables\n\nAdditional rules.\n",
    )?;

    let report = run_context_update(
        &resolved,
        ContextUpdateOptions {
            sections: vec!["Non-Negotiables".to_string()],
            file: Some(resolved.repo_root.join("update.md")),
            interactive: false,
            dry_run: true,
            output_path: resolved.repo_root.join("AGENTS.md"),
        },
    )?;

    assert!(report.dry_run);
    assert!(
        report
            .sections_updated
            .contains(&"Non-Negotiables".to_string())
    );

    Ok(())
}
