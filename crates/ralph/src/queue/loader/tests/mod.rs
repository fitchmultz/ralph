//! Queue loader tests grouped by behavior.
//!
//! Purpose:
//! - Queue loader tests grouped by behavior.
//!
//! Responsibilities:
//! - Provide shared fixtures for queue loader tests.
//! - Split loader coverage by validation behavior, repair behavior, and parsing edge cases.
//!
//! Not handled here:
//! - Production loader logic.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::read::*;
use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn task(id: &str) -> Task {
    Task {
        id: id.to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        description: None,
        priority: Default::default(),
        tags: vec!["code".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
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
    }
}

fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(())
}

fn resolved_with_paths(repo_root: &Path, queue_path: PathBuf, done_path: PathBuf) -> Resolved {
    Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

mod parse;
mod repair;
mod validate;
