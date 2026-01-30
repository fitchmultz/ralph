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
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use ralph::contracts::{QueueFile, Task, TaskStatus};

fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
        .current_dir(dir)
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
        priority: Default::default(),
        tags: vec!["cli".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["verify".to_string()],
        notes: vec![],
        request: Some("test".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        scheduled_start: None,
        completed_at,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
    }
}

fn write_queue(root: &Path, tasks: Vec<Task>) {
    let queue = QueueFile { version: 1, tasks };
    let rendered = serde_json::to_string_pretty(&queue).expect("serialize queue");
    std::fs::write(root.join(".ralph/queue.json"), rendered).expect("write queue.json");
}

fn write_done(root: &Path, tasks: Vec<Task>) {
    let done = QueueFile { version: 1, tasks };
    let rendered = serde_json::to_string_pretty(&done).expect("serialize done");
    std::fs::write(root.join(".ralph/done.json"), rendered).expect("write done.json");
}

fn setup_repo() -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().expect("temp dir");
    std::fs::create_dir_all(temp.path().join(".ralph")).expect("create .ralph dir");
    temp
}

#[test]
fn task_show_reads_from_queue() {
    let temp = setup_repo();
    write_queue(
        temp.path(),
        vec![make_task("RQ-0001", TaskStatus::Todo, "Test task")],
    );
    write_done(
        temp.path(),
        vec![make_task("RQ-0002", TaskStatus::Done, "Archived task")],
    );

    let (status, stdout, stderr) = run_in_dir(temp.path(), &["task", "show", "RQ-0001"]);
    assert!(
        status.success(),
        "expected task show to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("parse json output");
    assert_eq!(payload["id"], "RQ-0001");
    assert_eq!(payload["title"], "Test task");
}

#[test]
fn task_show_reads_from_done() {
    let temp = setup_repo();
    write_queue(
        temp.path(),
        vec![make_task("RQ-0001", TaskStatus::Todo, "Test task")],
    );
    write_done(
        temp.path(),
        vec![make_task("RQ-0002", TaskStatus::Done, "Archived task")],
    );

    let (status, stdout, stderr) = run_in_dir(temp.path(), &["task", "show", "RQ-0002"]);
    assert!(
        status.success(),
        "expected task show to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let payload: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("parse json output");
    assert_eq!(payload["id"], "RQ-0002");
    assert_eq!(payload["title"], "Archived task");
}

#[test]
fn task_show_reports_missing_task() {
    let temp = setup_repo();
    write_queue(
        temp.path(),
        vec![make_task("RQ-0001", TaskStatus::Todo, "Test task")],
    );
    write_done(temp.path(), vec![]);

    let (status, stdout, stderr) = run_in_dir(temp.path(), &["task", "show", "RQ-9999"]);
    assert!(
        !status.success(),
        "expected task show to fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("task not found: RQ-9999"),
        "missing error message: {combined}"
    );
}

#[test]
fn task_details_alias_supports_compact() {
    let temp = setup_repo();
    write_queue(
        temp.path(),
        vec![make_task("RQ-0001", TaskStatus::Todo, "Test task")],
    );
    write_done(temp.path(), vec![]);

    let (status, stdout, stderr) = run_in_dir(
        temp.path(),
        &["task", "details", "RQ-0001", "--format", "compact"],
    );
    assert!(
        status.success(),
        "expected task details to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("RQ-0001"), "missing task id: {stdout}");
    assert!(stdout.contains("Test task"), "missing task title: {stdout}");
}
