//! Shared test helpers for queue CLI subcommand tests.
//!
//! Responsibilities:
//! - Provide common utilities for creating test environments and mock data.
//! - Define base argument builders for consistent test setup.
//!
//! Not handled here:
//! - Actual test cases (see individual test modules).
//! - Test assertions and specific test logic.
//!
//! Invariants/assumptions:
//! - Tests using these helpers run in isolated temp directories.
//! - Queue files follow the standard QueueFile/Task schema.

use anyhow::Result;
use tempfile::TempDir;

use crate::cli::queue::{
    QueueExportArgs, QueueExportFormat, QueueListArgs, QueueListFormat, QueueSearchArgs,
    QueueSortOrder,
};
use crate::config;
use crate::contracts::{Config, QueueFile, Task, TaskStatus};
use std::collections::HashMap;
use std::path::Path;

/// Create a Resolved config for testing in the given temp directory.
pub fn resolved_for_dir(dir: &TempDir) -> config::Resolved {
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

/// Write a basic test queue file with a single task.
pub fn write_queue(path: &Path) -> Result<()> {
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
        estimated_minutes: None,
        actual_minutes: None,
    };
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let rendered = serde_json::to_string_pretty(&queue)?;
    std::fs::write(path, rendered)?;
    Ok(())
}

/// Create base list arguments for testing.
pub fn base_list_args() -> QueueListArgs {
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

/// Create base search arguments for testing.
pub fn base_search_args() -> QueueSearchArgs {
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

/// Create base export arguments for testing.
pub fn base_export_args() -> QueueExportArgs {
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

mod export;
mod import;
mod issue;
mod list_search;
