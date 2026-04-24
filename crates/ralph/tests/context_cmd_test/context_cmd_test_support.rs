//! Shared helpers for `ralph context` integration tests.
//!
//! Purpose:
//! - Shared helpers for `ralph context` integration tests.
//!
//! Responsibilities:
//! - Create isolated Ralph repositories for context command coverage.
//! - Provide reusable AGENTS.md fixtures for validation and update tests.
//! - Keep suite-local filesystem helpers out of the individual behavior modules.
//!
//! Not handled here:
//! - Individual command assertions.
//! - Production context generation logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Repositories are initialized outside the main workspace.
//! - Fixture content matches the legacy suite's coverage expectations.

use anyhow::Result;
use std::fs;

#[path = "../test_support.rs"]
mod test_support;

pub(super) fn setup_repo() -> Result<tempfile::TempDir> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;
    Ok(dir)
}

pub(super) fn valid_agents_md() -> &'static str {
    r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#
}

pub(super) fn write_agents_md(dir: &std::path::Path, content: &str) -> Result<()> {
    fs::write(dir.join("AGENTS.md"), content)?;
    Ok(())
}

pub(super) fn run_in_dir(
    dir: &std::path::Path,
    args: &[&str],
) -> (std::process::ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}
