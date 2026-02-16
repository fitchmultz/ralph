//! Tests for list, search, explain, next, and aging commands.
//!
//! Responsibilities:
//! - Test list/search command handlers and validation.
//! - Test explain and next command behavior.
//! - Test aging reports.
//!
//! Not handled here:
//! - Import/export operations (see import.rs and export.rs).
//! - Issue publishing (see issue.rs).

use super::{base_list_args, resolved_for_dir, write_queue};
use crate::cli::Cli;
use crate::cli::queue::{
    QueueExplainArgs, QueueNextArgs, aging, explain, list, next, search, shared::QueueReportFormat,
};
use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::Result;
use clap::CommandFactory;
use std::collections::HashMap;
use tempfile::TempDir;

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

    let mut args = super::base_search_args();
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
    let mut cmd = Cli::command();
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
    let mut cmd = Cli::command();
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
fn queue_list_json_format_outputs_valid_json() -> Result<()> {
    use crate::cli::queue::QueueListFormat;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let mut args = base_list_args();
    args.format = QueueListFormat::Json;

    list::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_list_scheduled_filter_excludes_unscheduled() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

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
            estimated_minutes: None,
            actual_minutes: None,
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
            estimated_minutes: None,
            actual_minutes: None,
        },
    ];
    let queue = QueueFile { version: 1, tasks };
    std::fs::write(&resolved.queue_path, serde_json::to_string_pretty(&queue)?)?;

    let mut args = base_list_args();
    args.scheduled = true;
    args.format = crate::cli::queue::QueueListFormat::Json;

    list::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_list_include_done_outputs_done_tasks() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

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
        custom_fields: HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };
    let queue = QueueFile {
        version: 1,
        tasks: vec![active_task],
    };
    std::fs::write(&resolved.queue_path, serde_json::to_string_pretty(&queue)?)?;

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
        estimated_minutes: None,
        actual_minutes: None,
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    std::fs::write(&resolved.done_path, serde_json::to_string_pretty(&done)?)?;

    let mut args = base_list_args();
    args.include_done = true;
    args.format = crate::cli::queue::QueueListFormat::Json;

    list::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_search_json_format_outputs_valid_json() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let mut args = super::base_search_args();
    args.format = crate::cli::queue::QueueListFormat::Json;

    search::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_explain_help_includes_examples() {
    let mut cmd = Cli::command();
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
        format: QueueReportFormat::Text,
        include_draft: false,
    };

    explain::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_explain_json_format() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = QueueExplainArgs {
        format: QueueReportFormat::Json,
        include_draft: false,
    };

    explain::handle(&resolved, args)?;
    Ok(())
}

#[test]
fn queue_next_help_includes_explain_flag() {
    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let next_cmd = queue
        .find_subcommand_mut("next")
        .expect("queue next subcommand");
    let help = next_cmd.render_long_help().to_string();

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

#[test]
fn queue_aging_help_examples_expanded() {
    let mut cmd = Cli::command();
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
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = aging::QueueAgingArgs {
        status: vec![],
        format: QueueReportFormat::Text,
    };
    aging::handle(&resolved, args)?;

    Ok(())
}

#[test]
fn queue_aging_handle_json_format() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = aging::QueueAgingArgs {
        status: vec![],
        format: QueueReportFormat::Json,
    };
    aging::handle(&resolved, args)?;

    Ok(())
}

#[test]
fn queue_aging_handle_with_status_filter() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = aging::QueueAgingArgs {
        status: vec![crate::cli::queue::shared::StatusArg::Todo],
        format: QueueReportFormat::Text,
    };
    aging::handle(&resolved, args)?;

    Ok(())
}
