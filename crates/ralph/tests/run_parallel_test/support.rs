//! Purpose: suite-local fixtures and helpers for `run_parallel_test` integration coverage.
//!
//! Responsibilities:
//! - Create disposable git+`.ralph/` repos from the cached integration-test scaffold.
//! - Keep a bare `origin` remote alive for the full test so parallel direct-push runs are real.
//! - Centralize repeated queue setup, noop-runner configuration, and parallel-run invocation.
//!
//! Scope:
//! - Helpers used only by `crates/ralph/tests/run_parallel_test.rs`.
//!
//! Usage:
//! - Call `RunParallelRepo::new()` to create a seeded repo with a live remote.
//! - Use `write_queue()` plus `configure_default_runner()` for the common noop-runner fixture.
//! - Call `run_parallel()` to execute `ralph run loop --parallel ...` under the suite lock.
//!
//! Invariants/Assumptions:
//! - Helpers preserve end-to-end CLI coverage; they do not bypass the `ralph` binary.
//! - The cached git+`.ralph/` template replaces repeated `git init` + `seed_ralph_dir()` setup.
//! - The bare `origin` tempdir must stay owned by the fixture for the entire test lifetime.

use anyhow::{Context, Result, ensure};
use ralph::contracts::{Task, TaskStatus};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

const DEFAULT_RUNNER: &str = "opencode";
const DEFAULT_MODEL: &str = "test-model";
const PARALLEL_WORKERS: &str = "2";

pub(super) struct RunParallelRepo {
    dir: TempDir,
    _origin: TempDir,
}

impl RunParallelRepo {
    pub(super) fn new() -> Result<Self> {
        let dir = super::test_support::temp_dir_outside_repo();
        super::test_support::seed_git_repo_with_ralph(dir.path())?;

        let origin = super::test_support::temp_dir_outside_repo();
        init_bare_remote(origin.path())?;
        add_origin_remote(dir.path(), origin.path())?;
        push_origin_head(dir.path())?;

        Ok(Self {
            dir,
            _origin: origin,
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    pub(super) fn write_queue(&self, tasks: &[Task]) -> Result<()> {
        super::test_support::write_queue(self.path(), tasks)
    }

    pub(super) fn configure_default_runner(&self) -> Result<()> {
        let runner_path = super::test_support::create_noop_runner(self.path(), DEFAULT_RUNNER)?;
        super::test_support::configure_parallel_test_runner(
            self.path(),
            DEFAULT_RUNNER,
            DEFAULT_MODEL,
            &runner_path,
            1,
        )
    }

    pub(super) fn write_relative_file(&self, relative_path: &str, contents: &str) -> Result<()> {
        let path = self.path().join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    pub(super) fn run_parallel(&self, max_tasks: u8) -> (ExitStatus, String, String) {
        let max_tasks = max_tasks.to_string();
        let args = [
            "run",
            "loop",
            "--parallel",
            PARALLEL_WORKERS,
            "--max-tasks",
            max_tasks.as_str(),
            "--force",
        ];
        let run_lock = super::test_support::parallel_run_lock().lock();
        let result = super::test_support::run_in_dir(self.path(), &args);
        drop(run_lock);
        result
    }

    pub(super) fn read_parallel_state(&self) -> Result<Option<serde_json::Value>> {
        super::test_support::read_parallel_state(self.path())
    }

    pub(super) fn read_parallel_state_required(&self) -> Result<serde_json::Value> {
        self.read_parallel_state()?
            .ok_or_else(|| anyhow::anyhow!("parallel state file should exist after run"))
    }

    pub(super) fn state_path(&self) -> PathBuf {
        self.path().join(".ralph/cache/parallel/state.json")
    }

    pub(super) fn workspaces_dir(&self) -> PathBuf {
        self.path().join(".ralph/workspaces")
    }

    pub(super) fn workspace_dirs(&self) -> Result<Vec<PathBuf>> {
        let workspaces_dir = self.workspaces_dir();
        if !workspaces_dir.exists() {
            return Ok(Vec::new());
        }

        let mut workspaces = Vec::new();
        for entry in std::fs::read_dir(&workspaces_dir)
            .with_context(|| format!("read {}", workspaces_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                workspaces.push(path);
            }
        }
        Ok(workspaces)
    }

    pub(super) fn count_tasks_in_done_or_removed(&self, original_tasks: &[Task]) -> Result<usize> {
        let queue = super::test_support::read_queue(self.path())?;
        let done = super::test_support::read_done(self.path())?;

        let mut count = 0;
        for task in original_tasks {
            let in_done = done.tasks.iter().any(|candidate| candidate.id == task.id);
            let still_in_queue = queue.tasks.iter().any(|candidate| candidate.id == task.id);
            if in_done || !still_in_queue {
                count += 1;
            }
        }
        Ok(count)
    }
}

pub(super) fn todo_task(id: &str, title: &str) -> Task {
    super::test_support::make_test_task(id, title, TaskStatus::Todo)
}

pub(super) fn todo_tasks(entries: &[(&str, &str)]) -> Vec<Task> {
    entries
        .iter()
        .map(|(id, title)| todo_task(id, title))
        .collect()
}

fn init_bare_remote(remote_path: &Path) -> Result<()> {
    run_git(
        remote_path,
        &["init", "--bare", "--quiet"],
        "git init --bare --quiet",
    )
}

fn add_origin_remote(repo_path: &Path, remote_path: &Path) -> Result<()> {
    let remote = remote_path.to_string_lossy();
    run_git(
        repo_path,
        &["remote", "add", "origin", remote.as_ref()],
        "git remote add origin",
    )
}

fn push_origin_head(repo_path: &Path) -> Result<()> {
    run_git(
        repo_path,
        &["push", "-u", "origin", "HEAD"],
        "git push -u origin HEAD",
    )
}

fn run_git(dir: &Path, args: &[&str], context: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .with_context(|| context.to_string())?;
    ensure!(status.success(), "{context} failed");
    Ok(())
}
