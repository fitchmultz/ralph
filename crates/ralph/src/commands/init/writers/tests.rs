//! Regression coverage for initialization file writers.
//!
//! Purpose:
//! - Regression coverage for initialization file writers.
//!
//! Responsibilities:
//! - Verify queue/done/config writer behavior for create, validate, and force-overwrite flows.
//! - Verify wizard answers seed queue/config output as expected.
//!
//! Not handled here:
//! - Interactive initialization UX.
//! - README creation or migration checks.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Existing files stay untouched when validation succeeds without `--force`.
//! - Wizard-backed writes preserve the expected initial task and config fields.

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
    let config_status = write_config(resolved.project_config_path.as_ref().unwrap(), false, None)?;

    assert_eq!(queue_status, FileInitStatus::Created);
    assert_eq!(done_status, FileInitStatus::Created);
    assert_eq!(config_status, FileInitStatus::Created);

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.version, 1);
    let done = crate::queue::load_queue(&resolved.done_path)?;
    assert_eq!(done.version, 1);
    let raw_cfg = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
    let cfg: Config = serde_json::from_str(&raw_cfg)?;
    assert_eq!(cfg.version, 2);

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
  "version": 2,
  "queue": {
    "file": ".ralph/queue.jsonc"
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
    let config_status = write_config(resolved.project_config_path.as_ref().unwrap(), false, None)?;

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
        r#"{"version":2,"project_type":"docs"}"#,
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
    let config_status = write_config(resolved.project_config_path.as_ref().unwrap(), true, None)?;

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
        r#"{"version":2,"project_type":"code"}"#,
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
        r#"{"version":2,"project_type":"code"}"#,
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
        model: "gpt-5.4".to_string(),
        phases: 2,
        create_first_task: true,
        first_task_title: Some("Test task".to_string()),
        first_task_description: Some("Test description".to_string()),
        first_task_priority: TaskPriority::High,
    };

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

    let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
    let cfg: Config = serde_json::from_str(&cfg_raw)?;
    assert_eq!(cfg.agent.runner, Some(Runner::Codex));
    assert_eq!(cfg.agent.phases, Some(2));

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].title, "Test task");
    assert_eq!(queue.tasks[0].priority, TaskPriority::High);

    Ok(())
}
