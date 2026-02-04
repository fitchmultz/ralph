//! Integration tests for parallel-mode preflight validation of queue/done paths.
//!
//! Responsibilities:
//! - Verify that parallel mode rejects queue/done paths that resolve outside the repo root.
//! - Verify fail-fast behavior occurs before any workers spawn (no parallel state file created).
//!
//! Not handled here:
//! - Workspace mapping behavior itself (covered by path_map and merge runner tests).
//! - Non-parallel behavior for absolute queue/done paths.
//!
//! Invariants/assumptions:
//! - PR automation is disabled in config to avoid `gh` availability affecting results.
//! - Tests run `ralph run loop --parallel 2 --force` to hit the real parallel preflight.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

mod test_support;

fn create_noop_runner(bin_dir: &Path) -> Result<PathBuf> {
    let runner_script = "#!/bin/bash\nexit 0\n";
    test_support::create_executable_script(bin_dir, "noop-runner", runner_script)
}

fn write_valid_queue(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create queue parent dir")?;
    }
    let queue = r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"todo","title":"Test","scope":["file.rs"],"evidence":["obs"],"plan":["step"],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#;
    std::fs::write(path, queue).context("write queue")?;
    Ok(())
}

fn write_valid_done(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create done parent dir")?;
    }
    let done = r#"{"version":1,"tasks":[]}"#;
    std::fs::write(path, done).context("write done")?;
    Ok(())
}

fn configure_paths_and_disable_pr_automation(
    repo_root: &Path,
    queue_path: &Path,
    done_path: &Path,
    runner_path: &Path,
) -> Result<()> {
    let config_path = repo_root.join(".ralph/config.json");
    let raw = std::fs::read_to_string(&config_path).context("read config")?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).context("parse config")?;

    if config.get("queue").is_none() {
        config["queue"] = serde_json::json!({});
    }
    config["queue"]["file"] = serde_json::json!(queue_path.to_string_lossy().to_string());
    config["queue"]["done_file"] = serde_json::json!(done_path.to_string_lossy().to_string());

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

    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    Ok(())
}

#[test]
fn parallel_run_fails_fast_when_queue_and_done_outside_repo_root() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // External queue/done files (outside repo root)
    let external = test_support::temp_dir_outside_repo();
    let queue_path = external.path().join("queue.json");
    let done_path = external.path().join("done.json");
    write_valid_queue(&queue_path)?;
    write_valid_done(&done_path)?;

    // No-op runner (defensive: if preflight regresses, we don't fail due to missing runner)
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = create_noop_runner(&bin_dir)?;

    configure_paths_and_disable_pr_automation(dir.path(), &queue_path, &done_path, &runner_path)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "loop", "--parallel", "2", "--force"]);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !status.success(),
        "expected failure due to repo containment preflight\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains("queue path") && combined.contains("not under repo root"),
        "expected 'queue path ... not under repo root' error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Fail-fast proof: parallel state file should not exist (workers not spawned).
    let state_path = dir.path().join(".ralph/cache/parallel/state.json");
    assert!(
        !state_path.exists(),
        "state file should not exist due to fail-fast"
    );

    Ok(())
}

#[test]
fn parallel_run_fails_fast_when_done_outside_repo_root() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure a valid in-repo queue exists (so we isolate failure to done-path containment)
    test_support::write_valid_single_todo_queue(dir.path())?;

    // External done file (outside repo root)
    let external = test_support::temp_dir_outside_repo();
    let done_path = external.path().join("done.json");
    write_valid_done(&done_path)?;

    let queue_path = dir.path().join(".ralph/queue.json");

    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = create_noop_runner(&bin_dir)?;

    configure_paths_and_disable_pr_automation(dir.path(), &queue_path, &done_path, &runner_path)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "loop", "--parallel", "2", "--force"]);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !status.success(),
        "expected failure due to done-path repo containment preflight\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        combined.contains("done path") && combined.contains("not under repo root"),
        "expected 'done path ... not under repo root' error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let state_path = dir.path().join(".ralph/cache/parallel/state.json");
    assert!(
        !state_path.exists(),
        "state file should not exist due to fail-fast"
    );

    Ok(())
}
