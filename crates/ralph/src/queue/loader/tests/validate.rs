//! Queue loader validation-flow tests.
//!
//! Purpose:
//! - Queue loader validation-flow tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
fn load_and_validate_queues_allows_missing_done_file() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let queue_path = ralph_dir.join("queue.json");
    save_queue(
        &queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        },
    )?;
    let done_path = ralph_dir.join("done.json");

    let resolved = resolved_with_paths(repo_root, queue_path, done_path);

    let (queue, done) = load_and_validate_queues(&resolved, true)?;
    assert_eq!(queue.tasks.len(), 1);
    assert!(done.is_some());
    assert!(done.expect("done queue").tasks.is_empty());
    Ok(())
}

#[test]
fn load_and_validate_queues_rejects_duplicate_ids_across_done() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let queue_path = ralph_dir.join("queue.json");
    save_queue(
        &queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        },
    )?;
    let done_path = ralph_dir.join("done.json");
    save_queue(
        &done_path,
        &QueueFile {
            version: 1,
            tasks: vec![{
                let mut t = task("RQ-0001");
                t.status = TaskStatus::Done;
                t.completed_at = Some("2026-01-18T00:00:00Z".to_string());
                t
            }],
        },
    )?;

    let resolved = resolved_with_paths(repo_root, queue_path, done_path);

    let err = load_and_validate_queues(&resolved, true).expect_err("expected duplicate id error");
    assert!(
        err.to_string()
            .contains("Duplicate task ID detected across queue and done")
    );
    Ok(())
}

#[test]
fn load_and_validate_queues_rejects_invalid_deps_when_include_done_false() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    save_queue(
        &queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![{
                let mut t = task("RQ-0001");
                t.depends_on = vec!["RQ-9999".to_string()];
                t
            }],
        },
    )?;

    let done_path = ralph_dir.join("done.json");
    let resolved = resolved_with_paths(repo_root, queue_path, done_path);

    let err =
        load_and_validate_queues(&resolved, false).expect_err("should fail on invalid dependency");
    assert!(
        err.to_string().contains("Invalid dependency"),
        "Error should mention invalid dependency: {}",
        err
    );

    Ok(())
}

#[test]
fn load_and_validate_queues_rejects_non_utc_timestamps_without_persisting() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let mut active_task = task("RQ-0001");
    active_task.created_at = Some("2026-01-18T12:00:00-05:00".to_string());
    active_task.updated_at = Some("2026-01-18T13:00:00-05:00".to_string());
    save_queue(
        &queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![active_task],
        },
    )?;

    let mut done_task = task("RQ-0002");
    done_task.status = TaskStatus::Done;
    done_task.created_at = Some("2026-01-18T10:00:00-07:00".to_string());
    done_task.updated_at = Some("2026-01-18T11:00:00-07:00".to_string());
    done_task.completed_at = Some("2026-01-18T12:00:00-07:00".to_string());
    save_queue(
        &done_path,
        &QueueFile {
            version: 1,
            tasks: vec![done_task],
        },
    )?;

    let resolved = resolved_with_paths(repo_root, queue_path.clone(), done_path.clone());

    let err = load_and_validate_queues(&resolved, true)
        .expect_err("non-UTC timestamps should fail without explicit repair");
    let err_msg = format!("{err:#}");
    assert!(
        err_msg.contains("must be a valid RFC3339 UTC timestamp"),
        "unexpected error message: {err_msg}"
    );

    let persisted_queue = load_queue(&queue_path)?;
    let persisted_done = load_queue(&done_path)?;
    assert_eq!(
        persisted_queue.tasks[0].created_at.as_deref(),
        Some("2026-01-18T12:00:00-05:00")
    );
    assert_eq!(
        persisted_done.tasks[0].completed_at.as_deref(),
        Some("2026-01-18T12:00:00-07:00")
    );

    Ok(())
}

#[test]
fn load_and_validate_queues_rejects_missing_terminal_completed_at_without_persisting() -> Result<()>
{
    let temp = TempDir::new()?;
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let mut queue_task = task("RQ-0001");
    queue_task.status = TaskStatus::Done;
    queue_task.completed_at = None;
    save_queue(
        &queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![queue_task],
        },
    )?;
    save_queue(&done_path, &QueueFile::default())?;

    let resolved = resolved_with_paths(repo_root, queue_path.clone(), done_path);

    let err = load_and_validate_queues(&resolved, true)
        .expect_err("missing completed_at should fail without explicit repair");
    let err_msg = format!("{err:#}");
    assert!(
        err_msg.contains("Missing completed_at"),
        "unexpected error message: {err_msg}"
    );

    let persisted_queue = load_queue(&queue_path)?;
    assert!(
        persisted_queue.tasks[0].completed_at.is_none(),
        "read-only validation must not backfill completed_at"
    );

    Ok(())
}

#[test]
fn load_queue_with_repair_and_validate_rejects_missing_timestamps() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let malformed = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': [], 'plan': [],}]}"#;
    std::fs::write(&queue_path, malformed)?;

    let result = load_queue_with_repair_and_validate(&queue_path, None, "RQ", 4, 10);

    let err = result.expect_err("should fail validation due to missing timestamps");
    let err_msg = err
        .chain()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        err_msg.contains("created_at") || err_msg.contains("updated_at"),
        "Error should mention missing timestamp: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn load_queue_with_repair_and_validate_accepts_valid_repair() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let malformed = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': ['observed',], 'plan': ['do thing',], 'created_at': '2026-01-18T00:00:00Z', 'updated_at': '2026-01-18T00:00:00Z',}]}"#;
    std::fs::write(&queue_path, malformed)?;

    let (queue, warnings) = load_queue_with_repair_and_validate(&queue_path, None, "RQ", 4, 10)?;

    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[0].title, "Test task");
    assert_eq!(queue.tasks[0].tags, vec!["bug"]);
    assert!(warnings.is_empty());

    Ok(())
}

#[test]
fn load_queue_with_repair_and_validate_detects_done_queue_issues() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");
    let done_path = temp.path().join("done.json");

    let active_malformed = r#"{'version': 1, 'tasks': [{'id': 'RQ-0002', 'title': 'Second task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': [], 'plan': [], 'created_at': '2026-01-18T00:00:00Z', 'updated_at': '2026-01-18T00:00:00Z', 'depends_on': ['RQ-0001',],}]}"#;
    std::fs::write(&queue_path, active_malformed)?;

    let done_queue = QueueFile {
        version: 1,
        tasks: vec![{
            let mut t = task("RQ-0001");
            t.status = TaskStatus::Done;
            t.completed_at = Some("2026-01-18T00:00:00Z".to_string());
            t
        }],
    };
    save_queue(&done_path, &done_queue)?;

    let (queue, warnings) =
        load_queue_with_repair_and_validate(&queue_path, Some(&done_queue), "RQ", 4, 10)?;

    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0002");
    assert!(warnings.is_empty());

    Ok(())
}
