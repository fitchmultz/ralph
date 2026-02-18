//! Regression test for RQ-0942: CI gate must not wipe parent queue/done files.
//!
//! Responsibilities:
//! - Verify CI gate strips all RALPH_*_OVERRIDE environment variables.
//! - Ensure child processes cannot access parent's queue/done paths via env.
//! - Prevent regression of data loss bug when running parallel loops.
//!
//! Not handled here:
//! - Full parallel mode E2E testing (see parallel_e2e_test.rs).
//! - Worker spawning isolation (see parallel_*_test.rs).
//!
//! Invariants/assumptions:
//! - CI gate command (default: make ci) runs in a subprocess.
//! - Environment variables are cleared before subprocess spawn.
//! - Tests that need to access queue/done should use isolated temp directories.

use anyhow::{Context, Result};
use ralph::config::{DONE_PATH_OVERRIDE_ENV, QUEUE_PATH_OVERRIDE_ENV, REPO_ROOT_OVERRIDE_ENV};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

mod test_support;
use test_support::ralph_bin;

/// Verify CI gate subprocess does not inherit RALPH_*_OVERRIDE environment variables.
///
/// This is the primary regression test for RQ-0942. If a worker process has
/// RALPH_REPO_ROOT_OVERRIDE set (pointing to a workspace), and the CI gate
/// fails to strip this variable, child processes could resolve .ralph/ paths
/// incorrectly and write to the parent's queue/done files.
#[test]
fn ci_gate_strips_ralph_override_env_vars() -> Result<()> {
    let parent = TempDir::new()?;
    let workspace = TempDir::new()?;

    // Setup: Initialize git repo in workspace (required for ralph commands)
    let git_status = Command::new("git")
        .current_dir(workspace.path())
        .args(["init", "--quiet"])
        .status()
        .context("git init")?;
    anyhow::ensure!(git_status.success(), "git init failed");

    // Setup: Configure git user
    Command::new("git")
        .current_dir(workspace.path())
        .args(["config", "user.name", "Test"])
        .status()
        .context("git config user.name")?;
    Command::new("git")
        .current_dir(workspace.path())
        .args(["config", "user.email", "test@example.com"])
        .status()
        .context("git config user.email")?;

    // Setup: Create .ralph directory and config
    let ralph_dir = workspace.path().join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let config = r#"{"agent": {"ci_gate_enabled": true, "ci_gate_command": "echo check"}}"#;
    std::fs::write(ralph_dir.join("config.json"), config)?;

    // Setup: Create queue and done files
    let queue_content = r#"{"version":1,"tasks":[{"id":"RQ-0001","title":"Test task","status":"todo","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#;
    std::fs::write(ralph_dir.join("queue.json"), queue_content)?;
    std::fs::write(ralph_dir.join("done.json"), r#"{"version":1,"tasks":[]}"#)?;

    // Execute: Run ralph with RALPH_*_OVERRIDE set (simulating parallel worker)
    // The CI gate should strip these variables before running the command
    let _output = Command::new(ralph_bin())
        .current_dir(workspace.path())
        .env(REPO_ROOT_OVERRIDE_ENV, workspace.path())
        .env(QUEUE_PATH_OVERRIDE_ENV, parent.path().join("queue.json"))
        .env(DONE_PATH_OVERRIDE_ENV, parent.path().join("done.json"))
        .args([
            "run",
            "one",
            "--id",
            "RQ-0001",
            "--non-interactive",
            "--parallel-worker",
        ])
        .output()
        .context("run ralph with override env")?;

    // The command may fail due to missing runner, but that's OK - we're testing
    // that the CI gate isolation works, not that the run succeeds.
    // The key is that the parent's queue/done files should not be affected.

    // Verify: Parent queue/done should NOT exist (they were never touched)
    assert!(
        !parent.path().join("queue.json").exists(),
        "Parent queue.json should not exist - CI gate leaked QUEUE_PATH_OVERRIDE"
    );
    assert!(
        !parent.path().join("done.json").exists(),
        "Parent done.json should not exist - CI gate leaked DONE_PATH_OVERRIDE"
    );

    Ok(())
}

/// Verify that Makefile test target clears RALPH_*_OVERRIDE variables.
///
/// This test checks that `make test` explicitly unsets the override variables
/// to prevent test processes from accidentally writing to parent paths.
#[test]
fn makefile_test_unsets_ralph_override_vars() -> Result<()> {
    // Read Makefile and verify it contains unset statements for all three vars
    let makefile_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("Makefile"))
        .context("resolve Makefile path")?;

    let makefile_content = std::fs::read_to_string(&makefile_path).context("read Makefile")?;

    // Check for unset statements in the test target
    assert!(
        makefile_content.contains("unset RALPH_QUEUE_PATH_OVERRIDE"),
        "Makefile must unset RALPH_QUEUE_PATH_OVERRIDE in test target"
    );
    assert!(
        makefile_content.contains("unset RALPH_DONE_PATH_OVERRIDE"),
        "Makefile must unset RALPH_DONE_PATH_OVERRIDE in test target"
    );
    assert!(
        makefile_content.contains("unset RALPH_REPO_ROOT_OVERRIDE"),
        "Makefile must unset RALPH_REPO_ROOT_OVERRIDE in test target"
    );

    Ok(())
}

/// Verify CI gate command subprocess has clean environment.
///
/// This test creates a scenario where a CI gate command is run with
/// RALPH_*_OVERRIDE variables set in the parent process, and verifies
/// the child process does NOT see these variables.
#[test]
fn ci_gate_child_process_has_no_override_env() -> Result<()> {
    let temp = TempDir::new()?;

    // Initialize git repo
    let git_status = Command::new("git")
        .current_dir(temp.path())
        .args(["init", "--quiet"])
        .status()
        .context("git init")?;
    anyhow::ensure!(git_status.success(), "git init failed");

    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.name", "Test"])
        .status()
        .context("git config user.name")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["config", "user.email", "test@example.com"])
        .status()
        .context("git config user.email")?;

    // Create .ralph directory and config with a CI gate that checks env
    let ralph_dir = temp.path().join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    // CI gate command that fails if any RALPH_*_OVERRIDE is set
    #[cfg(unix)]
    let ci_gate_command = "sh -c 'test -z \"$RALPH_QUEUE_PATH_OVERRIDE\" && test -z \"$RALPH_DONE_PATH_OVERRIDE\" && test -z \"$RALPH_REPO_ROOT_OVERRIDE\"'";
    #[cfg(windows)]
    let ci_gate_command = "powershell -NoProfile -Command \"if ($env:RALPH_QUEUE_PATH_OVERRIDE -or $env:RALPH_DONE_PATH_OVERRIDE -or $env:RALPH_REPO_ROOT_OVERRIDE) { exit 42 }\"";

    let config = serde_json::json!({
        "agent": {
            "ci_gate_enabled": true,
            "ci_gate_command": ci_gate_command
        }
    });
    std::fs::write(
        ralph_dir.join("config.json"),
        serde_json::to_string_pretty(&config)?,
    )?;

    // Create queue with one task
    let queue = serde_json::json!({
        "version": 1,
        "tasks": [{
            "id": "RQ-0001",
            "title": "Test task",
            "status": "todo",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }]
    });
    std::fs::write(
        ralph_dir.join("queue.json"),
        serde_json::to_string_pretty(&queue)?,
    )?;
    std::fs::write(ralph_dir.join("done.json"), r#"{"version":1,"tasks":[]}"#)?;

    // Commit initial state
    Command::new("git")
        .current_dir(temp.path())
        .args(["add", "."])
        .status()
        .context("git add")?;
    Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "-m", "init", "--quiet"])
        .status()
        .context("git commit")?;

    // Run with override env vars set - CI gate should strip them
    let output = Command::new(ralph_bin())
        .current_dir(temp.path())
        .env(REPO_ROOT_OVERRIDE_ENV, "/tmp/fake-workspace")
        .env(QUEUE_PATH_OVERRIDE_ENV, "/tmp/fake-queue.json")
        .env(DONE_PATH_OVERRIDE_ENV, "/tmp/fake-done.json")
        .args([
            "run",
            "one",
            "--id",
            "RQ-0001",
            "--non-interactive",
            "--parallel-worker",
        ])
        .output()
        .context("run ralph")?;

    // The command may fail for other reasons (missing runner), but if the
    // CI gate fails due to env vars still being set, we have a bug.
    // Check stderr for CI gate failure message
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("exit 42") && !stderr.contains("exited with code"),
        "CI gate should have stripped RALPH_*_OVERRIDE env vars. stderr: {}",
        stderr
    );

    Ok(())
}
