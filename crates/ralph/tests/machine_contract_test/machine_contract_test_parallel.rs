//! Parallel runtime contract coverage for `ralph machine`.
//!
//! Purpose:
//! - Verify machine-visible parallel execution and run-started documents.
//!
//! Responsibilities:
//! - Assert the idle parallel-status document shape and blocking continuations.
//! - Cover stale queue lock and blocked worker recovery states.
//! - Verify `run_started` preserves repo trust fields in the config payload.
//!
//! Non-scope:
//! - Queue/task mutation contracts.
//! - System info and doctor report contracts.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Parallel fixture files remain synthetic and local to each disposable repo.
//! - Assertions intentionally preserve the legacy flat suite’s exact contract expectations.

use super::machine_contract_test_support::{run_in_dir, setup_ralph_repo, trust_project_commands};
use anyhow::Result;
use serde_json::Value;

#[test]
fn machine_parallel_status_returns_versioned_continuation_document() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "parallel-status"]);
    assert!(
        status.success(),
        "machine run parallel-status failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 3);
    assert_eq!(document["lifecycle_counts"]["running"], 0);
    assert_eq!(document["lifecycle_counts"]["integrating"], 0);
    assert_eq!(document["lifecycle_counts"]["completed"], 0);
    assert_eq!(document["lifecycle_counts"]["failed"], 0);
    assert_eq!(document["lifecycle_counts"]["blocked"], 0);
    assert_eq!(document["lifecycle_counts"]["total"], 0);
    assert_eq!(document["blocking"], Value::Null);
    assert_eq!(
        document["continuation"]["headline"],
        "Parallel execution has not started."
    );
    assert_eq!(document["status"]["message"], "No parallel state found");
    Ok(())
}

#[test]
fn machine_parallel_status_surfaces_stale_queue_lock_operator_state() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let lock_dir = dir.path().join(".ralph/lock");
    std::fs::create_dir_all(&lock_dir)?;
    let stale_pid = 999_999;
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-03-21T12:00:00Z\ncommand: ralph run loop --parallel 4\nlabel: run loop\n"
        ),
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "parallel-status"]);
    assert!(
        status.success(),
        "machine run parallel-status failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 3);
    assert_eq!(document["lifecycle_counts"]["total"], 0);
    assert_eq!(document["blocking"]["status"], "stalled");
    assert_eq!(document["blocking"]["reason"]["kind"], "lock_blocked");
    assert_eq!(
        document["continuation"]["headline"],
        "Parallel execution is stalled on queue lock recovery."
    );
    assert_eq!(
        document["continuation"]["next_steps"][0]["command"],
        "ralph queue unlock"
    );
    Ok(())
}

#[test]
fn machine_run_started_preserves_repo_trust_in_config_payload() -> Result<()> {
    let dir = setup_ralph_repo()?;
    trust_project_commands(dir.path())?;

    let lock_dir = dir.path().join(".ralph/lock");
    std::fs::create_dir_all(&lock_dir)?;
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {}\nstarted_at: 2026-03-21T12:00:00Z\ncommand: ralph run loop --parallel 2\nlabel: run loop\n",
            std::process::id()
        ),
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "one", "--resume"]);
    assert!(
        !status.success(),
        "machine run one should stall on the active lock\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let run_started: Value = serde_json::from_str(
        stdout
            .lines()
            .next()
            .expect("expected machine run event before the lock failure"),
    )?;
    assert_eq!(run_started["kind"], "run_started");
    assert_eq!(
        run_started["payload"]["config"]["safety"]["repo_trusted"],
        Value::Bool(true)
    );
    assert_eq!(
        run_started["payload"]["config"]["safety"]["dirty_repo"],
        Value::Bool(true)
    );

    Ok(())
}

#[test]
fn machine_parallel_status_surfaces_blocked_worker_operator_state() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let state_dir = dir.path().join(".ralph/cache/parallel");
    std::fs::create_dir_all(&state_dir)?;
    let state_path = state_dir.join("state.json");
    let workspace_path = dir.path().join(".ralph/workspaces/RQ-1001");
    std::fs::create_dir_all(workspace_path.join(".ralph/cache/parallel"))?;
    std::fs::write(
        workspace_path.join(".ralph/cache/parallel/blocked_push.json"),
        serde_json::json!({
            "task_id": "RQ-1001",
            "reason": "push rejected after conflict review",
            "attempt": 3,
            "max_attempts": 5,
            "generated_at": "2026-03-21T12:05:00Z"
        })
        .to_string(),
    )?;

    let state = serde_json::json!({
        "schema_version": 3,
        "started_at": "2026-03-21T12:00:00Z",
        "target_branch": "main",
        "workers": [{
            "task_id": "RQ-1001",
            "workspace_path": workspace_path.display().to_string(),
            "lifecycle": "blocked_push",
            "started_at": "2026-03-21T12:00:00Z",
            "completed_at": "2026-03-21T12:05:00Z",
            "push_attempts": 3,
            "last_error": "push rejected after conflict review"
        }]
    });
    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "parallel-status"]);
    assert!(
        status.success(),
        "machine run parallel-status failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 3);
    assert_eq!(document["lifecycle_counts"]["blocked"], 1);
    assert_eq!(document["lifecycle_counts"]["total"], 1);
    assert_eq!(document["lifecycle_counts"]["running"], 0);
    assert_eq!(document["lifecycle_counts"]["integrating"], 0);
    assert_eq!(document["lifecycle_counts"]["completed"], 0);
    assert_eq!(document["lifecycle_counts"]["failed"], 0);
    assert_eq!(document["blocking"]["status"], "blocked");
    assert_eq!(document["blocking"]["reason"]["kind"], "operator_recovery");
    assert_eq!(document["blocking"]["reason"]["scope"], "parallel");
    assert_eq!(document["blocking"]["reason"]["reason"], "blocked_push");
    assert_eq!(document["continuation"]["blocking"], document["blocking"]);
    assert!(
        document["continuation"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("Retained for recovery:"))
    );
    assert!(
        document["continuation"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("Operator action required:"))
    );
    assert_eq!(
        document["continuation"]["next_steps"][1]["command"],
        "ralph run parallel retry --task <TASK_ID>"
    );
    Ok(())
}
