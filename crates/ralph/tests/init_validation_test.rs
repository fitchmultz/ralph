//! Integration tests for init command validation and config wiring.

use anyhow::Result;
use ralph::commands::init::{InitOptions, run_init};
use ralph::config::Resolved;
use ralph::contracts::Config;
use tempfile::TempDir;

fn resolved_for(dir: &TempDir) -> Resolved {
    let repo_root = dir.path().to_path_buf();
    let queue_path = repo_root.join(".ralph/queue.json");
    let done_path = repo_root.join(".ralph/done.json");
    let project_config_path = Some(repo_root.join(".ralph/config.json"));
    Resolved {
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
fn init_fails_on_invalid_config_json() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for(&dir);
    std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;

    // Write invalid JSON to config
    std::fs::write(resolved.project_config_path.as_ref().unwrap(), "NOT JSON")?;

    // Write valid queue/done to focus on config failure
    std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
    std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;

    let result = run_init(
        &resolved,
        InitOptions {
            force: false,
            force_lock: false,
            interactive: false,
            update_readme: false,
        },
    );

    assert!(result.is_err(), "Init should fail on invalid config JSON");
    Ok(())
}

#[test]
fn init_fails_on_structurally_invalid_queue() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for(&dir);
    std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;

    // Create a queue with missing required fields (structurally invalid for domain logic)
    // Here we omit created_at/updated_at/etc
    let queue_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Invalid Task"
    }
  ]
}"#;
    std::fs::write(&resolved.queue_path, queue_json)?;
    std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
    std::fs::write(
        resolved.project_config_path.as_ref().unwrap(),
        r#"{"version":1,"project_type":"code"}"#,
    )?;

    let result = run_init(
        &resolved,
        InitOptions {
            force: false,
            force_lock: false,
            interactive: false,
            update_readme: false,
        },
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("validate existing queue"),
        "Error should reference queue validation failure"
    );
    Ok(())
}
