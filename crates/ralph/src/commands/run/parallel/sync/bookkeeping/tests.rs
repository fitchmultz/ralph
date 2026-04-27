//! Tests for coordinator-owned parallel bookkeeping reconciliation.
//!
//! Purpose:
//! - Verify ignored/untracked queue/done files are reconciled by task result,
//!   not copied wholesale from worker snapshots.
//!
//! Responsibilities:
//! - Cover multi-worker stale-snapshot merge behavior.
//! - Cover malformed successful-worker terminal results and split tracking state.
//!
//! Non-scope:
//! - Worker process lifecycle and branch refresh behavior.
//!
//! Usage:
//! - Run through `cargo test -p ralph commands::run::parallel::sync::bookkeeping`.
//!
//! Invariants/Assumptions:
//! - Successful worker snapshots may contain stale sibling queue state.
//! - Coordinator queue/done remains the merge source of truth for ignored files.

use super::*;
use crate::contracts::{Config, QueueFile, TaskPriority};
use crate::testsupport::git as git_test;
use tempfile::TempDir;

fn test_resolved(repo_root: &Path) -> config::Resolved {
    config::Resolved {
        config: Config::default(),
        repo_root: repo_root.to_path_buf(),
        queue_path: repo_root.join(".ralph/queue.jsonc"),
        done_path: repo_root.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

fn task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: format!("Task {id}"),
        priority: TaskPriority::Medium,
        created_at: Some("2026-04-27T00:00:00Z".to_string()),
        updated_at: Some("2026-04-27T00:00:00Z".to_string()),
        completed_at: (status == TaskStatus::Done).then(|| "2026-04-27T00:01:00Z".to_string()),
        ..Task::default()
    }
}

fn ids(queue: &QueueFile) -> Vec<String> {
    let mut ids = queue
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn init_ignored_bookkeeping_repo(temp: &TempDir) -> Result<std::path::PathBuf> {
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    git_test::init_repo(&repo_root)?;
    std::fs::write(
        repo_root.join(".gitignore"),
        ".ralph/queue.jsonc\n.ralph/done.jsonc\n.ralph/cache/\n",
    )?;
    git_test::git_run(&repo_root, &["add", ".gitignore"])?;
    git_test::git_run(&repo_root, &["commit", "-m", "ignore bookkeeping"])?;
    Ok(repo_root)
}

fn worker_with_done_snapshot(
    temp: &TempDir,
    name: &str,
    done_tasks: Vec<Task>,
) -> Result<std::path::PathBuf> {
    let workspace = temp.path().join("workspaces").join(name);
    std::fs::create_dir_all(workspace.join(".ralph"))?;
    queue::save_queue(
        &workspace.join(".ralph/done.jsonc"),
        &QueueFile {
            version: 1,
            tasks: done_tasks,
        },
    )?;
    Ok(workspace)
}

#[test]
fn reconciles_two_ignored_jsonc_workers_without_last_writer_wins() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = init_ignored_bookkeeping_repo(&temp)?;
    let resolved = test_resolved(&repo_root);

    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![
                task("RQ-0001", TaskStatus::Todo),
                task("RQ-0002", TaskStatus::Todo),
            ],
        },
    )?;
    queue::save_queue(&resolved.done_path, &QueueFile::default())?;

    let worker_one =
        worker_with_done_snapshot(&temp, "RQ-0001", vec![task("RQ-0001", TaskStatus::Done)])?;
    let worker_two =
        worker_with_done_snapshot(&temp, "RQ-0002", vec![task("RQ-0002", TaskStatus::Done)])?;
    let queue_lock = crate::queue::acquire_queue_lock(&repo_root, "test", false)?;

    let authority = reconcile_successful_workers(
        &resolved,
        &queue_lock,
        &[
            SuccessfulWorkerBookkeeping {
                task_id: "RQ-0001".to_string(),
                workspace_path: worker_one,
            },
            SuccessfulWorkerBookkeeping {
                task_id: "RQ-0002".to_string(),
                workspace_path: worker_two,
            },
        ],
        "test parallel reconciliation",
    )?;

    assert_eq!(authority, BookkeepingAuthority::CoordinatorReconcile);
    assert_eq!(
        ids(&queue::load_queue(&resolved.queue_path)?),
        Vec::<String>::new()
    );
    assert_eq!(
        ids(&queue::load_queue(&resolved.done_path)?),
        vec!["RQ-0001".to_string(), "RQ-0002".to_string()]
    );
    assert!(crate::undo::undo_cache_dir(&repo_root).exists());
    Ok(())
}

#[test]
fn successful_worker_missing_done_entry_fails_without_mutating_coordinator() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = init_ignored_bookkeeping_repo(&temp)?;
    let resolved = test_resolved(&repo_root);
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001", TaskStatus::Todo)],
        },
    )?;
    queue::save_queue(&resolved.done_path, &QueueFile::default())?;
    let worker = worker_with_done_snapshot(&temp, "RQ-0001", Vec::new())?;
    let queue_lock = crate::queue::acquire_queue_lock(&repo_root, "test", false)?;

    let err = reconcile_successful_workers(
        &resolved,
        &queue_lock,
        &[SuccessfulWorkerBookkeeping {
            task_id: "RQ-0001".to_string(),
            workspace_path: worker,
        }],
        "test parallel reconciliation",
    )
    .expect_err("missing done entry should fail");

    assert!(err.to_string().contains("did not archive its task"));
    assert_eq!(
        ids(&queue::load_queue(&resolved.queue_path)?),
        vec!["RQ-0001".to_string()]
    );
    assert_eq!(
        ids(&queue::load_queue(&resolved.done_path)?),
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn successful_worker_duplicate_done_entry_fails() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = init_ignored_bookkeeping_repo(&temp)?;
    let resolved = test_resolved(&repo_root);
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001", TaskStatus::Todo)],
        },
    )?;
    queue::save_queue(&resolved.done_path, &QueueFile::default())?;
    let worker = worker_with_done_snapshot(
        &temp,
        "RQ-0001",
        vec![
            task("RQ-0001", TaskStatus::Done),
            task("RQ-0001", TaskStatus::Done),
        ],
    )?;
    let queue_lock = crate::queue::acquire_queue_lock(&repo_root, "test", false)?;

    let err = reconcile_successful_workers(
        &resolved,
        &queue_lock,
        &[SuccessfulWorkerBookkeeping {
            task_id: "RQ-0001".to_string(),
            workspace_path: worker,
        }],
        "test parallel reconciliation",
    )
    .expect_err("duplicate done entry should fail");

    assert!(err.to_string().contains("produced 2 done entries"));
    Ok(())
}

#[test]
fn successful_worker_non_done_terminal_status_fails() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = init_ignored_bookkeeping_repo(&temp)?;
    let resolved = test_resolved(&repo_root);
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001", TaskStatus::Todo)],
        },
    )?;
    queue::save_queue(&resolved.done_path, &QueueFile::default())?;
    let mut rejected = task("RQ-0001", TaskStatus::Rejected);
    rejected.completed_at = Some("2026-04-27T00:01:00Z".to_string());
    let worker = worker_with_done_snapshot(&temp, "RQ-0001", vec![rejected])?;
    let queue_lock = crate::queue::acquire_queue_lock(&repo_root, "test", false)?;

    let err = reconcile_successful_workers(
        &resolved,
        &queue_lock,
        &[SuccessfulWorkerBookkeeping {
            task_id: "RQ-0001".to_string(),
            workspace_path: worker,
        }],
        "test parallel reconciliation",
    )
    .expect_err("rejected status should fail for successful worker");

    assert!(err.to_string().contains("expected done"));
    Ok(())
}

#[test]
fn split_tracked_untracked_bookkeeping_fails_loudly() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    git_test::init_repo(&repo_root)?;
    std::fs::write(repo_root.join(".gitignore"), ".ralph/done.jsonc\n")?;
    queue::save_queue(&repo_root.join(".ralph/queue.jsonc"), &QueueFile::default())?;
    queue::save_queue(&repo_root.join(".ralph/done.jsonc"), &QueueFile::default())?;
    git_test::git_run(&repo_root, &["add", ".gitignore", ".ralph/queue.jsonc"])?;
    git_test::git_run(&repo_root, &["commit", "-m", "split bookkeeping"])?;

    let err = bookkeeping_authority(&test_resolved(&repo_root))
        .expect_err("split tracked/untracked bookkeeping must fail");

    assert!(err.to_string().contains("split queue/done tracking state"));
    Ok(())
}
