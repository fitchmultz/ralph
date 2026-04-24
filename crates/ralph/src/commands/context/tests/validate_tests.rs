//! Validation command tests.
//!
//! Purpose:
//! - Validation command tests.
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
fn validate_fails_when_file_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);

    let report = run_context_validate(
        &resolved,
        ContextValidateOptions {
            strict: false,
            path: resolved.repo_root.join("AGENTS.md"),
        },
    )?;

    assert!(!report.valid);
    assert!(!report.missing_sections.is_empty());

    Ok(())
}

#[test]
fn validate_passes_for_valid_file() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);

    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(resolved.repo_root.join("AGENTS.md"), content)?;

    let report = run_context_validate(
        &resolved,
        ContextValidateOptions {
            strict: false,
            path: resolved.repo_root.join("AGENTS.md"),
        },
    )?;

    assert!(report.valid);
    assert!(report.missing_sections.is_empty());

    Ok(())
}

#[test]
fn validate_strict_fails_for_missing_recommended() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = create_test_resolved(&dir);

    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(resolved.repo_root.join("AGENTS.md"), content)?;

    let report = run_context_validate(
        &resolved,
        ContextValidateOptions {
            strict: true,
            path: resolved.repo_root.join("AGENTS.md"),
        },
    )?;

    assert!(!report.valid);
    assert!(!report.missing_sections.is_empty());

    Ok(())
}
