//! Shared helpers for integration tests.
//!
//! This module centralizes test-only helpers that are reused across multiple integration-test
//! crates under `crates/ralph/tests/`.

use ralph::config;
use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use std::env;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[allow(dead_code)]
pub fn path_has_repo_markers(path: &Path) -> bool {
    path.ancestors()
        .any(|dir| dir.join(".git").exists() || dir.join(".ralph").is_dir())
}

#[allow(dead_code)]
pub fn find_non_repo_temp_base() -> PathBuf {
    let cwd = env::current_dir().expect("resolve current dir");
    let repo_root = config::find_repo_root(&cwd);
    let mut candidates = Vec::new();
    if let Some(parent) = repo_root.parent() {
        candidates.push(parent.to_path_buf());
    }
    candidates.push(env::temp_dir());
    candidates.push(PathBuf::from("/tmp"));

    for candidate in candidates {
        if candidate.as_os_str().is_empty() {
            continue;
        }
        if !path_has_repo_markers(&candidate) {
            return candidate;
        }
    }

    repo_root
}

#[allow(dead_code)]
pub fn temp_dir_outside_repo() -> TempDir {
    let base = find_non_repo_temp_base();
    std::fs::create_dir_all(&base).expect("ensure temp base exists");
    TempDir::new_in(&base).expect("create temp dir outside repo")
}

/// Helper to create a test task.
///
/// The fields are intentionally fully-populated so contract/rendering tests can rely on realistic
/// data without repeating boilerplate.
#[allow(dead_code)]
pub fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    let completed_at = match status {
        TaskStatus::Done | TaskStatus::Rejected => Some("2026-01-19T00:00:00Z".to_string()),
        _ => None,
    };
    Task {
        id: id.to_string(),
        title: title.to_string(),
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test evidence".to_string()],
        plan: vec!["test plan".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at,
        depends_on: vec![],
        custom_fields: std::collections::HashMap::new(),
    }
}

/// Helper to create a test queue with multiple tasks.
#[allow(dead_code)]
pub fn make_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

/// Rendering-focused task fixture.
///
/// This matches historical fixtures embedded in `tui_rendering_test.rs`:
/// - `plan` has two steps (to exercise multi-step rendering)
/// - `completed_at` is intentionally `None` even for Done/Rejected tasks (rendering tests
///   explicitly control timestamp sections when needed)
#[allow(dead_code)]
pub fn make_render_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    let mut task = make_test_task(id, title, status);
    task.plan = vec![
        "test plan step 1".to_string(),
        "test plan step 2".to_string(),
    ];
    task.completed_at = None;
    task
}

/// Rendering-focused queue fixture (uses `make_render_test_task`).
#[allow(dead_code)]
pub fn make_render_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_render_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_render_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_render_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}
