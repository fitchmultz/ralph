//! Shared setup helpers for `doctor_contract_test`.
//!
//! Purpose:
//! - Shared setup helpers for `doctor_contract_test`.
//!
//! Responsibilities:
//! - Create isolated git repositories for doctor contract coverage.
//! - Seed cached `.ralph/` fixtures instead of running real `ralph init`.
//! - Keep doctor-suite bootstrap files centralized in one place.
//!
//! Not handled here:
//! - Individual doctor assertions or output parsing.
//! - Generic test support used by other suites.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Seeded repos rely on cached `seed_ralph_dir()` scaffolding rather than real init.
//! - Git-only repos intentionally omit `.ralph/` files for missing-queue coverage.
//! - Doctor suite fixtures keep a minimal `Makefile` so project checks stay deterministic.

use anyhow::Result;
use std::path::Path;
use tempfile::TempDir;

pub(super) fn setup_git_repo() -> Result<TempDir> {
    let dir = super::test_support::temp_dir_outside_repo();
    super::test_support::git_init(dir.path())?;
    Ok(dir)
}

pub(super) fn setup_doctor_repo() -> Result<TempDir> {
    let dir = setup_git_repo()?;
    super::test_support::seed_ralph_dir(dir.path())?;
    write_makefile(dir.path())?;
    Ok(dir)
}

pub(super) fn setup_trusted_doctor_repo() -> Result<TempDir> {
    let dir = setup_doctor_repo()?;
    super::test_support::trust_project_commands(dir.path())?;
    Ok(dir)
}

pub(super) fn write_makefile(dir: &Path) -> Result<()> {
    std::fs::write(dir.join("Makefile"), "ci:\n\tcargo test\n")?;
    Ok(())
}

pub(super) fn write_repo_config(dir: &Path, contents: &str) -> Result<()> {
    std::fs::write(dir.join(".ralph/config.jsonc"), contents)?;
    Ok(())
}

pub(super) fn write_global_config(home_dir: &Path, contents: &str) -> Result<()> {
    let global_config_dir = home_dir.join(".config/ralph");
    std::fs::create_dir_all(&global_config_dir)?;
    std::fs::write(global_config_dir.join("config.jsonc"), contents)?;
    Ok(())
}
