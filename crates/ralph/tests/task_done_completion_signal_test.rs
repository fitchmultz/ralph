//! Integration tests for task completion signal behavior.
//!
//! Responsibilities:
//! - Verify `ralph task done` writes completion signals when forced by env.
//! - Verify default `task done` behavior still updates queue/done directly.
//!
//! Not handled here:
//! - Parallel worker orchestration or PR merge flows.
//! - Process-group supervision detection (covered by lock-specific tests).
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp git repos with `.ralph` state initialized.
//! - Queue/done files are valid JSON per schema version 1.

mod test_support;

use anyhow::Result;
use ralph::constants::paths::ENV_FORCE_COMPLETION_SIGNAL;
use ralph::contracts::{QueueFile, TaskStatus};
use ralph::queue;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use test_support::{git_init, make_test_task, ralph_bin, temp_dir_outside_repo};

#[derive(Debug, Deserialize)]
struct CompletionSignal {
    task_id: String,
    status: TaskStatus,
    notes: Vec<String>,
}

fn write_queue_files(repo_root: &Path) -> Result<(PathBuf, PathBuf)> {
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![make_test_task("RQ-0001", "Test Task", TaskStatus::Todo)],
    };
    let done_file = QueueFile {
        version: 1,
        tasks: Vec::new(),
    };

    queue::save_queue(&queue_path, &queue_file)?;
    queue::save_queue(&done_path, &done_file)?;

    Ok((queue_path, done_path))
}

fn run_task_done(repo_root: &Path, force_signal: bool) -> Result<(ExitStatus, String, String)> {
    let mut cmd = Command::new(ralph_bin());
    cmd.current_dir(repo_root)
        .env("RALPH_REPO_ROOT_OVERRIDE", repo_root)
        .env_remove("RUST_LOG")
        .args(["task", "done", "RQ-0001"]);

    if force_signal {
        cmd.env(ENV_FORCE_COMPLETION_SIGNAL, "1");
    } else {
        cmd.env_remove(ENV_FORCE_COMPLETION_SIGNAL);
    }

    let output = cmd.output()?;
    Ok((
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

#[test]
fn task_done_forced_writes_completion_signal_without_queue_mutation() -> Result<()> {
    let temp = temp_dir_outside_repo();
    let repo_root = temp.path();
    git_init(repo_root)?;
    let (queue_path, done_path) = write_queue_files(repo_root)?;

    let (status, _stdout, stderr) = run_task_done(repo_root, true)?;
    assert!(status.success(), "command failed: {stderr}");

    let queue_after = queue::load_queue(&queue_path)?;
    let done_after = queue::load_queue(&done_path)?;

    assert_eq!(queue_after.tasks.len(), 1);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");
    assert_eq!(queue_after.tasks[0].status, TaskStatus::Todo);
    assert!(done_after.tasks.is_empty());

    let signal_path = repo_root
        .join(".ralph")
        .join("cache")
        .join("completions")
        .join("RQ-0001.json");
    assert!(signal_path.exists());

    let raw = std::fs::read_to_string(&signal_path)?;
    let signal: CompletionSignal = serde_json::from_str(&raw)?;
    assert_eq!(signal.task_id, "RQ-0001");
    assert_eq!(signal.status, TaskStatus::Done);
    assert!(signal.notes.is_empty());

    Ok(())
}

#[test]
fn task_done_default_updates_queue_and_skips_signal() -> Result<()> {
    let temp = temp_dir_outside_repo();
    let repo_root = temp.path();
    git_init(repo_root)?;
    let (queue_path, done_path) = write_queue_files(repo_root)?;

    let (status, _stdout, stderr) = run_task_done(repo_root, false)?;
    assert!(status.success(), "command failed: {stderr}");

    let queue_after = queue::load_queue(&queue_path)?;
    let done_after = queue::load_queue(&done_path)?;

    assert!(queue_after.tasks.is_empty());
    assert_eq!(done_after.tasks.len(), 1);
    assert_eq!(done_after.tasks[0].id, "RQ-0001");
    assert_eq!(done_after.tasks[0].status, TaskStatus::Done);

    let signal_path = repo_root
        .join(".ralph")
        .join("cache")
        .join("completions")
        .join("RQ-0001.json");
    assert!(!signal_path.exists());

    Ok(())
}
