//! File creation utilities for Ralph initialization.
//!
//! Purpose:
//! - File creation utilities for Ralph initialization.
//!
//! Responsibilities:
//! - Create and write queue.jsonc, done.jsonc, and config.jsonc files.
//! - Validate existing files when not forcing overwrite.
//! - Integrate wizard answers for initial task and config values.
//!
//! Not handled here:
//! - README file creation (see `super::readme`).
//! - Interactive user input (see `super::wizard`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Parent directories are created as needed.
//! - Existing files are validated before being considered "Valid".
//! - Atomic writes are used for all file operations.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use crate::queue;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::FileInitStatus;
use super::wizard::WizardAnswers;

/// Write queue file, optionally including a first task from wizard answers.
pub fn write_queue(
    path: &Path,
    force: bool,
    id_prefix: &str,
    id_width: usize,
    wizard_answers: Option<&WizardAnswers>,
) -> Result<FileInitStatus> {
    if path.exists() && !force {
        // Validate existing file by trying to load it
        let queue = queue::load_queue(path)?;
        queue::validate_queue(&queue, id_prefix, id_width)
            .with_context(|| format!("validate existing queue {}", path.display()))?;
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let mut queue = QueueFile::default();

    // Add first task if wizard provided one
    if let Some(answers) = wizard_answers
        && answers.create_first_task
        && let (Some(title), Some(description)) = (
            answers.first_task_title.clone(),
            answers.first_task_description.clone(),
        )
    {
        let now = time::OffsetDateTime::now_utc();
        let timestamp = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| now.to_string());

        let task_id = format!("{}-{:0>width$}", id_prefix, 1, width = id_width);

        let task = Task {
            id: task_id,
            status: TaskStatus::Todo,
            title,
            description: None,
            priority: answers.first_task_priority,
            tags: vec!["onboarding".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: Some(description),
            agent: None,
            created_at: Some(timestamp.clone()),
            updated_at: Some(timestamp),
            completed_at: None,
            started_at: None,
            estimated_minutes: None,
            actual_minutes: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
        };

        queue.tasks.push(task);
    }

    let rendered = serde_json::to_string_pretty(&queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

/// Write done file (archive for completed tasks).
pub fn write_done(
    path: &Path,
    force: bool,
    id_prefix: &str,
    id_width: usize,
) -> Result<FileInitStatus> {
    if path.exists() && !force {
        // Validate existing file by trying to load it
        let queue = queue::load_queue(path)?;
        queue::validate_queue(&queue, id_prefix, id_width)
            .with_context(|| format!("validate existing done {}", path.display()))?;
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let queue = QueueFile::default();
    let rendered = serde_json::to_string_pretty(&queue).context("serialize done JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write done JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

/// Write config file, integrating wizard answers if provided.
pub fn write_config(
    path: &Path,
    force: bool,
    wizard_answers: Option<&WizardAnswers>,
) -> Result<FileInitStatus> {
    if path.exists() && !force {
        // Validate existing config using load_layer to support JSONC with comments
        crate::config::load_layer(path).with_context(|| {
            format!(
                "Config file exists but is invalid JSON/JSONC: {}. Use --force to overwrite.",
                path.display()
            )
        })?;
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    // Build config with wizard answers or defaults
    let config_json = if let Some(answers) = wizard_answers {
        let runner_str = format!("{:?}", answers.runner).to_lowercase();
        let model_str = if answers.model.contains("/") || answers.model.len() > 20 {
            // Custom model string
            answers.model.clone()
        } else {
            answers.model.clone()
        };

        serde_json::json!({
            "version": 2,
            "agent": {
                "runner": runner_str,
                "model": model_str,
                "phases": answers.phases
            }
        })
    } else {
        serde_json::json!({ "version": 2 })
    };

    let rendered = serde_json::to_string_pretty(&config_json).context("serialize config JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write config JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

#[cfg(test)]
mod tests;
