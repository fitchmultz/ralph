//! Sequential run-loop fail-fast regression tests.
//!
//! Purpose:
//! - Sequential run-loop fail-fast regression tests.
//!
//! Responsibilities:
//! - Prove the sequential run loop aborts immediately after a task execution failure.
//! - Prevent hot-loop retries that continuously reselect the same broken task.
//!
//! Not handled here:
//! - Parallel run-loop behavior.
//! - Root-cause validation for why a specific task failed.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `max_tasks = 0` still means unbounded execution for successful runs.
//! - Deterministic task execution failures must terminate the sequential loop after one attempt.

use super::{LoggerState, take_logs};
use crate::contracts::{AgentConfig, Config, QueueConfig, QueueFile, Runner, TaskStatus};
use crate::queue;
use crate::testsupport::{INTERRUPT_TEST_MUTEX, reset_ctrlc_interrupt_flag};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

fn git_status_ok(dir: &Path, args: &[&str], description: &str) -> anyhow::Result<()> {
    let _path_guard = crate::testsupport::path::path_lock()
        .lock()
        .expect("path lock");
    let status = Command::new("git").current_dir(dir).args(args).status()?;
    anyhow::ensure!(status.success(), "{description}");
    Ok(())
}

fn git_init(dir: &Path) -> anyhow::Result<()> {
    git_status_ok(dir, &["init", "--quiet"], "git init failed")?;

    let gitignore_path = dir.join(".gitignore");
    std::fs::write(&gitignore_path, ".ralph/lock\n.ralph/cache/\nbin/\n")?;
    git_status_ok(dir, &["add", ".gitignore"], "git add .gitignore failed")?;
    git_status_ok(
        dir,
        &["commit", "--quiet", "-m", "add gitignore"],
        "git commit .gitignore failed",
    )?;

    Ok(())
}

fn resolved_with_missing_runner(repo_root: std::path::PathBuf) -> crate::config::Resolved {
    crate::config::Resolved {
        config: Config {
            agent: AgentConfig {
                runner: Some(Runner::Opencode),
                model: Some(crate::contracts::Model::Gpt53),
                notification: crate::contracts::NotificationConfig {
                    enabled: Some(false),
                    ..crate::contracts::NotificationConfig::default()
                },
                ..AgentConfig::default()
            },
            queue: QueueConfig {
                file: Some(std::path::PathBuf::from(".ralph/queue.json")),
                done_file: Some(std::path::PathBuf::from(".ralph/done.json")),
                ..QueueConfig::default()
            },
            ..Config::default()
        },
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

#[test]
fn sequential_run_loop_aborts_after_single_task_failure() -> anyhow::Result<()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().expect("interrupt mutex");
    reset_ctrlc_interrupt_flag();

    let _ = take_logs();

    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    git_init(&repo_root)?;

    let resolved = resolved_with_missing_runner(repo_root.clone());

    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![crate::commands::run::tests::task_with_status(
                TaskStatus::Todo,
            )],
        },
    )?;
    queue::save_queue(&resolved.done_path, &QueueFile::default())?;

    let result = crate::commands::run::run_loop(
        &resolved,
        crate::commands::run::RunLoopOptions {
            max_tasks: 0,
            agent_overrides: crate::commands::run::AgentOverrides::default(),
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
            run_event_handler: None,
        },
    );

    let err = result.expect_err("expected run loop to fail fast on task execution error");
    let err_text = format!("{err:#}");
    assert!(
        err_text.contains("Plan cache not found")
            || err_text.contains("runner executable not found")
            || err_text.contains("No such file or directory"),
        "expected deterministic task failure, got: {err_text}"
    );

    let queue_after = queue::load_queue(&resolved.queue_path)?;
    assert_eq!(
        queue_after.tasks.len(),
        1,
        "expected task to remain in queue after fail-fast abort"
    );
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");

    let (state, logs) = take_logs();
    if state == LoggerState::TestLogger {
        let task_failed_logs = logs
            .iter()
            .filter(|line| line.contains("RunLoop: task failed:"))
            .count();
        assert_eq!(
            task_failed_logs, 1,
            "expected exactly one task failure log, got logs: {logs:?}"
        );
    }

    Ok(())
}
