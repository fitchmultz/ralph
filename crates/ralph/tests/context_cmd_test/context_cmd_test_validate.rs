//! `ralph context validate` integration tests.
//!
//! Purpose:
//! - `ralph context validate` integration tests.
//!
//! Responsibilities:
//! - Cover success and failure flows for context validation.
//! - Verify custom path handling and strict-mode behavior.
//! - Assert that validation reports missing sections in stderr when appropriate.
//!
//! Not handled here:
//! - `context init` generation behavior.
//! - `context update` mutation workflows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Validation uses on-disk AGENTS.md content only.
//! - Missing required sections must fail loudly.

use anyhow::Result;
use std::fs;

use super::context_cmd_test_support::{run_in_dir, setup_repo, valid_agents_md, write_agents_md};

#[test]
fn context_validate_fails_when_file_missing() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(!status.success(), "should fail when AGENTS.md missing");
    anyhow::ensure!(
        stderr.contains("Validation failed")
            || stderr.contains("missing")
            || stderr.contains("not found"),
        "should report validation failure in stderr, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_passes_for_valid_file() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(dir.path(), valid_agents_md())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(
        status.success(),
        "validation should pass\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("valid") || stderr.contains("valid"),
        "should report validity"
    );

    Ok(())
}

#[test]
fn context_validate_checks_context() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(
        dir.path(),
        r#"# Repository Guidelines

Test project context.

## Non-Negotiables

- Rule 1: Do not commit secrets
- Rule 2: Run tests before pushing

## Repository Map

- `src/`: Source code
- `tests/`: Test files

## Build, Test, and CI

Run `make ci` to verify everything.
"#,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(
        status.success(),
        "context validate should pass for valid context file\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("valid") || stderr.contains("valid"),
        "should report context is valid"
    );

    Ok(())
}

#[test]
fn context_validate_fails_for_missing_required_sections() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(
        dir.path(),
        r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.
"#,
    )?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(
        !status.success(),
        "should fail with missing required sections"
    );
    anyhow::ensure!(
        stderr.contains("Validation failed") || stderr.contains("Missing sections"),
        "should report missing sections, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_strict_fails_for_missing_recommended() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(dir.path(), valid_agents_md())?;

    let (status, _stdout, _stderr) = run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(status.success(), "non-strict validation should pass");

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "validate", "--strict"]);
    anyhow::ensure!(
        !status.success(),
        "strict validation should fail with missing recommended sections"
    );
    anyhow::ensure!(
        stderr.contains("Validation failed") || stderr.contains("Missing sections"),
        "should report missing sections in strict mode, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_respects_custom_path() -> Result<()> {
    let dir = setup_repo()?;
    fs::create_dir_all(dir.path().join("docs"))?;
    fs::write(dir.path().join("docs/AGENTS.md"), valid_agents_md())?;

    let (status, _stdout, stderr) = run_in_dir(
        dir.path(),
        &["context", "validate", "--path", "docs/AGENTS.md"],
    );
    anyhow::ensure!(
        status.success(),
        "validation should pass for custom path\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_reports_missing_sections_in_stderr() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(
        dir.path(),
        r#"# Repository Guidelines

Test project with no sections.
"#,
    )?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(!status.success(), "should fail with missing sections");
    anyhow::ensure!(
        stderr.contains("Non-Negotiables")
            || stderr.contains("Repository Map")
            || stderr.contains("Build, Test, and CI")
            || stderr.contains("Missing sections"),
        "should report which sections are missing, got: {stderr}"
    );

    Ok(())
}
