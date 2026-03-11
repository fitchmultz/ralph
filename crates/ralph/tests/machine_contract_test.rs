//! Integration tests for the `ralph machine` contract surface.
//!
//! Responsibilities:
//! - Verify the app-facing machine commands return stable JSON documents.
//! - Exercise queue read, task create, and task mutate through the public machine API.
//! - Keep machine-surface regressions isolated from human CLI snapshot tests.
//!
//! Does not handle:
//! - Exhaustive coverage for every human CLI command.
//! - macOS app decoding behavior.
//!
//! Invariants/assumptions callers must respect:
//! - Tests execute against the built `ralph` binary from Cargo.
//! - Fixtures use disposable Ralph repos initialized through the public CLI.

mod test_support;

use anyhow::Result;
use serde_json::Value;
use tempfile::tempdir;
use test_support::{git_init, ralph_init, run_in_dir};

use ralph::contracts::{TaskPriority, TaskStatus};

#[test]
fn machine_queue_read_returns_versioned_snapshot() -> Result<()> {
    let dir = tempdir()?;
    git_init(dir.path())?;
    ralph_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "queue", "read"]);
    assert!(
        status.success(),
        "machine queue read failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 1);
    assert!(document["paths"]["queue_path"].is_string());
    assert!(document["active"]["tasks"].is_array());
    assert!(document["done"]["tasks"].is_array());
    Ok(())
}

#[test]
fn machine_task_create_and_mutate_round_trip() -> Result<()> {
    let dir = tempdir()?;
    git_init(dir.path())?;
    ralph_init(dir.path())?;

    let create_request = serde_json::json!({
        "version": 1,
        "title": "Machine-created task",
        "description": "Created through ralph machine task create",
        "priority": TaskPriority::High.as_str(),
        "tags": ["machine", "app"],
        "scope": ["crates/ralph"],
        "template": null,
        "target": null
    });
    let create_path = dir.path().join("create-request.json");
    std::fs::write(&create_path, serde_json::to_string_pretty(&create_request)?)?;

    let (create_status, create_stdout, create_stderr) = run_in_dir(
        dir.path(),
        &[
            "machine",
            "task",
            "create",
            "--input",
            create_path.to_str().expect("utf-8 create request path"),
        ],
    );
    assert!(
        create_status.success(),
        "machine task create failed\nstdout:\n{create_stdout}\nstderr:\n{create_stderr}"
    );

    let created: Value = serde_json::from_str(&create_stdout)?;
    let task_id = created["task"]["id"]
        .as_str()
        .expect("created task id should be present")
        .to_string();

    let mutate_request = serde_json::json!({
        "version": 1,
        "atomic": true,
        "tasks": [
            {
                "task_id": task_id,
                "edits": [
                    { "field": "status", "value": TaskStatus::Doing.as_str() },
                    { "field": "priority", "value": TaskPriority::Critical.as_str() }
                ]
            }
        ]
    });
    let mutate_path = dir.path().join("mutate-request.json");
    std::fs::write(&mutate_path, serde_json::to_string_pretty(&mutate_request)?)?;

    let (mutate_status, mutate_stdout, mutate_stderr) = run_in_dir(
        dir.path(),
        &[
            "machine",
            "task",
            "mutate",
            "--input",
            mutate_path.to_str().expect("utf-8 mutate request path"),
        ],
    );
    assert!(
        mutate_status.success(),
        "machine task mutate failed\nstdout:\n{mutate_stdout}\nstderr:\n{mutate_stderr}"
    );

    let mutate_document: Value = serde_json::from_str(&mutate_stdout)?;
    assert_eq!(mutate_document["version"], 1);
    assert_eq!(mutate_document["report"]["tasks"][0]["applied_edits"], 2);

    let (read_status, read_stdout, read_stderr) =
        run_in_dir(dir.path(), &["machine", "queue", "read"]);
    assert!(
        read_status.success(),
        "machine queue read failed\nstdout:\n{read_stdout}\nstderr:\n{read_stderr}"
    );
    let read_document: Value = serde_json::from_str(&read_stdout)?;
    let tasks = read_document["active"]["tasks"]
        .as_array()
        .expect("queue read tasks array");
    let updated_task = tasks
        .iter()
        .find(|task| task["id"].as_str() == Some(&task_id))
        .expect("updated task should remain in queue");
    assert_eq!(updated_task["status"], TaskStatus::Doing.as_str());
    assert_eq!(updated_task["priority"], TaskPriority::Critical.as_str());

    Ok(())
}

#[test]
fn machine_system_info_reports_cli_version() -> Result<()> {
    let dir = tempdir()?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "system", "info"]);
    assert!(
        status.success(),
        "machine system info failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 1);
    assert!(document["cli_version"].as_str().is_some());
    Ok(())
}
