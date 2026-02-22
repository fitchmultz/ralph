//! File creation utilities for Ralph initialization.
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
            "version": 1,
            "agent": {
                "runner": runner_str,
                "model": model_str,
                "phases": answers.phases
            }
        })
    } else {
        serde_json::json!({ "version": 1 })
    };

    let rendered = serde_json::to_string_pretty(&config_json).context("serialize config JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write config JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::contracts::{Config, Runner, TaskPriority};
    use tempfile::TempDir;

    fn resolved_for(dir: &TempDir) -> config::Resolved {
        let repo_root = dir.path().to_path_buf();
        let queue_path = repo_root.join(".ralph/queue.jsonc");
        let done_path = repo_root.join(".ralph/done.jsonc");
        let project_config_path = Some(repo_root.join(".ralph/config.jsonc"));
        config::Resolved {
            config: Config::default(),
            repo_root,
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path,
        }
    }

    #[test]
    fn init_creates_missing_files() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        let queue_status = write_queue(
            &resolved.queue_path,
            false,
            &resolved.id_prefix,
            resolved.id_width,
            None,
        )?;
        let done_status = write_done(
            &resolved.done_path,
            false,
            &resolved.id_prefix,
            resolved.id_width,
        )?;
        let config_status =
            write_config(resolved.project_config_path.as_ref().unwrap(), false, None)?;

        assert_eq!(queue_status, FileInitStatus::Created);
        assert_eq!(done_status, FileInitStatus::Created);
        assert_eq!(config_status, FileInitStatus::Created);

        let queue = crate::queue::load_queue(&resolved.queue_path)?;
        assert_eq!(queue.version, 1);
        let done = crate::queue::load_queue(&resolved.done_path)?;
        assert_eq!(done.version, 1);
        let raw_cfg = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&raw_cfg)?;
        assert_eq!(cfg.version, 1);

        Ok(())
    }

    #[test]
    fn init_skips_existing_when_not_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        let queue_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Keep",
      "tags": ["code"],
      "scope": ["x"],
      "evidence": ["y"],
      "plan": ["z"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.queue_path, queue_json)?;
        let done_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0002",
      "status": "done",
      "title": "Done",
      "tags": ["code"],
      "scope": ["x"],
      "evidence": ["y"],
      "plan": ["z"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z",
      "completed_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.done_path, done_json)?;
        let config_json = r#"{
  "version": 1,
  "queue": {
    "file": ".ralph/queue.json"
  }
}"#;
        std::fs::write(resolved.project_config_path.as_ref().unwrap(), config_json)?;

        let queue_status = write_queue(
            &resolved.queue_path,
            false,
            &resolved.id_prefix,
            resolved.id_width,
            None,
        )?;
        let done_status = write_done(
            &resolved.done_path,
            false,
            &resolved.id_prefix,
            resolved.id_width,
        )?;
        let config_status =
            write_config(resolved.project_config_path.as_ref().unwrap(), false, None)?;

        assert_eq!(queue_status, FileInitStatus::Valid);
        assert_eq!(done_status, FileInitStatus::Valid);
        assert_eq!(config_status, FileInitStatus::Valid);

        let raw = std::fs::read_to_string(&resolved.queue_path)?;
        assert!(raw.contains("Keep"));
        let done_raw = std::fs::read_to_string(&resolved.done_path)?;
        assert!(done_raw.contains("Done"));

        Ok(())
    }

    #[test]
    fn init_overwrites_when_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"docs"}"#,
        )?;

        let queue_status = write_queue(
            &resolved.queue_path,
            true,
            &resolved.id_prefix,
            resolved.id_width,
            None,
        )?;
        let done_status = write_done(
            &resolved.done_path,
            true,
            &resolved.id_prefix,
            resolved.id_width,
        )?;
        let config_status =
            write_config(resolved.project_config_path.as_ref().unwrap(), true, None)?;

        assert_eq!(queue_status, FileInitStatus::Created);
        assert_eq!(done_status, FileInitStatus::Created);
        assert_eq!(config_status, FileInitStatus::Created);

        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&cfg_raw)?;
        assert_eq!(cfg.project_type, Some(crate::contracts::ProjectType::Code));

        Ok(())
    }

    #[test]
    fn init_fails_on_invalid_existing_queue() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;

        // Create a queue with an invalid ID prefix (WRONG-0001 vs RQ)
        let queue_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "WRONG-0001",
      "status": "todo",
      "title": "Bad ID",
      "tags": [],
      "scope": [],
      "evidence": [],
      "plan": [],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.queue_path, queue_json)?;
        std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"code"}"#,
        )?;

        let result = write_queue(
            &resolved.queue_path,
            false,
            &resolved.id_prefix,
            resolved.id_width,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("validate existing queue"));

        Ok(())
    }

    #[test]
    fn init_fails_on_invalid_existing_done() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;

        std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;

        // Create a done file with a task that has invalid ID prefix
        let done_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "WRONG-0002",
      "status": "done",
      "title": "Bad ID",
      "tags": [],
      "scope": [],
      "evidence": [],
      "plan": [],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.done_path, done_json)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"code"}"#,
        )?;

        let result = write_done(
            &resolved.done_path,
            false,
            &resolved.id_prefix,
            resolved.id_width,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("validate existing done"));

        Ok(())
    }

    #[test]
    fn init_with_wizard_answers_creates_configured_files() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        let wizard_answers = WizardAnswers {
            runner: Runner::Codex,
            model: "gpt-5.3-codex".to_string(),
            phases: 2,
            create_first_task: true,
            first_task_title: Some("Test task".to_string()),
            first_task_description: Some("Test description".to_string()),
            first_task_priority: TaskPriority::High,
        };

        // Manually write the queue with wizard answers to test the write_queue function
        write_queue(
            &resolved.queue_path,
            true,
            &resolved.id_prefix,
            resolved.id_width,
            Some(&wizard_answers),
        )?;

        write_config(
            resolved.project_config_path.as_ref().unwrap(),
            true,
            Some(&wizard_answers),
        )?;

        // Verify config has correct runner and phases
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&cfg_raw)?;
        assert_eq!(cfg.agent.runner, Some(Runner::Codex));
        assert_eq!(cfg.agent.phases, Some(2));

        // Verify queue has first task
        let queue = crate::queue::load_queue(&resolved.queue_path)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].title, "Test task");
        assert_eq!(queue.tasks[0].priority, TaskPriority::High);

        Ok(())
    }
}
