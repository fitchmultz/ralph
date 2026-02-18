//! Tests for merge runner functionality.
//!
//! Responsibilities:
//! - Unit tests for validation and merge operations.
//! - Integration tests for conflict detection and resolution.
//!
//! Not handled here:
//! - Actual AI runner invocation (mocked).
//! - GitHub API interactions (tested separately in git module).
//! - Completion signal handling (deprecated - now handled by merge-agent subprocess).

use super::*;
use crate::commands::run::parallel::merge_runner::conflict::{
    conflict_files, prepare_and_merge, validate_queue_done_in_workspace,
};
use crate::contracts::{Config, QueueFile, Task, TaskStatus};
use crate::testsupport::git as git_test;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn validate_pr_head_accepts_exact_match() {
    assert!(validate_pr_head("ralph/", "RQ-0001", "ralph/RQ-0001").is_ok());
    assert!(validate_pr_head("ralph/", "RQ-0001", " ralph/RQ-0001 ").is_ok());
}

#[test]
fn validate_pr_head_rejects_prefix_mismatch() {
    let err = validate_pr_head("ralph/", "RQ-0001", "feature/RQ-0001")
        .expect_err("expected mismatch error");
    assert!(err.contains("expected 'ralph/RQ-0001'"));
}

#[test]
fn validate_pr_head_rejects_task_id_with_path_separators() {
    let err = validate_pr_head("ralph/", "RQ/0001", "ralph/RQ/0001")
        .expect_err("expected path separator error");
    assert!(err.contains("invalid task_id"));
}

#[test]
fn validate_pr_head_rejects_task_id_with_parent_reference() {
    let err = validate_pr_head("ralph/", "RQ-..", "ralph/RQ-..")
        .expect_err("expected parent reference error");
    assert!(err.contains("invalid task_id"));
}

/// Setup a test scenario with a bare remote and two branches that can be merged.
/// Returns (remote_dir, author_dir, workspace_dir) so the remote path stays alive.
fn setup_merge_test() -> (TempDir, TempDir, TempDir) {
    // Create directories: remote (bare), author repo, workspace repo
    let remote_dir = TempDir::new().unwrap();
    let author_dir = TempDir::new().unwrap();
    let workspace_dir = TempDir::new().unwrap();

    // Initialize bare remote
    git_test::init_bare_repo(remote_dir.path()).unwrap();

    // Setup author repo with remote
    git_test::init_repo(author_dir.path()).unwrap();
    git_test::add_remote(author_dir.path(), "origin", remote_dir.path()).unwrap();

    // Create initial commit and push to main
    fs::write(author_dir.path().join("README.md"), "# Initial").unwrap();
    git_test::commit_all(author_dir.path(), "Initial commit").unwrap();
    git_test::git_run(author_dir.path(), &["push", "-u", "origin", "HEAD:main"]).unwrap();

    (remote_dir, author_dir, workspace_dir)
}

/// Clone from the bare remote into the workspace directory.
fn clone_from_remote(remote_dir: &std::path::Path, workspace_dir: &std::path::Path) -> Result<()> {
    git_test::clone_repo(remote_dir, workspace_dir)
}

#[test]
fn merge_runner_clean_merge_succeeds() {
    let (remote_dir, author_dir, workspace_dir) = setup_merge_test();

    // Create a feature branch from main
    git_test::git_run(author_dir.path(), &["checkout", "-b", "feature"]).unwrap();
    fs::write(author_dir.path().join("feature.txt"), "feature content").unwrap();
    git_test::commit_all(author_dir.path(), "Add feature").unwrap();
    git_test::push_branch(author_dir.path(), "feature").unwrap();

    // Go back to main and add a non-conflicting commit
    git_test::git_run(author_dir.path(), &["checkout", "main"]).unwrap();
    fs::write(author_dir.path().join("main.txt"), "main content").unwrap();
    git_test::commit_all(author_dir.path(), "Add main content").unwrap();
    git_test::push_branch(author_dir.path(), "main").unwrap();

    // Clone workspace from the bare remote
    clone_from_remote(remote_dir.path(), workspace_dir.path()).unwrap();
    git_test::configure_user(workspace_dir.path()).unwrap();

    // Run prepare_and_merge
    let outcome = prepare_and_merge(workspace_dir.path(), "feature", "main").unwrap();

    // Should be clean merge
    assert!(
        matches!(outcome, crate::git::error::GitMergeOutcome::Clean),
        "Expected clean merge outcome"
    );

    // No conflicts should be detected
    let conflicts = conflict_files(workspace_dir.path()).unwrap();
    assert!(
        conflicts.is_empty(),
        "Expected no conflicts, got: {:?}",
        conflicts
    );
}

#[test]
fn merge_runner_conflicted_merge_continues() {
    let (remote_dir, author_dir, workspace_dir) = setup_merge_test();

    // Create a feature branch and modify a file
    git_test::git_run(author_dir.path(), &["checkout", "-b", "feature"]).unwrap();
    fs::write(author_dir.path().join("shared.txt"), "feature version").unwrap();
    git_test::commit_all(author_dir.path(), "Feature change").unwrap();
    git_test::push_branch(author_dir.path(), "feature").unwrap();

    // Go back to main and modify the same file differently
    git_test::git_run(author_dir.path(), &["checkout", "main"]).unwrap();
    fs::write(author_dir.path().join("shared.txt"), "main version").unwrap();
    git_test::commit_all(author_dir.path(), "Main change").unwrap();
    git_test::push_branch(author_dir.path(), "main").unwrap();

    // Clone workspace from the bare remote
    clone_from_remote(remote_dir.path(), workspace_dir.path()).unwrap();
    git_test::configure_user(workspace_dir.path()).unwrap();

    // Run prepare_and_merge
    let outcome = prepare_and_merge(workspace_dir.path(), "feature", "main").unwrap();

    // Should report conflicts
    assert!(
        matches!(
            outcome,
            crate::git::error::GitMergeOutcome::Conflicts { .. }
        ),
        "Expected conflict outcome"
    );

    // Conflicts should be detected
    let conflicts = conflict_files(workspace_dir.path()).unwrap();
    assert_eq!(
        conflicts,
        vec!["shared.txt"],
        "Expected conflict in shared.txt"
    );
}

#[test]
fn merge_runner_nonexistent_target_fails() {
    let (remote_dir, author_dir, workspace_dir) = setup_merge_test();

    // Create and push a feature branch
    git_test::git_run(author_dir.path(), &["checkout", "-b", "feature"]).unwrap();
    fs::write(author_dir.path().join("feature.txt"), "feature").unwrap();
    git_test::commit_all(author_dir.path(), "Feature commit").unwrap();
    git_test::push_branch(author_dir.path(), "feature").unwrap();

    // Clone workspace from the bare remote
    clone_from_remote(remote_dir.path(), workspace_dir.path()).unwrap();
    git_test::configure_user(workspace_dir.path()).unwrap();

    // Try to merge a non-existent branch (origin/nonexistent)
    // This will produce exit code 1 with "not something we can merge" in stderr,
    // which our helper treats as Conflicts. We then verify that conflict_files
    // returns empty, which triggers the error path in resolve_conflicts.
    let outcome = prepare_and_merge(workspace_dir.path(), "feature", "nonexistent").unwrap();

    // The merge returns Conflicts outcome for exit code 1
    assert!(
        matches!(
            outcome,
            crate::git::error::GitMergeOutcome::Conflicts { .. }
        ),
        "Expected conflict outcome for non-existent branch"
    );

    // But there are no actual conflict files
    let conflicts = conflict_files(workspace_dir.path()).unwrap();
    assert!(
        conflicts.is_empty(),
        "Non-existent branch should not produce conflict files"
    );

    // The resolve_conflicts function would detect this mismatch and error.
    // This verifies we don't mask real merge failures.
}

// Tests for workspace queue/done validation (RQ-0561)

fn build_test_task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        description: None,
        priority: Default::default(),
        tags: vec!["test".to_string()],
        scope: vec!["file.rs".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: if status == TaskStatus::Done {
            Some("2026-01-18T00:00:00Z".to_string())
        } else {
            None
        },
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

fn build_test_resolved(
    original_repo_root: &std::path::Path,
    queue_path: PathBuf,
    done_path: PathBuf,
) -> crate::config::Resolved {
    crate::config::Resolved {
        config: Config::default(),
        repo_root: original_repo_root.to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

fn save_queue_file(path: &std::path::Path, queue: &QueueFile) {
    let json = serde_json::to_string_pretty(queue).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, json).unwrap();
}

#[test]
fn validate_queue_done_missing_done_file_allowed() {
    let original_repo = TempDir::new().unwrap();
    let workspace_repo = TempDir::new().unwrap();

    let ralph_dir = original_repo.path().join(".ralph");
    fs::create_dir_all(&ralph_dir).unwrap();

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    // Create valid queue in workspace
    let workspace_ralph = workspace_repo.path().join(".ralph");
    fs::create_dir_all(&workspace_ralph).unwrap();
    let workspace_queue = workspace_ralph.join("queue.json");

    let valid_queue = QueueFile {
        version: 1,
        tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
    };
    save_queue_file(&workspace_queue, &valid_queue);
    // Note: no done.json in workspace

    let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

    let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
    assert!(
        result.is_ok(),
        "Missing done file should be allowed: {:?}",
        result
    );
}

#[test]
fn validate_queue_done_invalid_json_in_queue_rejected() {
    let original_repo = TempDir::new().unwrap();
    let workspace_repo = TempDir::new().unwrap();

    let ralph_dir = original_repo.path().join(".ralph");
    fs::create_dir_all(&ralph_dir).unwrap();

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    // Create invalid JSON in workspace queue
    let workspace_ralph = workspace_repo.path().join(".ralph");
    fs::create_dir_all(&workspace_ralph).unwrap();
    let workspace_queue = workspace_ralph.join("queue.json");
    fs::write(&workspace_queue, "not valid json").unwrap();

    let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

    let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
    assert!(result.is_err(), "Invalid JSON in queue should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("validate queue file") || err_msg.contains("parse"),
        "Error should indicate validation failure: {}",
        err_msg
    );
}

#[test]
fn validate_queue_done_invalid_json_in_done_rejected() {
    let original_repo = TempDir::new().unwrap();
    let workspace_repo = TempDir::new().unwrap();

    let ralph_dir = original_repo.path().join(".ralph");
    fs::create_dir_all(&ralph_dir).unwrap();

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let workspace_ralph = workspace_repo.path().join(".ralph");
    fs::create_dir_all(&workspace_ralph).unwrap();

    // Valid queue
    let workspace_queue = workspace_ralph.join("queue.json");
    let valid_queue = QueueFile {
        version: 1,
        tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
    };
    save_queue_file(&workspace_queue, &valid_queue);

    // Invalid done file
    let workspace_done = workspace_ralph.join("done.json");
    fs::write(&workspace_done, "not valid json").unwrap();

    let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

    let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
    assert!(result.is_err(), "Invalid JSON in done should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("load done file") || err_msg.contains("parse"),
        "Error should indicate done file failure: {}",
        err_msg
    );
}

#[test]
fn validate_queue_done_duplicate_ids_rejected() {
    let original_repo = TempDir::new().unwrap();
    let workspace_repo = TempDir::new().unwrap();

    let ralph_dir = original_repo.path().join(".ralph");
    fs::create_dir_all(&ralph_dir).unwrap();

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let workspace_ralph = workspace_repo.path().join(".ralph");
    fs::create_dir_all(&workspace_ralph).unwrap();

    // Queue with RQ-0001
    let workspace_queue = workspace_ralph.join("queue.json");
    let queue = QueueFile {
        version: 1,
        tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
    };
    save_queue_file(&workspace_queue, &queue);

    // Done file also with RQ-0001 (duplicate!)
    let workspace_done = workspace_ralph.join("done.json");
    let done = QueueFile {
        version: 1,
        tasks: vec![build_test_task("RQ-0001", TaskStatus::Done)],
    };
    save_queue_file(&workspace_done, &done);

    let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

    let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
    assert!(result.is_err(), "Duplicate IDs should be rejected");
    let err = result.unwrap_err();
    // Check the full error chain since the message is in a context layer
    let full_error = err
        .chain()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        full_error.contains("Duplicate task ID detected across queue and done"),
        "Error chain should mention duplicate ID: {}",
        full_error
    );
}

// Test for merge blocker on head mismatch (RQ-0592)
#[test]
fn handle_work_item_returns_blocker_on_head_mismatch() {
    // Create a minimal resolved config with default branch prefix
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path().join("repo");
    fs::create_dir_all(&repo_root).unwrap();
    let ralph_dir = repo_root.join(".ralph");
    fs::create_dir_all(&ralph_dir).unwrap();

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    // Write minimal queue file
    let queue = QueueFile {
        version: 1,
        tasks: vec![build_test_task("RQ-0001", TaskStatus::Doing)],
    };
    save_queue_file(&queue_path, &queue);

    let resolved = build_test_resolved(&repo_root, queue_path, done_path);

    // Create a work item with mismatched head (wrong prefix)
    let work_item = MergeWorkItem {
        task_id: "RQ-0001".to_string(),
        pr: crate::git::PrInfo {
            number: 1,
            url: "https://example.com/pr/1".to_string(),
            head: "wrong-prefix/RQ-0001".to_string(), // Mismatched!
            base: "main".to_string(),
        },
        workspace_path: None,
    };

    // Stop signal (not stopped)
    let merge_stop = std::sync::atomic::AtomicBool::new(false);

    // Call handle_work_item
    let result = handle_work_item(
        &resolved,
        work_item,
        crate::contracts::ParallelMergeMethod::Squash,
        crate::contracts::ConflictPolicy::Reject,
        crate::contracts::MergeRunnerConfig::default(),
        3,
        &temp.path().join("workspaces"),
        false,
        &merge_stop,
    );

    // Should return Ok(Some(MergeResult { merged: false, merge_blocker: Some(...), ... }))
    let result = result.expect("handle_work_item should not error");
    assert!(result.is_some(), "should return a MergeResult for blocker");

    let merge_result = result.unwrap();
    assert!(!merge_result.merged, "merged should be false");
    assert!(
        merge_result.merge_blocker.is_some(),
        "merge_blocker should be set"
    );
    assert!(
        merge_result
            .merge_blocker
            .as_ref()
            .unwrap()
            .contains("head mismatch"),
        "blocker should mention head mismatch: {}",
        merge_result.merge_blocker.as_ref().unwrap()
    );
    assert!(merge_result.queue_bytes.is_none());
    assert!(merge_result.done_bytes.is_none());
    assert!(merge_result.productivity_bytes.is_none());
}
