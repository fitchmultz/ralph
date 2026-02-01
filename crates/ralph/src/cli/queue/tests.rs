//! Tests for queue CLI subcommand handlers.
//!
//! Responsibilities:
//! - Validate queue CLI help output and basic handler behavior.
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
    QueueExportArgs, QueueExportFormat, QueueListArgs, QueueListFormat, QueueSearchArgs,
    QueueSortOrder, export, list, search,
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
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
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
