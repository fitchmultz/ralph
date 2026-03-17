//! Purpose: suite-local fixtures and helpers for `parallel_direct_push_test` integration coverage.
//!
//! Responsibilities:
//! - Create disposable git+`.ralph/` repos from the cached integration-test scaffold.
//! - Keep a bare `origin` remote alive for tests that exercise real direct-push flows.
//! - Centralize parallel state writes, noop-runner setup, and common CLI invocation boilerplate.
//!
//! Scope:
//! - Helpers used only by `crates/ralph/tests/parallel_direct_push_test.rs`.
//!
//! Usage:
//! - Call `ParallelDirectPushRepo::new()` for status/retry/state-shape tests.
//! - Call `ParallelDirectPushRepo::with_origin()` for end-to-end parallel run tests.
//! - Use `configure_default_runner()` or `configure_runner_script()` before `run_parallel()`.
//!
//! Invariants/Assumptions:
//! - Helpers preserve end-to-end CLI coverage; they do not bypass the `ralph` binary.
//! - The cached git+`.ralph/` scaffold replaces repeated `git init` + `seed_ralph_dir()` setup.
//! - The bare `origin` tempdir must stay owned by the fixture for the entire test lifetime.

use anyhow::{Context, Result, ensure};
use ralph::contracts::{Task, TaskStatus};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

const DEFAULT_RUNNER: &str = "opencode";
const DEFAULT_MODEL: &str = "test-model";
const PARALLEL_WORKERS: &str = "2";

pub(super) struct ParallelDirectPushRepo {
    dir: TempDir,
    _origin: Option<TempDir>,
}

impl ParallelDirectPushRepo {
    pub(super) fn new() -> Result<Self> {
        let dir = super::test_support::temp_dir_outside_repo();
        super::test_support::seed_git_repo_with_ralph(dir.path())?;
        Ok(Self { dir, _origin: None })
    }

    pub(super) fn with_origin() -> Result<Self> {
        let mut repo = Self::new()?;
        let origin = super::test_support::temp_dir_outside_repo();
        init_bare_remote(origin.path())?;
        add_origin_remote(repo.path(), origin.path())?;
        push_origin_head(repo.path())?;
        repo._origin = Some(origin);
        Ok(repo)
    }

    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }

    pub(super) fn run(&self, args: &[&str]) -> (ExitStatus, String, String) {
        super::test_support::run_in_dir(self.path(), args)
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
        let result = self.run(&args);
        drop(run_lock);
        result
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

    pub(super) fn configure_runner_script(&self, script: &str) -> Result<PathBuf> {
        let runner_path = self.path().join("bin").join(DEFAULT_RUNNER);
        if let Some(parent) = runner_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        super::test_support::create_executable_script(
            runner_path
                .parent()
                .expect("runner path should have a parent directory"),
            DEFAULT_RUNNER,
            script,
        )?;
        super::test_support::configure_parallel_test_runner(
            self.path(),
            DEFAULT_RUNNER,
            DEFAULT_MODEL,
            &runner_path,
            1,
        )?;
        Ok(runner_path)
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

    pub(super) fn state_path(&self) -> PathBuf {
        self.path().join(".ralph/cache/parallel/state.json")
    }

    pub(super) fn write_parallel_state(&self, state: &serde_json::Value) -> Result<()> {
        if let Some(parent) = self.state_path().parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(self.state_path(), serde_json::to_string_pretty(state)?)
            .with_context(|| format!("write {}", self.state_path().display()))?;
        Ok(())
    }

    pub(super) fn read_parallel_state(&self) -> Result<Option<serde_json::Value>> {
        super::test_support::read_parallel_state(self.path())
    }

    pub(super) fn read_parallel_state_required(&self) -> Result<serde_json::Value> {
        self.read_parallel_state()?
            .ok_or_else(|| anyhow::anyhow!("parallel state file should exist after run"))
    }

    pub(super) fn push_origin_head(&self) -> Result<()> {
        push_origin_head(self.path())
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
