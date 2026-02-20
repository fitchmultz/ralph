//! Integration test for parallel mode gh CLI preflight check.
//!
//! Responsibilities:
//! - Verify that parallel mode fails fast when gh CLI is not available but auto_pr/auto_merge is enabled.
//! - Verify that the error message is clear and actionable.
//! - Verify that parallel mode proceeds when gh is available and authenticated.
//!
//! Not handled here:
//! - Testing actual PR creation (that requires real gh with repository access).
//!
//! Invariants/assumptions:
//! - The test creates a fake gh binary to simulate various gh states.
//! - The fake gh is placed first in PATH to shadow any real gh installation.

use anyhow::Result;
use std::process::Command;

mod test_support;

/// Creates a fake gh binary that fails with a given message.
fn create_fake_gh_failing(bin_dir: &std::path::Path, fail_message: &str) -> Result<()> {
    let gh_script = format!(
        r#"#!/bin/bash
echo "{}" >&2
exit 1
"#,
        fail_message
    );
    test_support::create_executable_script(bin_dir, "gh", &gh_script)?;
    Ok(())
}

/// Creates a fake runner that would succeed (but shouldn't be called due to fail-fast).
fn create_noop_runner(bin_dir: &std::path::Path) -> Result<()> {
    let runner_script = r#"#!/bin/bash
# This runner should never be called due to gh preflight failure
exit 0
"#;
    test_support::create_executable_script(bin_dir, "noop-runner", runner_script)?;
    Ok(())
}

#[test]
fn parallel_run_fails_fast_when_gh_not_found() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Initialize ralph
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create bin directory with fake failing gh
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    create_fake_gh_failing(&bin_dir, "gh: command not found")?;
    create_noop_runner(&bin_dir)?;

    // Configure to use the noop runner and enable auto_pr
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"opencode","opencode_bin":"{}","phases":1}},"parallel":{{"auto_pr":true,"auto_merge":false}}}}"#,
        bin_dir.join("noop-runner").display()
    );
    std::fs::write(&config_path, config_content)?;

    // Run parallel mode with the fake gh in PATH first
    // Note: We don't add a task because the gh preflight check should fail before task selection
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .args(["run", "loop", "--parallel", "2", "--force"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should fail due to gh preflight check
    assert!(
        !output.status.success(),
        "expected failure due to gh preflight\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Should contain clear error message about gh
    assert!(
        combined.contains("gh CLI check failed") || combined.contains("GitHub CLI"),
        "expected gh-related error message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify state file was NOT created (fail-fast before expensive orchestration)
    let state_path = dir.path().join(".ralph/cache/parallel/state.json");
    assert!(
        !state_path.exists(),
        "state file should not exist due to fail-fast"
    );

    Ok(())
}

#[test]
fn parallel_run_fails_fast_when_gh_not_authenticated() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Initialize ralph
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create bin directory with fake gh that succeeds for --version but fails for auth status
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    // Create a fake gh that passes --version but fails auth status
    let gh_script = r#"#!/bin/bash
if [ "$1" = "--version" ]; then
    echo "gh version 2.40.0"
    exit 0
elif [ "$1" = "auth" ] && [ "$2" = "status" ]; then
    echo "You are not logged into any GitHub hosts. Run gh auth login to authenticate." >&2
    exit 1
fi
exit 1
"#;
    test_support::create_executable_script(&bin_dir, "gh", gh_script)?;
    create_noop_runner(&bin_dir)?;

    // Configure to use the noop runner and enable auto_merge (which requires gh)
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"opencode","opencode_bin":"{}","phases":1}},"parallel":{{"auto_pr":false,"auto_merge":true}}}}"#,
        bin_dir.join("noop-runner").display()
    );
    std::fs::write(&config_path, config_content)?;

    // Run parallel mode with the fake gh in PATH first
    // Note: We don't add a task because the gh preflight check should fail before task selection
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .args(["run", "loop", "--parallel", "2", "--force"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should fail due to gh preflight check
    assert!(
        !output.status.success(),
        "expected failure due to gh auth preflight\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Should contain clear error message about gh not being authenticated
    assert!(
        combined.contains("not authenticated") || combined.contains("gh auth login"),
        "expected gh auth-related error message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn parallel_run_skips_gh_check_when_auto_pr_disabled() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Initialize ralph
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create bin directory with fake gh that always fails
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    create_fake_gh_failing(&bin_dir, "gh: command not found")?;

    // Configure with auto_pr and auto_merge disabled - gh check should be skipped
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"opencode","opencode_bin":"{}/nonexistent","phases":1}},"parallel":{{"auto_pr":false,"auto_merge":false}}}}"#,
        bin_dir.display()
    );
    std::fs::write(&config_path, config_content)?;

    // Note: We're not adding any tasks, so the loop should exit with "No todo tasks"
    // The key point is that it should NOT fail with gh-related error

    // Run parallel mode with the fake gh in PATH first
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .args(["run", "loop", "--parallel", "2", "--force"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should succeed (or at least not fail due to gh - it may fail due to no tasks or runner not found)
    // The important thing is it should NOT fail with gh-related error
    assert!(
        !combined.contains("gh CLI check failed") && !combined.contains("GitHub CLI"),
        "should not fail with gh error when auto_pr/auto_merge are disabled\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn parallel_run_proceeds_when_gh_available_and_authenticated() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Initialize ralph
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Create bin directory with fake gh that succeeds for both checks
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    // Create a fake gh that passes both --version and auth status
    let gh_script = r#"#!/bin/bash
if [ "$1" = "--version" ]; then
    echo "gh version 2.40.0"
    exit 0
elif [ "$1" = "auth" ] && [ "$2" = "status" ]; then
    echo "Logged in to github.com as test-user"
    exit 0
fi
exit 0
"#;
    test_support::create_executable_script(&bin_dir, "gh", gh_script)?;
    create_noop_runner(&bin_dir)?;

    // Configure to use the noop runner and enable auto_pr (requires gh)
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"opencode","opencode_bin":"{}","phases":1}},"parallel":{{"auto_pr":true,"auto_merge":false}}}}"#,
        bin_dir.join("noop-runner").display()
    );
    std::fs::write(&config_path, config_content)?;

    // Run parallel mode with the fake gh in PATH first
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .args(["run", "loop", "--parallel", "2", "--force"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Should NOT fail with gh-related error (it may fail for other reasons like no tasks)
    // The key assertion: no gh preflight failure
    assert!(
        !combined.contains("gh CLI check failed")
            && !combined.contains("GitHub CLI")
            && !combined.contains("not authenticated")
            && !combined.contains("gh auth login"),
        "should not fail with gh error when gh is available and authenticated\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
