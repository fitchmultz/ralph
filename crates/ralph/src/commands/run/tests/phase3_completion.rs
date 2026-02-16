//! Phase 3 completion signal tests for run command.

use super::{resolved_with_repo_root, task_with_status};
use crate::commands::run::phases::phase3::finalize_phase3_if_done;
use crate::commands::run::supervision::PushPolicy;
use crate::completions;
use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use crate::testsupport::git as git_test;

#[test]
fn apply_phase3_completion_signal_moves_task_and_clears_signal() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_status(TaskStatus::Doing)],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["Reviewed".to_string()],
        runner_used: None,
        model_used: None,
    };
    completions::write_completion_signal(&resolved.repo_root, &signal)?;

    let status = crate::commands::run::apply_phase3_completion_signal(&resolved, "RQ-0001")?;
    assert_eq!(status, Some(TaskStatus::Done));

    let done = queue::load_queue(&resolved.done_path)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");
    assert_eq!(done.tasks[0].status, TaskStatus::Done);
    assert_eq!(done.tasks[0].notes, vec!["Reviewed".to_string()]);

    let remaining = queue::load_queue(&resolved.queue_path)?;
    assert!(remaining.tasks.is_empty());

    let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
    assert!(signal_after.is_none());

    Ok(())
}

#[test]
fn apply_phase3_completion_signal_already_archived_clears_signal() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let mut done_task = task_with_status(TaskStatus::Done);
    done_task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    let done_file = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    queue::save_queue(&resolved.done_path, &done_file)?;

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["Reviewed".to_string()],
        runner_used: None,
        model_used: None,
    };
    completions::write_completion_signal(&resolved.repo_root, &signal)?;

    let status = crate::commands::run::apply_phase3_completion_signal(&resolved, "RQ-0001")?;
    assert_eq!(status, Some(TaskStatus::Done));

    let done = queue::load_queue(&resolved.done_path)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");

    let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
    assert!(signal_after.is_none());
    Ok(())
}

#[test]
fn apply_phase3_completion_signal_missing_returns_none() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_status(TaskStatus::Doing)],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let status = crate::commands::run::apply_phase3_completion_signal(&resolved, "RQ-0001")?;
    assert!(status.is_none());
    Ok(())
}

#[test]
fn apply_phase3_completion_signal_keeps_signal_on_failure() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["Reviewed".to_string()],
        runner_used: None,
        model_used: None,
    };
    completions::write_completion_signal(&resolved.repo_root, &signal)?;

    let err =
        crate::commands::run::apply_phase3_completion_signal(&resolved, "RQ-0001").unwrap_err();
    assert!(
        err.to_string().contains("task not found"),
        "expected missing task error, got: {err}"
    );

    let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
    assert!(signal_after.is_some());
    Ok(())
}

#[test]
fn finalize_phase3_if_done_runs_post_run_supervise_without_signal() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    git_test::init_repo(temp.path())?;
    let mut resolved = resolved_with_repo_root(temp.path().to_path_buf());
    resolved.config.agent.ci_gate_enabled = Some(false);

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;
    let mut done_task = task_with_status(TaskStatus::Done);
    done_task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    let done_file = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    queue::save_queue(&resolved.done_path, &done_file)?;
    git_test::commit_all(temp.path(), "init")?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let finalized = finalize_phase3_if_done(
        &resolved,
        "RQ-0001",
        None,
        crate::contracts::GitRevertMode::Disabled,
        true,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;
    assert!(finalized, "expected phase 3 finalization to run");

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(status.trim().is_empty(), "expected clean repo");
    Ok(())
}
