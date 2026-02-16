//! Tests for import command.
//!
//! Responsibilities:
//! - Test import command handlers and validation.
//! - Test CSV and JSON import formats.
//! - Test duplicate handling strategies.
//!
//! Not handled here:
//! - Export operations (see export.rs).
//! - List/search operations (see list_search.rs).

use super::resolved_for_dir;
use crate::cli::queue::{
    QueueImportArgs, QueueImportFormat,
    import::{self, OnDuplicate},
};
use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::Result;
use clap::CommandFactory;
use tempfile::TempDir;

fn base_import_args() -> QueueImportArgs {
    QueueImportArgs {
        format: QueueImportFormat::Json,
        input: None,
        dry_run: false,
        on_duplicate: OnDuplicate::Fail,
    }
}

#[test]
fn queue_import_help_includes_examples() {
    use crate::cli::Cli;

    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let import_cmd = queue
        .find_subcommand_mut("import")
        .expect("queue import subcommand");
    let help = import_cmd.render_long_help().to_string();

    assert!(
        help.contains("ralph queue import"),
        "missing import example: {help}"
    );
    assert!(
        help.contains("--dry-run"),
        "missing --dry-run example: {help}"
    );
    assert!(
        help.contains("--on-duplicate"),
        "missing --on-duplicate example: {help}"
    );
}

#[test]
fn queue_import_dry_run_does_not_write() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let initial_task = Task {
        id: "RQ-0001".to_string(),
        title: "Existing task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![initial_task],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let import_json = r#"[{"id": "RQ-9999", "title": "Imported task", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.dry_run = true;

    import::handle(&resolved, true, args)?;

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 1);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");

    Ok(())
}

#[test]
fn queue_import_dry_run_invalid_timestamp_fails() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let initial_task = Task {
        id: "RQ-0001".to_string(),
        title: "Existing task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![initial_task],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let import_json = r#"[{"id": "RQ-9999", "title": "Imported task", "status": "todo", "created_at": "not-a-timestamp", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.dry_run = true;

    let err = import::handle(&resolved, true, args).expect_err("dry-run should validate");
    assert!(
        err.to_string().to_lowercase().contains("timestamp")
            || err.to_string().to_lowercase().contains("created_at"),
        "unexpected error: {err}"
    );

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 1);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");

    Ok(())
}

#[test]
fn queue_import_writes_and_repositions() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let doing_task = Task {
        id: "RQ-0001".to_string(),
        title: "Doing task".to_string(),
        description: None,
        status: TaskStatus::Doing,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![doing_task],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let import_json = r#"[{"id": "", "title": "Task A", "status": "todo"}, {"id": "", "title": "Task B", "status": "todo"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);

    import::handle(&resolved, true, args)?;

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 3);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");
    assert_eq!(queue_after.tasks[0].status, TaskStatus::Doing);

    assert!(queue_after.tasks[1].title == "Task A" || queue_after.tasks[1].title == "Task B");
    assert!(queue_after.tasks[2].title == "Task A" || queue_after.tasks[2].title == "Task B");

    assert!(queue_after.tasks[1].created_at.is_some());
    assert!(queue_after.tasks[1].updated_at.is_some());

    Ok(())
}

#[test]
fn queue_import_duplicate_fail() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let existing_task = Task {
        id: "RQ-0001".to_string(),
        title: "Existing".to_string(),
        description: None,
        status: TaskStatus::Todo,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![existing_task],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let import_json = r#"[{"id": "RQ-0001", "title": "Duplicate", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.on_duplicate = OnDuplicate::Fail;

    let err = import::handle(&resolved, true, args).expect_err("should fail on duplicate");
    assert!(
        err.to_string().contains("Duplicate"),
        "expected duplicate error: {}",
        err
    );

    Ok(())
}

#[test]
fn queue_import_duplicate_skip() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let existing_task = Task {
        id: "RQ-0001".to_string(),
        title: "Existing".to_string(),
        description: None,
        status: TaskStatus::Todo,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![existing_task],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let import_json = r#"[{"id": "RQ-0001", "title": "Duplicate", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}, {"id": "RQ-0002", "title": "New task", "status": "todo"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.on_duplicate = OnDuplicate::Skip;

    import::handle(&resolved, true, args)?;

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 2);

    Ok(())
}

#[test]
fn queue_import_duplicate_rename() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let existing_task = Task {
        id: "RQ-0001".to_string(),
        title: "Existing".to_string(),
        description: None,
        status: TaskStatus::Todo,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        ..Default::default()
    };
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![existing_task],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let import_json = r#"[{"id": "RQ-0001", "title": "Renamed task", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.on_duplicate = OnDuplicate::Rename;

    import::handle(&resolved, true, args)?;

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 2);
    let ids: Vec<String> = queue_after.tasks.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"RQ-0001".to_string()));
    assert!(ids.contains(&"RQ-0002".to_string()));

    Ok(())
}

#[test]
fn queue_import_csv_succeeds() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let csv = concat!(
        "id,title,status,priority,tags,scope,evidence,plan,notes,request,created_at,updated_at,completed_at,depends_on,custom_fields\n",
        "RQ-0003,Dep 3,todo,medium,,,,,,,\n",
        "RQ-0004,Dep 4,todo,medium,,,,,,,\n",
        ",Test CSV task,todo,high,\"tag1,tag2\",\"scope1,scope2\",\"evidence1; evidence2\",\"plan1; plan2\",\"notes1; notes2\",request,2026-01-01T00:00:00Z,2026-01-01T00:00:00Z,,\"RQ-0003,RQ-0004\",\"key=value,foo=bar\""
    );
    let import_path = dir.path().join("import.csv");
    std::fs::write(&import_path, csv)?;

    let mut args = base_import_args();
    args.format = QueueImportFormat::Csv;
    args.input = Some(import_path);

    import::handle(&resolved, true, args)?;

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 3);
    assert!(queue_after.tasks.iter().any(|t| t.id == "RQ-0003"));
    assert!(queue_after.tasks.iter().any(|t| t.id == "RQ-0004"));

    let main = queue_after
        .tasks
        .iter()
        .find(|t| t.title == "Test CSV task")
        .expect("main imported task");
    assert_eq!(main.status, TaskStatus::Todo);
    assert_eq!(main.tags, vec!["tag1", "tag2"]);
    assert_eq!(main.scope, vec!["scope1", "scope2"]);
    assert_eq!(main.evidence, vec!["evidence1", "evidence2"]);
    assert_eq!(main.plan, vec!["plan1", "plan2"]);
    assert_eq!(main.notes, vec!["notes1", "notes2"]);
    assert_eq!(main.depends_on, vec!["RQ-0003", "RQ-0004"]);
    assert_eq!(main.custom_fields.get("key"), Some(&"value".to_string()));
    assert_eq!(main.custom_fields.get("foo"), Some(&"bar".to_string()));

    Ok(())
}

#[test]
fn queue_import_json_wrapper_succeeds() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    let json =
        r#"{"version": 1, "tasks": [{"id": "", "title": "Wrapped task", "status": "todo"}]}"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);

    import::handle(&resolved, true, args)?;

    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 1);
    assert_eq!(queue_after.tasks[0].title, "Wrapped task");

    Ok(())
}
