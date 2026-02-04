//! Integration tests for parallel-mode workspace_root gitignore preflight.
//!
//! Responsibilities:
//! - Fail fast when parallel.workspace_root resolves inside repo_root and is not ignored.
//! - Avoid creating workspace directories/state on that failure path.
//! - Confirm the preflight does not block when the path is ignored.
//!
//! Not handled here:
//! - Workspace clone correctness (covered by git/workspace.rs tests).
//! - Origin/gh preflights beyond ensuring they don't mask this preflight.
//!
//! Invariants/assumptions:
//! - PR automation is disabled to avoid requiring `gh`.
//! - Tests use `--force` to avoid unrelated clean-repo failures from init scaffolding.

use anyhow::{Context, Result};
use std::process::Command;

mod test_support;

fn disable_pr_automation_and_set_noop_runner(
    repo_root: &std::path::Path,
    runner_path: &std::path::Path,
) -> Result<()> {
    let config_path = repo_root.join(".ralph/config.json");
    let raw = std::fs::read_to_string(&config_path).context("read config")?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).context("parse config")?;

    if config.get("parallel").is_none() {
        config["parallel"] = serde_json::json!({});
    }
    config["parallel"]["auto_pr"] = serde_json::json!(false);
    config["parallel"]["auto_merge"] = serde_json::json!(false);

    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }
    config["agent"]["runner"] = serde_json::json!("opencode");
    config["agent"]["opencode_bin"] = serde_json::json!(runner_path.to_string_lossy().to_string());
    config["agent"]["phases"] = serde_json::json!(1);

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

fn set_workspace_root(repo_root: &std::path::Path, value: &str) -> Result<()> {
    let config_path = repo_root.join(".ralph/config.json");
    let raw = std::fs::read_to_string(&config_path).context("read config")?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).context("parse config")?;

    if config.get("parallel").is_none() {
        config["parallel"] = serde_json::json!({});
    }
    config["parallel"]["workspace_root"] = serde_json::json!(value);

    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

#[test]
fn parallel_run_fails_fast_when_repo_local_workspace_root_not_ignored() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Add origin so the origin preflight doesn't mask our workspace_root preflight
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

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Remove the .ralph/workspaces/ entry that init added, so we can test the failure case
    let gitignore_path = dir.path().join(".gitignore");
    let gitignore_content = std::fs::read_to_string(&gitignore_path)?;
    let filtered: Vec<&str> = gitignore_content
        .lines()
        .filter(|line| !line.contains(".ralph/workspaces"))
        .collect();
    std::fs::write(&gitignore_path, filtered.join("\n") + "\n")?;

    test_support::write_valid_single_todo_queue(dir.path())?;

    // No-op runner.
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path =
        test_support::create_executable_script(&bin_dir, "noop-runner", "#!/bin/bash\nexit 0\n")?;
    disable_pr_automation_and_set_noop_runner(dir.path(), &runner_path)?;

    // Configure a repo-local workspace root that is NOT gitignored.
    set_workspace_root(dir.path(), ".ralph/workspaces")?;

    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir.path())
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .args(["run", "loop", "--parallel", "2", "--force"])
        .output()?;

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!output.status.success(), "expected failure\n{combined}");
    assert!(
        combined.contains("Parallel preflight: parallel.workspace_root resolves inside the repo but is not gitignored"),
        "expected actionable workspace_root gitignore error\n{combined}"
    );

    // Fail-fast proof: no state file, and no workspace dir created.
    assert!(!dir.path().join(".ralph/cache/parallel/state.json").exists());
    assert!(!dir.path().join(".ralph/workspaces").exists());

    Ok(())
}

#[test]
fn parallel_run_not_blocked_by_workspace_root_preflight_when_ignored() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Add origin so later preflights don't fail for unrelated reasons.
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

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path =
        test_support::create_executable_script(&bin_dir, "noop-runner", "#!/bin/bash\nexit 0\n")?;
    disable_pr_automation_and_set_noop_runner(dir.path(), &runner_path)?;

    set_workspace_root(dir.path(), ".ralph/workspaces")?;
    // Ignore it (shared path).
    let gitignore_path = dir.path().join(".gitignore");
    let mut existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    if !existing.contains(".ralph/workspaces/") {
        existing.push_str("\n.ralph/workspaces/\n");
        std::fs::write(&gitignore_path, existing)?;
    }

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "loop", "--parallel", "2", "--force"]);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("Parallel preflight: parallel.workspace_root resolves inside the repo but is not gitignored"),
        "workspace_root gitignore preflight should not trigger when ignored\n{combined}"
    );

    // Do not assert success; later behavior can vary. We only care that this preflight is not the blocker.
    let _ = status;

    Ok(())
}
