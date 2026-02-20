//! Integration test for parallel mode origin-remote preflight check.
//!
//! Responsibilities:
//! - Verify that parallel mode fails fast when no `origin` remote is configured.
//! - Verify that the error message is clear/actionable (suggests adding origin or disabling parallel).
//! - Verify fail-fast behavior occurs before orchestration (no parallel state file created).
//!
//! Not handled here:
//! - Validating push authentication/authorization (network/credentials dependent).
//! - Success-path parallel execution.
//!
//! Invariants/assumptions:
//! - We disable PR automation to avoid requiring `gh` in this test.
//! - `--force` is used to bypass clean-repo requirements during test setup.

use anyhow::Result;
use std::process::Command;

mod test_support;

#[test]
fn parallel_run_fails_fast_when_origin_remote_missing() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Initialize ralph project files.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure a valid queue exists so we reach the origin preflight.
    test_support::write_valid_single_todo_queue(dir.path())?;

    // Create a no-op runner (defensive; preflight should fail before runner is used).
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let _runner_path =
        test_support::create_executable_script(&bin_dir, "noop-runner", "#!/bin/bash\nexit 0\n")?;

    // Update config: set runner + disable PR automation so gh is not required.
    test_support::configure_runner(dir.path(), "opencode", "unused", Some(&_runner_path))?;
    {
        let config_path = dir.path().join(".ralph/config.json");
        let raw = std::fs::read_to_string(&config_path)?;
        let mut config: serde_json::Value = serde_json::from_str(&raw)?;
        if config.get("parallel").is_none() {
            config["parallel"] = serde_json::json!({});
        }
        config["parallel"]["auto_pr"] = serde_json::json!(false);
        config["parallel"]["auto_merge"] = serde_json::json!(false);
        std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    }

    // Run parallel mode; repo intentionally has no `origin` remote.
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .args(["run", "loop", "--parallel", "2", "--force"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "expected failure due to missing origin preflight\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Error should be clear + actionable.
    assert!(
        combined.contains("origin"),
        "expected message mentioning 'origin'\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains("git remote add origin"),
        "expected suggestion to add origin\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.to_lowercase().contains("disable parallel"),
        "expected suggestion to disable parallel mode\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Fail-fast proof: no parallel state file created.
    let state_path = dir.path().join(".ralph/cache/parallel/state.json");
    assert!(
        !state_path.exists(),
        "state file should not exist due to fail-fast"
    );

    Ok(())
}

#[test]
fn parallel_run_succeeds_when_origin_remote_exists() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Add origin remote (required for parallel mode)
    let status = Command::new("git")
        .current_dir(dir.path())
        .args([
            "remote",
            "add",
            "origin",
            "https://example.com/test-repo.git",
        ])
        .status()?;
    anyhow::ensure!(status.success(), "git remote add origin failed");

    // Initialize ralph project files.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure a valid queue exists.
    test_support::write_valid_single_todo_queue(dir.path())?;

    // Create a no-op runner.
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let _runner_path =
        test_support::create_executable_script(&bin_dir, "noop-runner", "#!/bin/bash\nexit 0\n")?;

    // Update config: set runner + disable PR automation so gh is not required.
    test_support::configure_runner(dir.path(), "opencode", "unused", Some(&_runner_path))?;
    {
        let config_path = dir.path().join(".ralph/config.json");
        let raw = std::fs::read_to_string(&config_path)?;
        let mut config: serde_json::Value = serde_json::from_str(&raw)?;
        if config.get("parallel").is_none() {
            config["parallel"] = serde_json::json!({});
        }
        config["parallel"]["auto_pr"] = serde_json::json!(false);
        config["parallel"]["auto_merge"] = serde_json::json!(false);
        std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    }

    // Run parallel mode; repo has origin remote so it should pass preflight.
    // Note: It may still fail due to no tasks being available or runner issues,
    // but it should NOT fail due to missing origin.
    // Use --max-tasks 1 to ensure the parallel loop terminates.
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .args([
            "run",
            "loop",
            "--parallel",
            "2",
            "--force",
            "--max-tasks",
            "1",
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should NOT fail due to missing origin.
    assert!(
        !combined.contains("No 'origin' git remote configured"),
        "should not fail with origin error when origin exists\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Should NOT fail with origin preflight message.
    assert!(
        !combined.contains("origin remote check failed"),
        "should not fail with origin preflight error when origin exists\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
