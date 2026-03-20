//! Purpose: suite-local helpers for lock contention integration tests.
//!
//! Responsibilities:
//! - Spawn and synchronize the self-exec lock-holder subprocess used by contention scenarios.
//! - Build minimal queue fixtures and resolved config for run-loop abort regression tests.
//! - Centralize shared `run_loop()` invocation options so abort-path timing remains consistent.
//!
//! Scope:
//! - Shared test-only setup and teardown helpers for this suite.
//!
//! Usage:
//! - Contention tests call `spawn_lock_holder()` and keep the returned handle alive until assertions finish.
//! - Run-loop abort tests call `setup_run_loop_fixture()` and `run_loop_once()`.
//!
//! Invariants/Assumptions:
//! - Subprocess spawning must keep `--exact lock_holder_process --nocapture` and the existing env vars unchanged.
//! - Readiness detection is driven only by the `LOCK_HELD` stdout line.
//! - Queue fixtures must remain semantically identical to the pre-split monolith.

use super::*;
use ralph::commands::run;
use ralph::config::Resolved;
use ralph::contracts::{Config, QueueFile, Task, TaskPriority, TaskStatus};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

pub(super) struct LockHolderHandle {
    child: Option<Child>,
    child_stdin: Option<ChildStdin>,
    lock_dir: PathBuf,
}

impl LockHolderHandle {
    fn cleanup(&mut self) {
        if let Some(child_stdin) = self.child_stdin.take() {
            drop(child_stdin);
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.lock_dir);
    }
}

impl Drop for LockHolderHandle {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub(super) fn spawn_lock_holder(repo_root: &Path, label: Option<&str>) -> Result<LockHolderHandle> {
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let mut command = Command::new(super::lock_holder::current_exe());
    command
        .arg("--exact")
        .arg("lock_holder_process")
        .arg("--nocapture")
        .env("RALPH_TEST_LOCK_HOLD", "1")
        .env("RALPH_TEST_REPO_ROOT", repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    if let Some(label) = label {
        command.env("RALPH_TEST_LOCK_LABEL", label);
    }

    let mut child = command.spawn().context("spawn lock holder process")?;
    let child_stdin = child.stdin.take().context("capture lock holder stdin")?;
    let stdout = child.stdout.take().context("capture lock holder stdout")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line.clone()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let handle = LockHolderHandle {
        child: Some(child),
        child_stdin: Some(child_stdin),
        lock_dir: lock::queue_lock_dir(repo_root),
    };

    let got_signal =
        super::test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            while let Ok(line) = rx.try_recv() {
                if line.contains("LOCK_HELD") {
                    return true;
                }
            }
            false
        });
    anyhow::ensure!(got_signal, "lock holder did not signal readiness");

    Ok(handle)
}

pub(super) struct RunLoopFixture {
    _dir: TempDir,
    pub(super) repo_root: PathBuf,
    pub(super) resolved: Resolved,
}

pub(super) fn setup_run_loop_fixture(relates_to: Vec<String>) -> Result<RunLoopFixture> {
    let dir = TempDir::new().context("create temp dir")?;
    let repo_root = dir.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let queue = QueueFile {
        version: 1,
        tasks: vec![test_task(relates_to)],
    };
    let queue_path = repo_root.join(".ralph/queue.jsonc");
    let done_path = repo_root.join(".ralph/done.jsonc");
    ralph::queue::save_queue(&queue_path, &queue)?;
    ralph::queue::save_queue(&done_path, &QueueFile::default())?;

    let resolved = Resolved {
        config: Config::default(),
        repo_root: repo_root.clone(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
    };

    Ok(RunLoopFixture {
        _dir: dir,
        repo_root,
        resolved,
    })
}

fn test_task(relates_to: Vec<String>) -> Task {
    Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        description: None,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec!["src/main.rs".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-02-06T00:00:00Z".to_string()),
        updated_at: Some("2026-02-06T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        estimated_minutes: None,
        actual_minutes: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to,
        duplicates: None,
        custom_fields: Default::default(),
        parent_id: None,
    }
}

pub(super) fn run_loop_once(resolved: &Resolved) -> (Result<()>, Duration) {
    let start = Instant::now();
    let result = run::run_loop(resolved, run_loop_options());
    let elapsed = start.elapsed();
    (result, elapsed)
}

fn run_loop_options() -> run::RunLoopOptions {
    run::RunLoopOptions {
        max_tasks: 0,
        agent_overrides: ralph::agent::AgentOverrides::default(),
        force: false,
        auto_resume: false,
        starting_completed: 0,
        non_interactive: true,
        parallel_workers: None,
        wait_when_blocked: false,
        wait_poll_ms: 1000,
        wait_timeout_seconds: 0,
        notify_when_unblocked: false,
        wait_when_empty: false,
        empty_poll_ms: 30_000,
    }
}
