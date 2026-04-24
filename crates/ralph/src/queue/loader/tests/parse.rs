//! Queue loader parsing and repair edge-case tests.
//!
//! Purpose:
//! - Queue loader parsing and repair edge-case tests.
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
fn load_and_validate_queues_rejects_malformed_timestamps_without_rewrite() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let mut bad_task = task("RQ-0001");
    bad_task.created_at = Some("not-a-timestamp".to_string());
    save_queue(
        &queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![bad_task],
        },
    )?;

    let resolved = resolved_with_paths(repo_root, queue_path.clone(), done_path);

    let err = load_and_validate_queues(&resolved, false)
        .expect_err("expected malformed timestamp to fail validation");
    let err_msg = format!("{:#}", err);
    assert!(
        err_msg.contains("must be a valid RFC3339 UTC timestamp"),
        "unexpected error message: {err_msg}"
    );

    let persisted = std::fs::read_to_string(&queue_path)?;
    assert!(
        persisted.contains("not-a-timestamp"),
        "malformed timestamp should not be rewritten during conservative repair"
    );

    Ok(())
}

#[test]
fn load_queue_with_repair_fixes_malformed_json() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test", "status": "todo", "tags": ["bug",],}]}"#;
    std::fs::write(&queue_path, malformed)?;

    let queue = load_queue_with_repair(&queue_path)?;
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[0].tags, vec!["bug"]);

    Ok(())
}

#[test]
fn load_queue_with_repair_fixes_complex_malformed_json() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let malformed = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',],}]}"#;
    std::fs::write(&queue_path, malformed)?;

    let queue = load_queue_with_repair(&queue_path)?;
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
    assert_eq!(queue.tasks[0].title, "Test task");
    assert_eq!(queue.tasks[0].tags, vec!["bug"]);

    Ok(())
}

#[test]
fn load_queue_accepts_scalar_custom_fields_and_save_normalizes_to_strings() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    std::fs::write(
        &queue_path,
        r#"{"version":1,"tasks":[{"id":"RQ-0001","title":"t","created_at":"2026-01-18T00:00:00Z","updated_at":"2026-01-18T00:00:00Z","custom_fields":{"n":1411,"b":false}}]}"#,
    )?;

    let queue = load_queue(&queue_path)?;
    assert_eq!(
        queue.tasks[0].custom_fields.get("n").map(String::as_str),
        Some("1411")
    );
    assert_eq!(
        queue.tasks[0].custom_fields.get("b").map(String::as_str),
        Some("false")
    );

    save_queue(&queue_path, &queue)?;
    let rendered = std::fs::read_to_string(&queue_path)?;
    assert!(rendered.contains("\"n\": \"1411\""));
    assert!(rendered.contains("\"b\": \"false\""));

    Ok(())
}

#[test]
fn load_queue_malformed_json_returns_error() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": }]}"#;
    std::fs::write(&queue_path, malformed)?;

    let result = load_queue(&queue_path);
    assert!(result.is_err(), "Should error on malformed JSON");
    let err = result.expect_err("malformed JSON should fail");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("parse") || err_msg.contains("JSON"),
        "Error should mention parsing/JSON: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn load_queue_with_repair_fails_on_unrepairable_json() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let unrepairable = r#"{this is not valid json at all"#;
    std::fs::write(&queue_path, unrepairable)?;

    let result = load_queue_with_repair(&queue_path);
    assert!(result.is_err(), "Should error on unrepairable JSON");
    let err = result.expect_err("unrepairable JSON should fail");
    let err_msg = format!("{:#}", err);
    assert!(
        err_msg.contains("parse") || err_msg.contains("JSON") || err_msg.contains("repair"),
        "Error should mention parsing or repair failure: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn load_queue_handles_empty_file() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    std::fs::write(&queue_path, "")?;

    let result = load_queue(&queue_path);
    assert!(result.is_err(), "Should error on empty file");
    let err_msg = format!("{:#}", result.expect_err("empty file should fail"));
    assert!(
        err_msg.contains("EOF") || err_msg.contains("parse") || err_msg.contains("empty"),
        "Error should indicate empty or unparseable file: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn load_queue_detects_truncated_file() -> Result<()> {
    let temp = TempDir::new()?;
    let queue_path = temp.path().join("queue.json");

    let truncated = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test""#;
    std::fs::write(&queue_path, truncated)?;

    let result = load_queue(&queue_path);
    assert!(result.is_err(), "Should error on truncated JSON");
    let err_msg = format!("{:#}", result.expect_err("truncated file should fail"));
    assert!(
        err_msg.contains("EOF") || err_msg.contains("unexpected end") || err_msg.contains("parse"),
        "Error should indicate truncated file or EOF: {}",
        err_msg
    );

    Ok(())
}
