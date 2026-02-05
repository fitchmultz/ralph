//! Integration tests for custom fields functionality.
//!
//! These tests verify that custom fields:
//! - Can be set via CLI and persisted to queue.json
//! - Validate field keys (no whitespace, non-empty)
//! - Display correctly in queue show output

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use tempfile::TempDir;

use ralph::config;
use ralph::contracts::{Config, QueueFile, Task, TaskStatus};
use ralph::queue;
use ralph::timeutil;

fn setup_test_queue() -> Result<(TempDir, config::Resolved)> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();

    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    // Create initial queue with one task
    let queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task for custom fields".to_string(),
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["test evidence".to_string()],
            plan: vec!["test plan".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }],
    };

    let queue_json = serde_json::to_string_pretty(&queue)?;
    std::fs::write(&queue_path, queue_json)?;

    // Create empty done.json
    let done = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let done_json = serde_json::to_string_pretty(&done)?;
    std::fs::write(&done_path, done_json)?;

    // Create config
    let config_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.json");

    let config = Config {
        queue: ralph::contracts::QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
        },
        ..Default::default()
    };

    let config_json = serde_json::to_string_pretty(&config)?;
    std::fs::write(&config_path, config_json)?;

    let resolved = config::Resolved {
        config,
        repo_root: repo_root.to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(config_path),
    };

    Ok((temp_dir, resolved))
}

#[test]
fn test_set_field_persists_to_queue_json() -> Result<()> {
    let (_temp_dir, resolved) = setup_test_queue()?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    queue::set_field(&mut queue_file, "RQ-0001", "severity", "high", &now)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    // Reload and verify
    let reloaded = queue::load_queue(&resolved.queue_path)?;
    assert_eq!(reloaded.tasks.len(), 1);
    assert_eq!(
        reloaded.tasks[0].custom_fields.get("severity"),
        Some(&"high".to_string())
    );

    Ok(())
}

#[test]
fn test_set_field_updates_existing_field() -> Result<()> {
    let (_temp_dir, resolved) = setup_test_queue()?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    // Set initial value
    queue::set_field(&mut queue_file, "RQ-0001", "complexity", "low", &now)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    // Update value
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now2 = timeutil::now_utc_rfc3339()?;
    queue::set_field(&mut queue_file, "RQ-0001", "complexity", "high", &now2)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    // Verify updated value
    let reloaded = queue::load_queue(&resolved.queue_path)?;
    assert_eq!(
        reloaded.tasks[0].custom_fields.get("complexity"),
        Some(&"high".to_string())
    );

    Ok(())
}

#[test]
fn test_set_field_rejects_empty_key() -> Result<()> {
    let (_temp_dir, resolved) = setup_test_queue()?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    let result = queue::set_field(&mut queue_file, "RQ-0001", "", "value", &now);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(err_msg.contains("missing") || err_msg.contains("key"));

    Ok(())
}

#[test]
fn test_set_field_rejects_whitespace_in_key() -> Result<()> {
    let (_temp_dir, resolved) = setup_test_queue()?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    let result = queue::set_field(&mut queue_file, "RQ-0001", "severity level", "high", &now);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(err_msg.contains("whitespace") || err_msg.contains("invalid"));

    Ok(())
}

#[test]
fn test_queue_validate_rejects_empty_custom_field_key() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");

    // Create task with empty custom field key
    let mut custom_fields = HashMap::new();
    custom_fields.insert("".to_string(), "value".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["test".to_string()],
            plan: vec!["test".to_string()],
            notes: vec![],
            request: Some("test".to_string()),
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields,
            parent_id: None,
        }],
    };

    let queue_json = serde_json::to_string_pretty(&queue)?;
    std::fs::write(&queue_path, queue_json)?;

    let result = queue::validate_queue(&queue::load_queue(&queue_path)?, "RQ", 4);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(err_msg.contains("empty") || err_msg.contains("custom field"));

    Ok(())
}

#[test]
fn test_queue_validate_rejects_whitespace_in_custom_field_key() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");

    // Create task with whitespace in custom field key
    let mut custom_fields = HashMap::new();
    custom_fields.insert("severity level".to_string(), "high".to_string());

    let queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["test".to_string()],
            plan: vec!["test".to_string()],
            notes: vec![],
            request: Some("test".to_string()),
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields,
            parent_id: None,
        }],
    };

    let queue_json = serde_json::to_string_pretty(&queue)?;
    std::fs::write(&queue_path, queue_json)?;

    let result = queue::validate_queue(&queue::load_queue(&queue_path)?, "RQ", 4);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(err_msg.contains("whitespace") || err_msg.contains("invalid"));

    Ok(())
}

#[test]
fn test_custom_fields_display_in_queue_show() -> Result<()> {
    let (_temp_dir, resolved) = setup_test_queue()?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    // Set multiple custom fields
    queue::set_field(&mut queue_file, "RQ-0001", "severity", "high", &now)?;
    queue::set_field(&mut queue_file, "RQ-0001", "complexity", "O(n log n)", &now)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    // Reload and verify custom fields are persisted correctly
    let reloaded = queue::load_queue(&resolved.queue_path)?;
    let task = &reloaded.tasks[0];

    // Verify custom fields are stored correctly
    assert_eq!(
        task.custom_fields.get("complexity"),
        Some(&"O(n log n)".to_string())
    );
    assert_eq!(
        task.custom_fields.get("severity"),
        Some(&"high".to_string())
    );

    Ok(())
}

#[test]
fn test_custom_fields_serialization_roundtrip() -> Result<()> {
    let mut custom_fields = HashMap::new();
    custom_fields.insert("severity".to_string(), "high".to_string());
    custom_fields.insert("complexity".to_string(), "O(n)".to_string());

    let task = Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["test".to_string()],
        notes: vec![],
        request: Some("test".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: custom_fields.clone(),
        parent_id: None,
    };

    // Serialize and deserialize
    let json = serde_json::to_string(&task)?;
    let deserialized: Task = serde_json::from_str(&json)?;

    assert_eq!(deserialized.custom_fields, custom_fields);

    Ok(())
}
