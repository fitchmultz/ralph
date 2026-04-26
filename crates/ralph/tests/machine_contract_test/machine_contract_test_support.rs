//! Shared helpers for `ralph machine` contract integration tests.
//!
//! Purpose:
//! - Provide suite-local repo bootstrap and JSON request helpers for machine contract tests.
//!
//! Responsibilities:
//! - Initialize disposable git and Ralph repositories through the public test harness.
//! - Wrap the shared integration helpers in a machine-suite-local surface.
//! - Keep repeated request-file creation and trust setup out of scenario modules.
//!
//! Non-scope:
//! - Owning scenario assertions for queue, task, recovery, or parallel behavior.
//! - Replacing the global integration `test_support` module.
//!
//! Usage:
//! - Use `setup_git_repo()` when a test only needs git state.
//! - Use `setup_ralph_repo()` when a test needs a fully initialized Ralph repo.
//! - Use `write_json_file()` for machine input payloads that are passed via `--input`.
//!
//! Invariants/assumptions callers must respect:
//! - Repositories are disposable temp directories created outside the workspace tree.
//! - JSON helper output must remain UTF-8 and stable enough for existing CLI contract assertions.

use anyhow::Result;
use ralph::contracts::{Task, TaskStatus};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use tempfile::TempDir;

#[path = "../test_support.rs"]
mod test_support;

pub(super) fn setup_git_repo() -> Result<TempDir> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    Ok(dir)
}

pub(super) fn setup_ralph_repo() -> Result<TempDir> {
    let dir = setup_git_repo()?;
    test_support::ralph_init(dir.path())?;
    Ok(dir)
}

pub(super) fn trust_project_commands(dir: &Path) -> Result<()> {
    test_support::trust_project_commands(dir)
}

pub(super) fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}

pub(super) fn create_fake_runner(dir: &Path, runner: &str, script: &str) -> Result<PathBuf> {
    test_support::create_fake_runner(dir, runner, script)
}

pub(super) fn configure_runner(
    dir: &Path,
    runner: &str,
    model: &str,
    runner_path: Option<&Path>,
) -> Result<()> {
    test_support::configure_runner(dir, runner, model, runner_path)
}

pub(super) fn configure_ci_gate(
    dir: &Path,
    command: Option<&str>,
    enabled: Option<bool>,
) -> Result<()> {
    test_support::configure_ci_gate(dir, command, enabled)
}

pub(super) fn git_add_all_commit(dir: &Path, message: &str) -> Result<()> {
    test_support::git_add_all_commit(dir, message)
}

pub(super) fn write_json_file<T: Serialize>(
    dir: &Path,
    name: &str,
    document: &T,
) -> Result<PathBuf> {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(document)?)?;
    Ok(path)
}

pub(super) fn write_queue(dir: &Path, tasks: &[Task]) -> Result<()> {
    test_support::write_queue(dir, tasks)
}

pub(super) fn write_done(dir: &Path, tasks: &[Task]) -> Result<()> {
    test_support::write_done(dir, tasks)
}

pub(super) fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    test_support::make_test_task(id, title, status)
}
