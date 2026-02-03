//! Integration tests for `ralph task show/details` CLI behavior.
//!
//! Responsibilities:
//! - Validate queue + done lookups for task detail output.
//! - Confirm the `details` alias resolves to the show handler.
//!
//! Not handled here:
//! - Exhaustive formatting or color output verification.
//! - TUI rendering or runner execution paths.
//!
//! Invariants/assumptions:
//! - The Ralph binary is available via CARGO_BIN_EXE_ralph or adjacent to the test binary.
//! - Repo root discovery works via a `.ralph/queue.json` file in the current directory.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, ExitStatus};

mod test_support;

use ralph::contracts::{QueueFile, Task, TaskStatus};

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir)
        .env_remove("RALPH_REPO_ROOT_OVERRIDE")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn make_task(id: &str, status: TaskStatus, title: &str) -> Task {
    let completed_at = matches!(status, TaskStatus::Done | TaskStatus::Rejected)
        .then_some("2026-01-19T00:00:00Z".to_string());
    Task {
        id: id.to_string(),
        status,
        title: title.to_string(),
        priority: ralph::contracts::TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["evidence".to_string()],
        plan: vec!["plan".to_string()],
        notes: vec![],
        request: Some("request".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

fn make_queue_file(tasks: Vec<Task>) -> QueueFile {
    QueueFile { version: 1, tasks }
}

#[test]
fn task_show_finds_task_in_queue() {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path()).expect("git init");

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    assert!(status.success(), "ralph init failed: {}", stderr);

    let queue = make_queue_file(vec![
        make_task("RQ-0001", TaskStatus::Todo, "First task"),
        make_task("RQ-0002", TaskStatus::Doing, "Second task"),
    ]);
    let queue_path = dir.path().join(".ralph/queue.json");
    let json = serde_json::to_string_pretty(&queue).expect("serialize queue");
    std::fs::write(&queue_path, json).expect("write queue.json");

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["task", "show", "RQ-0001"]);
    assert!(status.success(), "task show failed: {}", stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("RQ-0001") && combined.contains("First task"),
        "expected task details in output: {}",
        combined
    );
}

#[test]
fn task_show_finds_task_in_done() {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path()).expect("git init");

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    assert!(status.success(), "ralph init failed: {}", stderr);

    let done = make_queue_file(vec![make_task(
        "RQ-0001",
        TaskStatus::Done,
        "Completed task",
    )]);
    let done_path = dir.path().join(".ralph/done.json");
    let json = serde_json::to_string_pretty(&done).expect("serialize done");
    std::fs::write(&done_path, json).expect("write done.json");

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["task", "show", "RQ-0001"]);
    assert!(status.success(), "task show failed: {}", stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("RQ-0001") && combined.contains("Completed task"),
        "expected task details in output: {}",
        combined
    );
}

#[test]
fn task_show_details_alias_works() {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path()).expect("git init");

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    assert!(status.success(), "ralph init failed: {}", stderr);

    let queue = make_queue_file(vec![make_task("RQ-0001", TaskStatus::Todo, "Alias test")]);
    let queue_path = dir.path().join(".ralph/queue.json");
    let json = serde_json::to_string_pretty(&queue).expect("serialize queue");
    std::fs::write(&queue_path, json).expect("write queue.json");

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["task", "details", "RQ-0001"]);
    assert!(status.success(), "task details failed: {}", stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("RQ-0001") && combined.contains("Alias test"),
        "expected task details via alias: {}",
        combined
    );
}

#[test]
fn task_show_reports_missing_task() {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path()).expect("git init");

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    assert!(status.success(), "ralph init failed: {}", stderr);

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["task", "show", "RQ-9999"]);
    assert!(!status.success(), "expected failure for missing task");
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(
        combined.contains("not found") || combined.contains("No task"),
        "expected 'not found' message: {}",
        combined
    );
}
