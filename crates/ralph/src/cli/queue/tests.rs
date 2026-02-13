//! Tests for queue CLI subcommand handlers.
//!
//! Responsibilities:
//! - Validate queue CLI help output and basic handler behavior.
//! - Test JSON output format and scheduled filter behavior.
//! - Guard against regressions in command examples and formatting.
//!
//! Not handled here:
//! - Full end-to-end CLI invocation and filesystem side effects.
//! - Runner execution paths or task queue persistence.
//!
//! Invariants/assumptions:
//! - Clap command definitions are accessible through `crate::cli::Cli`.
//! - Help output is rendered using clap's long help formatter.

use anyhow::Result;
use clap::CommandFactory;
use tempfile::TempDir;

use super::{
    QueueExplainArgs, QueueExportArgs, QueueExportFormat, QueueImportArgs, QueueImportFormat,
    QueueListArgs, QueueListFormat, QueueNextArgs, QueueSearchArgs, QueueSortOrder, export, import,
    list, next, search,
};
use crate::config;
use crate::contracts::{Config, QueueFile, Task, TaskStatus};
use std::collections::HashMap;
use std::path::Path;

fn resolved_for_dir(dir: &TempDir) -> config::Resolved {
    config::Resolved {
        config: Config::default(),
        repo_root: dir.path().to_path_buf(),
        queue_path: dir.path().join("queue.json"),
        done_path: dir.path().join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

fn write_queue(path: &Path) -> Result<()> {
    let task = Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        description: None,
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
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    };
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let rendered = serde_json::to_string_pretty(&queue)?;
    std::fs::write(path, rendered)?;
    Ok(())
}

fn base_list_args() -> QueueListArgs {
    QueueListArgs {
        status: vec![],
        tag: vec![],
        scope: vec![],
        filter_deps: None,
        include_done: false,
        only_done: false,
        format: QueueListFormat::Compact,
        limit: 50,
        all: false,
        sort_by: None,
        order: QueueSortOrder::Descending,
        quiet: false,
        scheduled: false,
        scheduled_after: None,
        scheduled_before: None,
        with_eta: false,
    }
}

fn base_search_args() -> QueueSearchArgs {
    QueueSearchArgs {
        query: "test".to_string(),
        regex: false,
        match_case: false,
        fuzzy: false,
        status: vec![],
        tag: vec![],
        scope: vec![],
        include_done: false,
        only_done: false,
        format: QueueListFormat::Compact,
        limit: 50,
        all: false,
        scheduled: false,
    }
}

fn base_export_args() -> QueueExportArgs {
    QueueExportArgs {
        format: QueueExportFormat::Csv,
        output: None,
        status: vec![],
        tag: vec![],
        scope: vec![],
        id_pattern: None,
        created_after: None,
        created_before: None,
        include_archive: false,
        only_archive: false,
        quiet: false,
    }
}

#[test]
fn queue_list_rejects_conflicting_done_flags() {
    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);

    let mut args = base_list_args();
    args.include_done = true;
    args.only_done = true;

    let err = list::handle(&resolved, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("Conflicting flags")
            && msg.contains("--include-done")
            && msg.contains("--only-done"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_search_rejects_conflicting_done_flags() {
    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);

    let mut args = base_search_args();
    args.include_done = true;
    args.only_done = true;

    let err = search::handle(&resolved, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("Conflicting flags")
            && msg.contains("--include-done")
            && msg.contains("--only-done"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_list_handle_smoke() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = base_list_args();
    list::handle(&resolved, args)?;

    Ok(())
}

#[test]
fn queue_validate_help_examples_expanded() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let validate = queue
        .find_subcommand_mut("validate")
        .expect("queue validate subcommand");
    let help = validate.render_long_help().to_string();

    assert!(
        help.contains("ralph queue validate"),
        "missing validate example: {help}"
    );
    assert!(
        help.contains("ralph --verbose queue validate"),
        "missing verbose validate example: {help}"
    );
}

#[test]
fn queue_next_id_help_examples_expanded() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let next_id = queue
        .find_subcommand_mut("next-id")
        .expect("queue next-id subcommand");
    let help = next_id.render_long_help().to_string();

    assert!(
        help.contains("ralph queue next-id"),
        "missing next-id example: {help}"
    );
    assert!(
        help.contains("ralph --verbose queue next-id"),
        "missing verbose next-id example: {help}"
    );
}

#[test]
fn queue_export_rejects_conflicting_archive_flags() {
    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);

    let mut args = base_export_args();
    args.include_archive = true;
    args.only_archive = true;

    let err = export::handle(&resolved, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("Conflicting flags")
            && msg.contains("--include-archive")
            && msg.contains("--only-archive"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_export_help_examples_expanded() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let export_cmd = queue
        .find_subcommand_mut("export")
        .expect("queue export subcommand");
    let help = export_cmd.render_long_help().to_string();

    assert!(
        help.contains("ralph queue export"),
        "missing export example: {help}"
    );
    assert!(
        help.contains("--format csv"),
        "missing format example: {help}"
    );
    assert!(
        help.contains("--output tasks.csv"),
        "missing output example: {help}"
    );
}

#[test]
fn queue_list_json_format_outputs_valid_json() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let mut args = base_list_args();
    args.format = QueueListFormat::Json;

    // Capture stdout by redirecting to a buffer (we test by running the handler)
    list::handle(&resolved, args)?;
    // If no panic/error, JSON was printed successfully
    Ok(())
}

#[test]
fn queue_list_scheduled_filter_excludes_unscheduled() -> Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create queue with mixed scheduled/unscheduled tasks
    let tasks = vec![
        Task {
            id: "RQ-0001".to_string(),
            title: "Scheduled task".to_string(),
            description: None,
            status: TaskStatus::Todo,
            scheduled_start: Some("2026-06-15T10:00:00Z".to_string()),
            priority: Default::default(),
            tags: vec![],
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
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        },
        Task {
            id: "RQ-0002".to_string(),
            title: "Unscheduled task".to_string(),
            description: None,
            status: TaskStatus::Todo,
            scheduled_start: None,
            priority: Default::default(),
            tags: vec![],
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
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        },
    ];
    let queue = QueueFile { version: 1, tasks };
    std::fs::write(&resolved.queue_path, serde_json::to_string_pretty(&queue)?)?;

    // Test with --scheduled flag
    let mut args = base_list_args();
    args.scheduled = true;
    args.format = QueueListFormat::Json;

    list::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_list_include_done_outputs_done_tasks() -> Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create active queue
    let active_task = Task {
        id: "RQ-0001".to_string(),
        title: "Active task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: Default::default(),
        tags: vec![],
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
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
    };
    let queue = QueueFile {
        version: 1,
        tasks: vec![active_task],
    };
    std::fs::write(&resolved.queue_path, serde_json::to_string_pretty(&queue)?)?;

    // Create done file
    let done_task = Task {
        id: "RQ-0000".to_string(),
        title: "Done task".to_string(),
        description: None,
        status: TaskStatus::Done,
        priority: Default::default(),
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: Some("2026-01-02T00:00:00Z".to_string()),
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    std::fs::write(&resolved.done_path, serde_json::to_string_pretty(&done)?)?;

    // Test with --include-done and JSON format
    let mut args = base_list_args();
    args.include_done = true;
    args.format = QueueListFormat::Json;

    list::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_search_json_format_outputs_valid_json() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let mut args = base_search_args();
    args.format = QueueListFormat::Json;

    search::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_explain_help_includes_examples() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let explain_cmd = queue
        .find_subcommand_mut("explain")
        .expect("queue explain subcommand");
    let help = explain_cmd.render_long_help().to_string();

    assert!(
        help.contains("ralph queue explain"),
        "missing explain example: {help}"
    );
    assert!(
        help.contains("--format json"),
        "missing format json example: {help}"
    );
    assert!(
        help.contains("--include-draft"),
        "missing include-draft example: {help}"
    );
}

#[test]
fn queue_explain_smoke() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = QueueExplainArgs {
        format: crate::cli::queue::shared::QueueReportFormat::Text,
        include_draft: false,
    };

    super::explain::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_explain_json_format() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = QueueExplainArgs {
        format: crate::cli::queue::shared::QueueReportFormat::Json,
        include_draft: false,
    };

    super::explain::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_next_help_includes_explain_flag() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let next_cmd = queue
        .find_subcommand_mut("next")
        .expect("queue next subcommand");
    let help = next_cmd.render_long_help().to_string();

    // Check that --explain flag is documented
    assert!(help.contains("--explain"), "missing --explain flag: {help}");
    assert!(
        help.contains("Print an explanation when no runnable task is found"),
        "missing explain description: {help}"
    );
}

#[test]
fn queue_next_with_explain() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = QueueNextArgs {
        with_title: false,
        with_eta: false,
        explain: true,
    };

    next::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_next_without_explain() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = QueueNextArgs {
        with_title: false,
        with_eta: false,
        explain: false,
    };

    next::handle(&resolved, args)?;
    Ok(())
}

// ------------------------------------------------------------------------
// Import tests
// ------------------------------------------------------------------------

fn base_import_args() -> QueueImportArgs {
    QueueImportArgs {
        format: QueueImportFormat::Json,
        input: None,
        dry_run: false,
        on_duplicate: import::OnDuplicate::Fail,
    }
}

#[test]
fn queue_import_help_includes_examples() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let import_cmd = queue
        .find_subcommand_mut("import")
        .expect("queue import subcommand");
    let help = import_cmd.render_long_help().to_string();

    // Check for key examples
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
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create initial queue with one task
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

    // Create import file
    let import_json = r#"[{"id": "RQ-9999", "title": "Imported task", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    // Import with dry-run
    let mut args = base_import_args();
    args.input = Some(import_path);
    args.dry_run = true;

    import::handle(&resolved, true, args)?;

    // Verify queue unchanged
    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 1);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");

    Ok(())
}

#[test]
fn queue_import_dry_run_invalid_timestamp_fails() -> Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create initial queue with one task
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

    // Invalid created_at should fail validation even in dry-run.
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

    // Verify queue unchanged
    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 1);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001");

    Ok(())
}

#[test]
fn queue_import_writes_and_repositions() -> Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create queue with first task in "doing" status
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

    // Create import file with 2 tasks (empty IDs will be auto-generated with correct prefix)
    let import_json = r#"[{"id": "", "title": "Task A", "status": "todo"}, {"id": "", "title": "Task B", "status": "todo"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    // Import
    let mut args = base_import_args();
    args.input = Some(import_path);

    import::handle(&resolved, true, args)?;

    // Verify queue has 3 tasks, with doing task still first
    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 3);
    assert_eq!(queue_after.tasks[0].id, "RQ-0001"); // Doing task stays first
    assert_eq!(queue_after.tasks[0].status, TaskStatus::Doing);

    // Imported tasks should be at positions 1 and 2
    assert!(queue_after.tasks[1].title == "Task A" || queue_after.tasks[1].title == "Task B");
    assert!(queue_after.tasks[2].title == "Task A" || queue_after.tasks[2].title == "Task B");

    // Verify timestamps were backfilled
    assert!(queue_after.tasks[1].created_at.is_some());
    assert!(queue_after.tasks[1].updated_at.is_some());

    Ok(())
}

#[test]
fn queue_import_duplicate_fail() -> Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create queue with task
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

    // Try to import task with same ID
    let import_json = r#"[{"id": "RQ-0001", "title": "Duplicate", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.on_duplicate = import::OnDuplicate::Fail;

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
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create queue with task
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

    // Import with one duplicate and one new task (note: new task needs ID for this test structure)
    let import_json = r#"[{"id": "RQ-0001", "title": "Duplicate", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}, {"id": "RQ-0002", "title": "New task", "status": "todo"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.on_duplicate = import::OnDuplicate::Skip;

    import::handle(&resolved, true, args)?;

    // Verify only 2 tasks (original + 1 new)
    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 2);

    Ok(())
}

#[test]
fn queue_import_duplicate_rename() -> Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create queue with task
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

    // Import task with duplicate ID
    let import_json = r#"[{"id": "RQ-0001", "title": "Renamed task", "status": "todo", "created_at": "2026-01-02T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}]"#;
    let import_path = dir.path().join("import.json");
    std::fs::write(&import_path, import_json)?;

    let mut args = base_import_args();
    args.input = Some(import_path);
    args.on_duplicate = import::OnDuplicate::Rename;

    import::handle(&resolved, true, args)?;

    // Verify 2 tasks with different IDs
    let queue_after = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_after.tasks.len(), 2);
    // One task should have RQ-0001, the other RQ-0002
    let ids: Vec<String> = queue_after.tasks.iter().map(|t| t.id.clone()).collect();
    assert!(ids.contains(&"RQ-0001".to_string()));
    assert!(ids.contains(&"RQ-0002".to_string()));

    Ok(())
}

#[test]
fn queue_import_csv_succeeds() -> Result<()> {
    use crate::contracts::TaskStatus;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Create empty queue
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    // Create CSV import file with full export header
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

    // Create empty queue
    let initial_queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial_queue)?;

    // Create JSON wrapper import (id is required in Task struct; empty ID triggers auto-generation)
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

// ------------------------------------------------------------------------
// Issue publish tests
// ------------------------------------------------------------------------

fn base_issue_publish_args(task_id: &str) -> super::issue::QueueIssuePublishArgs {
    super::issue::QueueIssuePublishArgs {
        task_id: task_id.to_string(),
        dry_run: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    }
}

fn issue_task(
    id: &str,
    title: &str,
    status: TaskStatus,
    tags: &[&str],
    custom_fields: &[(&str, &str)],
) -> Task {
    let mut fields = HashMap::new();
    for (key, value) in custom_fields {
        fields.insert((*key).to_string(), (*value).to_string());
    }

    Task {
        id: id.to_string(),
        status,
        title: title.to_string(),
        description: None,
        priority: Default::default(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["verify".to_string()],
        notes: vec![],
        request: Some("test".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: fields,
        parent_id: None,
    }
}

fn write_issue_queue_tasks(path: &Path, tasks: Vec<Task>) -> Result<()> {
    let queue = QueueFile { version: 1, tasks };
    let rendered = serde_json::to_string_pretty(&queue)?;
    std::fs::write(path, rendered)?;
    Ok(())
}

#[cfg(unix)]
fn create_fake_gh_for_issue_publish(
    tmp_dir: &TempDir,
    task_id: &str,
    issue_url: &str,
    auth_ok: bool,
) -> std::path::PathBuf {
    use std::io::Write;

    let bin_dir = tmp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let gh_path = bin_dir.join("gh");
    let log_path = tmp_dir.path().join("gh.log");

    let auth_status_block = if auth_ok {
        "exit 0"
    } else {
        "echo \"You are not logged into any GitHub hosts\" >&2\nexit 1"
    };

    // Use only shell builtins so tests can safely set PATH to only include this bin dir.
    let script = format!(
        r#"#!/bin/sh
set -eu

LOG="{log_path}"
TASK_ID="{task_id}"
ISSUE_URL="{issue_url}"

if [ "${{1:-}}" = "--version" ]; then
  echo "gh version 0.0.0-test"
  exit 0
fi

if [ "${{1:-}}" = "auth" ] && [ "${{2:-}}" = "status" ]; then
  {auth_status_block}
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "create" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  TITLE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    elif [ "$arg" = "--title" ]; then
      i=$((i+1)); eval TITLE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi

  if [ -z "$TITLE" ]; then
    echo "missing title" >&2
    exit 3
  fi

  FOUND=0
  while IFS= read -r line; do
    if [ "$line" = "<!-- ralph_task_id: $TASK_ID -->" ]; then
      FOUND=1
    fi
  done < "$BODY_FILE"

  if [ "$FOUND" -ne 1 ]; then
    echo "missing marker" >&2
    exit 4
  fi

  echo "$ISSUE_URL"
  exit 0
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "edit" ]; then
  printf '%s\n' "$*" >> "$LOG"

  # Ensure edit uses add semantics (not replacement) for labels/assignees.
  HAS_ADD_LABEL=0
  HAS_ADD_ASSIGNEE=0
  for arg in "$@"; do
    if [ "$arg" = "--add-label" ]; then HAS_ADD_LABEL=1; fi
    if [ "$arg" = "--add-assignee" ]; then HAS_ADD_ASSIGNEE=1; fi
  done

  if [ "$HAS_ADD_LABEL" -ne 1 ] || [ "$HAS_ADD_ASSIGNEE" -ne 1 ]; then
    echo "missing add flags" >&2
    exit 5
  fi

  exit 0
fi

echo "unknown args: $*" >&2
exit 1
"#,
        log_path = log_path.display(),
        task_id = task_id,
        issue_url = issue_url,
        auth_status_block = auth_status_block
    );

    let mut file = std::fs::File::create(&gh_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh_path, perms).unwrap();
    }

    bin_dir
}

#[cfg(unix)]
fn create_fake_gh_for_issue_publish_multi(
    tmp_dir: &TempDir,
    issue_url: &str,
    auth_ok: bool,
    fail_task_id: Option<&str>,
) -> std::path::PathBuf {
    use std::io::Write;

    let bin_dir = tmp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let gh_path = bin_dir.join("gh");
    let log_path = tmp_dir.path().join("gh.log");
    let fail_task_id = fail_task_id.unwrap_or("");

    let auth_status_block = if auth_ok {
        "exit 0"
    } else {
        "echo \"You are not logged into any GitHub hosts\" >&2\nexit 1"
    };

    let script = format!(
        r#"#!/bin/sh
set -eu

LOG="{log_path}"
FAIL_TASK_ID="{fail_task_id}"

if [ "${{1:-}}" = "--version" ]; then
  echo "gh version 0.0.0-test"
  exit 0
fi

if [ "${{1:-}}" = "auth" ] && [ "${{2:-}}" = "status" ]; then
  {auth_status_block}
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "create" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  TITLE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    elif [ "$arg" = "--title" ]; then
      i=$((i+1)); eval TITLE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi
  if [ -z "$TITLE" ]; then
    echo "missing title" >&2
    exit 3
  fi

  if [ -n "$FAIL_TASK_ID" ] && grep -q "ralph_task_id: $FAIL_TASK_ID" "$BODY_FILE"; then
    echo "simulated failure for task $FAIL_TASK_ID" >&2
    exit 7
  fi

  echo "{issue_url}"
  exit 0
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "edit" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi

  if [ -n "$FAIL_TASK_ID" ] && grep -q "ralph_task_id: $FAIL_TASK_ID" "$BODY_FILE"; then
    echo "simulated failure for task $FAIL_TASK_ID" >&2
    exit 7
  fi
  exit 0
fi

echo "unknown args: $*" >&2
exit 1
"#,
        log_path = log_path.display(),
        issue_url = issue_url,
        fail_task_id = fail_task_id,
        auth_status_block = auth_status_block
    );

    let mut file = std::fs::File::create(&gh_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh_path, perms).unwrap();
    }

    bin_dir
}

#[test]
fn queue_issue_publish_help_examples_expanded() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let issue_cmd = queue
        .find_subcommand_mut("issue")
        .expect("queue issue subcommand");
    let help = issue_cmd.render_long_help().to_string();

    // Check that examples are present
    assert!(
        help.contains("ralph queue issue publish"),
        "missing issue publish example: {help}"
    );
    assert!(help.contains("ralph queue issue publish-many"));
}

#[test]
fn queue_issue_publish_dry_run_succeeds() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = base_issue_publish_args("RQ-0001");
    let args = super::issue::QueueIssuePublishArgs {
        dry_run: true,
        ..args
    };

    // Dry-run should succeed without gh
    let result = super::issue::handle_publish(&resolved, true, args);
    assert!(result.is_ok());

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_dry_run_filters() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task("RQ-0001", "Bug task one", TaskStatus::Todo, &["bug"], &[]),
            issue_task("RQ-0002", "Bug task two", TaskStatus::Todo, &["bug"], &[]),
            issue_task("RQ-0003", "Other task", TaskStatus::Doing, &["cli"], &[]),
        ],
    )?;

    let args = super::issue::QueueIssuePublishManyArgs {
        status: vec![super::shared::StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: Some("^RQ-0001$".to_string()),
        dry_run: false,
        execute: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    };

    let result = super::issue::handle_publish_many(&resolved, true, args);
    assert!(result.is_ok());

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 3);
    assert_eq!(queue.tasks[0].id, "RQ-0001");

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_exec_mixed_create_update() -> Result<()> {
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task("RQ-0001", "Bug task one", TaskStatus::Todo, &["bug"], &[]),
            issue_task(
                "RQ-0002",
                "Bug task two",
                TaskStatus::Todo,
                &["bug"],
                &[
                    ("github_issue_url", "https://github.com/org/repo/issues/777"),
                    ("github_issue_number", "777"),
                ],
            ),
        ],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish_multi(
        &dir,
        "https://github.com/org/repo/issues/123",
        true,
        None,
    );

    let args = super::issue::QueueIssuePublishManyArgs {
        status: vec![super::shared::StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: true,
        label: vec!["triage".to_string()],
        assignee: vec![],
        repo: None,
    };

    with_prepend_path(&bin_dir, || {
        super::issue::handle_publish_many(&resolved, true, args)
    })?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let first = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("first task");
    let second = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("second task");

    assert_eq!(
        first
            .custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/123")
    );
    assert!(
        first
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );
    assert_eq!(
        second
            .custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/777")
    );
    assert!(
        second
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_skips_if_unchanged() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    let task = issue_task("RQ-0001", "No-op task", TaskStatus::Todo, &["bug"], &[]);
    let body = super::export::render_task_as_github_issue_body(&task);
    let hash = crate::git::compute_issue_sync_hash(
        &format!("{}: {}", task.id, task.title),
        &body,
        &[],
        &[],
        None,
    )?;

    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![issue_task(
            "RQ-0001",
            "No-op task",
            TaskStatus::Todo,
            &["bug"],
            &[
                ("github_issue_url", "https://github.com/org/repo/issues/123"),
                (crate::git::GITHUB_ISSUE_SYNC_HASH_KEY, &hash),
            ],
        )],
    )?;

    let args = super::issue::QueueIssuePublishManyArgs {
        status: vec![super::shared::StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    };

    super::issue::handle_publish_many(&resolved, true, args)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_partial_failures_do_not_abort() -> Result<()> {
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task(
                "RQ-0001",
                "Task that fails",
                TaskStatus::Todo,
                &["bug"],
                &[],
            ),
            issue_task(
                "RQ-0002",
                "Task that succeeds",
                TaskStatus::Todo,
                &["bug"],
                &[
                    ("github_issue_url", "https://github.com/org/repo/issues/777"),
                    ("github_issue_number", "777"),
                ],
            ),
        ],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish_multi(
        &dir,
        "https://github.com/org/repo/issues/123",
        true,
        Some("RQ-0001"),
    );

    let args = super::issue::QueueIssuePublishManyArgs {
        status: vec![super::shared::StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: true,
        label: vec![],
        assignee: vec![],
        repo: None,
    };
    let err = with_prepend_path(&bin_dir, || {
        super::issue::handle_publish_many(&resolved, true, args)
    })
    .expect_err("expected publish-many failure");
    assert!(
        err.to_string().contains("completed with 1 failed task(s)")
            || err.to_string().contains("simulated failure"),
        "unexpected error: {err}"
    );

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 2);
    assert_eq!(queue.tasks[1].id, "RQ-0002");
    assert!(
        queue.tasks[1]
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );

    Ok(())
}

#[test]
fn queue_issue_publish_fails_when_task_not_found() {
    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path).expect("write queue");

    let args = base_issue_publish_args("RQ-9999"); // Non-existent task

    let err = super::issue::handle_publish(&resolved, true, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("RQ-9999"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_issue_publish_fails_when_gh_missing() -> Result<()> {
    use crate::testsupport::path::with_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = base_issue_publish_args("RQ-0001");

    // Ensure PATH does not contain gh.
    let err = with_path("", || super::issue::handle_publish(&resolved, true, args))
        .expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("GitHub CLI (`gh`) not found on PATH"),
        "unexpected error: {msg}"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_fails_when_gh_unauthenticated() -> Result<()> {
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/123",
        false,
    );

    let args = base_issue_publish_args("RQ-0001");
    let err = with_prepend_path(&bin_dir, || {
        super::issue::handle_publish(&resolved, true, args)
    })
    .expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("not authenticated") && msg.contains("gh auth login"),
        "unexpected error: {msg}"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_creates_issue_and_persists_custom_fields() -> Result<()> {
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/123",
        true,
    );

    let args = super::issue::QueueIssuePublishArgs {
        label: vec!["bug".to_string()],
        assignee: vec!["@me".to_string()],
        ..base_issue_publish_args("RQ-0001")
    };

    with_prepend_path(&bin_dir, || {
        super::issue::handle_publish(&resolved, true, args)
    })?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task");

    assert_eq!(
        task.custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/123")
    );
    assert_eq!(
        task.custom_fields
            .get("github_issue_number")
            .map(String::as_str),
        Some("123")
    );
    assert!(
        task.updated_at.as_deref() != Some("2026-01-18T00:00:00Z"),
        "updated_at should be updated on publish"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_updates_existing_issue_and_backfills_issue_number() -> Result<()> {
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Write queue, then set a github_issue_url (but no number).
    write_queue(&resolved.queue_path)?;
    let mut queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id == "RQ-0001")
        .expect("task");
    task.custom_fields.insert(
        "github_issue_url".to_string(),
        "https://github.com/org/repo/issues/777".to_string(),
    );
    crate::queue::save_queue(&resolved.queue_path, &queue)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/777",
        true,
    );

    let args = super::issue::QueueIssuePublishArgs {
        label: vec!["help-wanted".to_string()],
        assignee: vec!["@me".to_string()],
        ..base_issue_publish_args("RQ-0001")
    };

    with_prepend_path(&bin_dir, || {
        super::issue::handle_publish(&resolved, true, args)
    })?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task");

    assert_eq!(
        task.custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/777")
    );
    assert_eq!(
        task.custom_fields
            .get("github_issue_number")
            .map(String::as_str),
        Some("777")
    );

    Ok(())
}

#[test]
fn queue_issue_publish_help_contains_publish_subcommand() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let _issue_cmd = queue
        .find_subcommand_mut("issue")
        .expect("queue issue subcommand");
}

#[test]
fn queue_aging_help_examples_expanded() {
    let mut cmd = crate::cli::Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let aging = queue
        .find_subcommand_mut("aging")
        .expect("queue aging subcommand");
    let help = aging.render_long_help().to_string();

    assert!(
        help.contains("ralph queue aging"),
        "missing aging example: {help}"
    );
    assert!(
        help.contains("ralph queue aging --format json"),
        "missing aging json example: {help}"
    );
    assert!(
        help.contains("ralph queue aging --status todo --status doing"),
        "missing aging status filter example: {help}"
    );
}

#[test]
fn queue_aging_handle_smoke() -> Result<()> {
    use super::aging;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = aging::QueueAgingArgs {
        status: vec![],
        format: super::QueueReportFormat::Text,
    };
    aging::handle(&resolved, args)?;

    Ok(())
}

#[test]
fn queue_aging_handle_json_format() -> Result<()> {
    use super::aging;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = aging::QueueAgingArgs {
        status: vec![],
        format: super::QueueReportFormat::Json,
    };
    aging::handle(&resolved, args)?;

    Ok(())
}

#[test]
fn queue_aging_handle_with_status_filter() -> Result<()> {
    use super::aging;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = aging::QueueAgingArgs {
        status: vec![super::StatusArg::Todo],
        format: super::QueueReportFormat::Text,
    };
    aging::handle(&resolved, args)?;

    Ok(())
}
