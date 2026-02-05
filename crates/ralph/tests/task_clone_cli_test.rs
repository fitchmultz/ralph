//! Integration tests for `ralph task clone` / `ralph task duplicate`.
//!
//! Responsibilities:
//! - Verify clone creates a new task ID and persists it to the queue.
//! - Verify `--dry-run` does not mutate the queue.
//! - Verify `--title-prefix` and `--status` affect the clone.
//! - Verify failure mode when cloning a missing task.
//!
//! Not handled here:
//! - Full formatting snapshotting of clone stdout (prefer state assertions).
//!
//! Invariants/assumptions:
//! - New task ID increments from max existing numeric suffix in the queue/done set.
//! - Clone inserts into queue at a deterministic position (assert by ID presence, not index).

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

#[test]
fn task_clone_creates_new_task_with_next_id() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Source", TaskStatus::Todo);
    let t2 = test_support::make_test_task("RQ-0002", "Other", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1, t2])?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "clone",
            "RQ-0001",
            "--status",
            "todo",
            "--title-prefix",
            "[CLONE] ",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "clone failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let queue = test_support::read_queue(dir.path())?;
    let cloned = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0003")
        .expect("expected RQ-0003");
    anyhow::ensure!(
        cloned.title.starts_with("[CLONE] "),
        "title-prefix not applied"
    );
    anyhow::ensure!(
        matches!(cloned.status, TaskStatus::Todo),
        "status override not applied"
    );

    Ok(())
}

#[test]
fn task_duplicate_alias_works() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Source", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1])?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "duplicate", "RQ-0001"]);
    anyhow::ensure!(status.success(), "duplicate failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    anyhow::ensure!(
        queue.tasks.iter().any(|t| t.id == "RQ-0002"),
        "expected new task RQ-0002"
    );

    Ok(())
}

#[test]
fn task_clone_dry_run_does_not_mutate() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Source", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[t1])?;

    let before = std::fs::read_to_string(dir.path().join(".ralph/queue.json"))?;
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "clone", "RQ-0001", "--dry-run"]);
    anyhow::ensure!(status.success(), "dry-run clone failed\nstderr:\n{stderr}");
    let after = std::fs::read_to_string(dir.path().join(".ralph/queue.json"))?;
    anyhow::ensure!(before == after, "queue mutated during --dry-run");

    Ok(())
}

#[test]
fn task_clone_missing_task_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "clone", "RQ-9999"]);
    anyhow::ensure!(!status.success(), "expected failure for missing task");
    anyhow::ensure!(
        stderr.to_lowercase().contains("not found"),
        "unexpected stderr:\n{stderr}"
    );
    Ok(())
}

#[test]
fn task_clone_copies_task_fields() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut t1 = test_support::make_test_task("RQ-0001", "Source Task", TaskStatus::Todo);
    t1.priority = ralph::contracts::TaskPriority::High;
    t1.scope = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
    t1.tags = vec!["rust".to_string(), "cli".to_string()];
    test_support::write_queue(dir.path(), &[t1])?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "clone", "RQ-0001"]);
    anyhow::ensure!(status.success(), "clone failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let cloned = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("expected RQ-0002");

    anyhow::ensure!(cloned.title == "Source Task", "title not copied");
    anyhow::ensure!(
        matches!(cloned.priority, ralph::contracts::TaskPriority::High),
        "priority not copied"
    );
    anyhow::ensure!(
        cloned.scope.contains(&"src/main.rs".to_string()),
        "scope not copied"
    );
    anyhow::ensure!(cloned.tags.contains(&"rust".to_string()), "tags not copied");

    Ok(())
}

#[test]
fn task_clone_default_status_is_draft() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let t1 = test_support::make_test_task("RQ-0001", "Source", TaskStatus::Done);
    test_support::write_queue(dir.path(), &[t1])?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "clone", "RQ-0001"]);
    anyhow::ensure!(status.success(), "clone failed\nstderr:\n{stderr}");

    let queue = test_support::read_queue(dir.path())?;
    let cloned = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("expected RQ-0002");
    anyhow::ensure!(
        matches!(cloned.status, TaskStatus::Draft),
        "default status should be draft, got {:?}",
        cloned.status
    );

    Ok(())
}
