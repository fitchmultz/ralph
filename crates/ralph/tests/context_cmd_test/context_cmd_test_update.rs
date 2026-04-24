//! `ralph context update` integration tests.
//!
//! Purpose:
//! - `ralph context update` integration tests.
//!
//! Responsibilities:
//! - Cover file-based update flows, section targeting, and dry-run behavior.
//! - Verify custom output-path handling for updates.
//! - Verify missing-source and missing-file failures.
//!
//! Not handled here:
//! - `context init` generation paths.
//! - Validation-only reporting behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Update tests mutate only temp-repo fixtures.
//! - Section replacement assertions compare final file content on disk.

use anyhow::Result;
use std::fs;

use super::context_cmd_test_support::{run_in_dir, setup_repo, valid_agents_md, write_agents_md};

#[test]
fn context_update_fails_when_file_missing() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) =
        run_in_dir(dir.path(), &["context", "update", "--section", "test"]);
    anyhow::ensure!(!status.success(), "should fail when AGENTS.md missing");
    anyhow::ensure!(
        stderr.contains("does not exist") || stderr.contains("not found"),
        "should report missing file, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_update_with_file_succeeds() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(dir.path(), valid_agents_md())?;
    fs::write(
        dir.path().join("update.md"),
        "## Non-Negotiables\n\nUpdated rules with new information.\n",
    )?;

    let (status, _stdout, stderr) =
        run_in_dir(dir.path(), &["context", "update", "--file", "update.md"]);
    anyhow::ensure!(status.success(), "update should succeed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Updated rules"),
        "section should be updated, got:\n{content}"
    );

    Ok(())
}

#[test]
fn context_update_dry_run_does_not_modify() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(
        dir.path(),
        r#"# Repository Guidelines

Test project.

## Non-Negotiables

Original rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#,
    )?;
    fs::write(
        dir.path().join("update.md"),
        "## Non-Negotiables\n\nUpdated rules with new information.\n",
    )?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["context", "update", "--file", "update.md", "--dry-run"],
    );
    anyhow::ensure!(
        status.success(),
        "dry-run should succeed\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("Dry run") || stderr.contains("Dry run"),
        "should indicate dry run mode"
    );

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Original rules"),
        "original content should be preserved in dry run, got:\n{content}"
    );
    anyhow::ensure!(
        !content.contains("Updated rules"),
        "content should not be updated in dry run"
    );

    Ok(())
}

#[test]
fn context_update_with_section_filter() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(
        dir.path(),
        r#"# Repository Guidelines

Test project.

## Non-Negotiables

Original non-negotiables.

## Repository Map

Original repository map.

## Build, Test, and CI

Original build info.
"#,
    )?;
    fs::write(
        dir.path().join("update.md"),
        "## Non-Negotiables\n\nUpdated non-negotiables.\n\n## Repository Map\n\nUpdated repository map.\n",
    )?;

    let (status, _stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "context",
            "update",
            "--file",
            "update.md",
            "--section",
            "Non-Negotiables",
        ],
    );
    anyhow::ensure!(status.success(), "update should succeed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Updated non-negotiables"),
        "Non-Negotiables should be updated"
    );
    anyhow::ensure!(
        content.contains("Original repository map"),
        "Repository Map should not be updated"
    );

    Ok(())
}

#[test]
fn context_update_respects_output_path() -> Result<()> {
    let dir = setup_repo()?;
    fs::create_dir_all(dir.path().join("docs"))?;
    fs::write(dir.path().join("docs/AGENTS.md"), valid_agents_md())?;
    fs::write(
        dir.path().join("update.md"),
        "## Non-Negotiables\n\nUpdated rules.\n",
    )?;

    let (status, _stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "context",
            "update",
            "--file",
            "update.md",
            "--output",
            "docs/AGENTS.md",
        ],
    );
    anyhow::ensure!(status.success(), "update should succeed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("docs/AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Updated rules"),
        "section should be updated at custom path"
    );

    Ok(())
}

#[test]
fn context_update_fails_without_source() -> Result<()> {
    let dir = setup_repo()?;
    write_agents_md(dir.path(), valid_agents_md())?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "update"]);
    anyhow::ensure!(!status.success(), "should fail without update source");
    anyhow::ensure!(
        stderr.contains("No update source")
            || stderr.contains("--file")
            || stderr.contains("--interactive"),
        "should report missing update source, got: {stderr}"
    );

    Ok(())
}
