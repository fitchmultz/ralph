//! Watch-task regression tests.
//!
//! Responsibilities:
//! - Verify watch task creation, upgrade, deduplication, and reconciliation behavior.
//! - Keep the public watch-task surface covered after modularization.
//!
//! Not handled here:
//! - File watching or debounce orchestration.
//! - Comment parsing behavior outside task handling.
//!
//! Invariants/assumptions:
//! - Tests use isolated temp repos and queue files.
//! - Helper builders preserve the v1/v2 watch identity contracts under test.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use tempfile::{NamedTempFile, TempDir};

use crate::commands::watch::comments::{build_comment_regex, detect_comments};
use crate::commands::watch::identity::{
    WATCH_FIELD_COMMENT_TYPE, WATCH_FIELD_CONTENT_HASH, WATCH_FIELD_FILE, WATCH_FIELD_FINGERPRINT,
    WATCH_FIELD_IDENTITY_KEY, WATCH_FIELD_LINE, WATCH_FIELD_LOCATION_KEY, WATCH_FIELD_VERSION,
    WATCH_VERSION_V2, generate_comment_fingerprint,
};
use crate::commands::watch::types::{CommentType, DetectedComment, WatchOptions};
use crate::config::Resolved;
use crate::contracts::{Config, QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue::{load_queue, save_queue};

use super::handle_detected_comments;
use super::materialize::{create_task_from_comment, generate_task_id};
use super::{reconcile_watch_tasks, task_exists_for_comment};

fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");
    let queue = QueueFile::default();
    let queue_json = serde_json::to_string_pretty(&queue).unwrap();
    std::fs::write(&queue_path, queue_json).unwrap();

    Resolved {
        config: Config::default(),
        repo_root: temp_dir.path().to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

fn watch_opts(auto_queue: bool, close_removed: bool) -> WatchOptions {
    WatchOptions {
        patterns: vec!["*.rs".to_string()],
        debounce_ms: 100,
        auto_queue,
        notify: false,
        ignore_patterns: vec![],
        comment_types: vec![CommentType::All],
        paths: vec![PathBuf::from(".")],
        force: false,
        close_removed,
    }
}

fn detected_comment(
    path: &str,
    line: usize,
    comment_type: CommentType,
    content: &str,
) -> DetectedComment {
    DetectedComment {
        file_path: PathBuf::from(path),
        line_number: line,
        comment_type,
        content: content.to_string(),
        context: format!("{path}:{line} - {content}"),
    }
}

fn task_with_custom_fields(
    id: &str,
    status: TaskStatus,
    custom_fields: HashMap<String, String>,
) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "watch task".to_string(),
        description: None,
        priority: TaskPriority::Medium,
        tags: vec!["watch".to_string(), "todo".to_string()],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        estimated_minutes: None,
        actual_minutes: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields,
        parent_id: None,
    }
}

fn v1_custom_fields(
    path: &str,
    line: usize,
    comment_type: &str,
    content: &str,
) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    fields.insert(WATCH_FIELD_VERSION.to_string(), "1".to_string());
    fields.insert(WATCH_FIELD_FILE.to_string(), path.to_string());
    fields.insert(WATCH_FIELD_LINE.to_string(), line.to_string());
    fields.insert(
        WATCH_FIELD_COMMENT_TYPE.to_string(),
        comment_type.to_string(),
    );
    fields.insert(
        WATCH_FIELD_FINGERPRINT.to_string(),
        generate_comment_fingerprint(content),
    );
    fields
}

#[test]
fn generate_task_id_first_id_format() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);

    let task_id = generate_task_id(&resolved).unwrap();

    assert_eq!(task_id, "RQ-0001");
}

#[test]
fn create_task_from_comment_populates_v2_custom_fields() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let comment = detected_comment(
        "/test/file.rs",
        42,
        CommentType::Todo,
        "Fix the error handling",
    );

    let task = create_task_from_comment(&comment, &resolved).unwrap();

    assert_eq!(
        task.custom_fields.get(WATCH_FIELD_VERSION),
        Some(&WATCH_VERSION_V2.to_string())
    );
    assert_eq!(
        task.custom_fields.get(WATCH_FIELD_FILE),
        Some(&"/test/file.rs".to_string())
    );
    assert_eq!(
        task.custom_fields.get(WATCH_FIELD_LINE),
        Some(&"42".to_string())
    );
    assert_eq!(
        task.custom_fields.get(WATCH_FIELD_COMMENT_TYPE),
        Some(&"todo".to_string())
    );
    assert!(task.custom_fields.contains_key(WATCH_FIELD_CONTENT_HASH));
    assert!(task.custom_fields.contains_key(WATCH_FIELD_LOCATION_KEY));
    assert!(task.custom_fields.contains_key(WATCH_FIELD_IDENTITY_KEY));
}

#[test]
fn create_task_from_comment_is_mode_independent() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let mut source = NamedTempFile::new().unwrap();
    writeln!(source, "// TODO: normalize watch output").unwrap();
    source.flush().unwrap();

    let all_regex = build_comment_regex(&[CommentType::All]).unwrap();
    let todo_regex = build_comment_regex(&[CommentType::Todo]).unwrap();

    let all_comments = detect_comments(source.path(), &all_regex).unwrap();
    let todo_comments = detect_comments(source.path(), &todo_regex).unwrap();

    assert_eq!(all_comments.len(), 1);
    assert_eq!(todo_comments.len(), 1);

    let all_task = create_task_from_comment(&all_comments[0], &resolved).unwrap();
    let todo_task = create_task_from_comment(&todo_comments[0], &resolved).unwrap();

    assert_eq!(all_task.title, todo_task.title);
    assert_eq!(all_task.notes, todo_task.notes);
    assert_eq!(all_task.scope, todo_task.scope);
    assert_eq!(
        all_task.custom_fields.get(WATCH_FIELD_CONTENT_HASH),
        todo_task.custom_fields.get(WATCH_FIELD_CONTENT_HASH)
    );
    assert_eq!(
        all_task.custom_fields.get(WATCH_FIELD_IDENTITY_KEY),
        todo_task.custom_fields.get(WATCH_FIELD_IDENTITY_KEY)
    );
}

#[test]
fn repeated_scan_of_unchanged_file_does_not_duplicate_task() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let comments = vec![detected_comment(
        "/src/a.rs",
        10,
        CommentType::Todo,
        "fix this",
    )];
    let processed_files = vec![PathBuf::from("/src/a.rs")];

    handle_detected_comments(
        &resolved,
        &comments,
        &processed_files,
        &watch_opts(true, false),
    )
    .unwrap();
    handle_detected_comments(
        &resolved,
        &comments,
        &processed_files,
        &watch_opts(true, false),
    )
    .unwrap();

    let queue = load_queue(&resolved.queue_path).unwrap();
    assert_eq!(queue.tasks.len(), 1);
}

#[test]
fn removing_one_of_identical_comments_only_closes_its_task() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let comments = vec![
        detected_comment("/src/a.rs", 10, CommentType::Todo, "fix this"),
        detected_comment("/src/b.rs", 20, CommentType::Todo, "fix this"),
    ];

    handle_detected_comments(
        &resolved,
        &comments,
        &[PathBuf::from("/src/a.rs"), PathBuf::from("/src/b.rs")],
        &watch_opts(true, false),
    )
    .unwrap();

    handle_detected_comments(
        &resolved,
        &[],
        &[PathBuf::from("/src/a.rs")],
        &watch_opts(true, true),
    )
    .unwrap();

    let queue = load_queue(&resolved.queue_path).unwrap();
    let a_task = queue
        .tasks
        .iter()
        .find(|task| task.custom_fields.get(WATCH_FIELD_FILE) == Some(&"/src/a.rs".to_string()))
        .unwrap();
    let b_task = queue
        .tasks
        .iter()
        .find(|task| task.custom_fields.get(WATCH_FIELD_FILE) == Some(&"/src/b.rs".to_string()))
        .unwrap();

    assert_eq!(a_task.status, TaskStatus::Done);
    assert_eq!(b_task.status, TaskStatus::Todo);
}

#[test]
fn legacy_v1_task_is_upgraded_in_place_when_comment_still_exists() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let mut queue = QueueFile::default();
    queue.tasks.push(task_with_custom_fields(
        "RQ-0001",
        TaskStatus::Todo,
        v1_custom_fields("/src/a.rs", 10, "todo", "fix this"),
    ));
    save_queue(&resolved.queue_path, &queue).unwrap();

    handle_detected_comments(
        &resolved,
        &[detected_comment(
            "/src/a.rs",
            10,
            CommentType::Todo,
            "fix this",
        )],
        &[PathBuf::from("/src/a.rs")],
        &watch_opts(true, false),
    )
    .unwrap();

    let queue = load_queue(&resolved.queue_path).unwrap();
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(
        queue.tasks[0].custom_fields.get(WATCH_FIELD_VERSION),
        Some(&WATCH_VERSION_V2.to_string())
    );
    assert!(
        queue.tasks[0]
            .custom_fields
            .contains_key(WATCH_FIELD_IDENTITY_KEY)
    );
}

#[test]
fn task_exists_for_comment_ignores_done_and_rejected_watch_tasks() {
    let comment = detected_comment("/src/a.rs", 10, CommentType::Todo, "fix this");
    let mut queue = QueueFile::default();
    queue.tasks.push(task_with_custom_fields(
        "RQ-0001",
        TaskStatus::Done,
        v1_custom_fields("/src/a.rs", 10, "todo", "fix this"),
    ));
    queue.tasks.push(task_with_custom_fields(
        "RQ-0002",
        TaskStatus::Rejected,
        v1_custom_fields("/src/a.rs", 10, "todo", "fix this"),
    ));

    assert!(!task_exists_for_comment(&queue, &comment));
}

#[test]
fn reconcile_only_closes_tasks_for_processed_files() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let mut queue = QueueFile::default();
    queue.tasks.push(
        create_task_from_comment(
            &detected_comment("/src/a.rs", 10, CommentType::Todo, "fix this"),
            &resolved,
        )
        .unwrap(),
    );
    queue.tasks.push(
        create_task_from_comment(
            &detected_comment("/src/b.rs", 20, CommentType::Todo, "fix this"),
            &resolved,
        )
        .unwrap(),
    );
    save_queue(&resolved.queue_path, &queue).unwrap();

    let closed = reconcile_watch_tasks(
        &resolved,
        &[],
        &[PathBuf::from("/src/a.rs")],
        &watch_opts(true, true),
    )
    .unwrap();

    assert_eq!(closed.len(), 1);
    let queue = load_queue(&resolved.queue_path).unwrap();
    let a_task = queue
        .tasks
        .iter()
        .find(|task| task.custom_fields.get(WATCH_FIELD_FILE) == Some(&"/src/a.rs".to_string()))
        .unwrap();
    let b_task = queue
        .tasks
        .iter()
        .find(|task| task.custom_fields.get(WATCH_FIELD_FILE) == Some(&"/src/b.rs".to_string()))
        .unwrap();
    assert_eq!(a_task.status, TaskStatus::Done);
    assert_eq!(b_task.status, TaskStatus::Todo);
}
