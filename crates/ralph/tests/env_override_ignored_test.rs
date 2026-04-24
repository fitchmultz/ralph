//! Integration tests for ignored RALPH_*_OVERRIDE environment variables.
//!
//! Purpose:
//! - Integration tests for ignored RALPH_*_OVERRIDE environment variables.
//!
//! Responsibilities:
//! - Verify repository resolution is based on command CWD, not override env vars.
//! - Verify queue/done path resolution ignores override env vars.
//! - Verify `queue stop` writes stop signals to the CWD repo.
//!
//! Not handled here:
//! - Parallel execution orchestration behavior.
//! - CI gate command semantics.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Test repos are isolated temp git repos.
//! - `ralph config paths` prints stable `key: value` lines for path keys.

use anyhow::Result;
use std::path::Path;
use std::process::Command;

mod test_support;

fn canonical_or_self(path: &Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn parse_path_line(output: &str, key: &str) -> String {
    let prefix = format!("{key}: ");
    output
        .lines()
        .find_map(|line| {
            line.strip_prefix(&prefix)
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| panic!("missing `{key}:` in output:\n{output}"))
}

#[test]
fn config_paths_ignores_repo_root_override_env() -> Result<()> {
    let parent = test_support::temp_dir_outside_repo();
    test_support::git_init(parent.path())?;

    let workspace = test_support::temp_dir_outside_repo();
    test_support::git_init(workspace.path())?;

    let output = Command::new(test_support::ralph_bin())
        .current_dir(workspace.path())
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", parent.path())
        .args(["config", "paths"])
        .output()?;

    anyhow::ensure!(
        output.status.success(),
        "config paths failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let repo_root = parse_path_line(&stdout, "repo_root");

    assert_eq!(
        canonical_or_self(Path::new(&repo_root)),
        canonical_or_self(workspace.path()),
        "repo_root should follow CWD, not RALPH_REPO_ROOT_OVERRIDE"
    );

    Ok(())
}

#[test]
fn config_paths_ignores_queue_and_done_override_env() -> Result<()> {
    let parent = test_support::temp_dir_outside_repo();
    test_support::git_init(parent.path())?;

    let workspace = test_support::temp_dir_outside_repo();
    test_support::git_init(workspace.path())?;

    let output = Command::new(test_support::ralph_bin())
        .current_dir(workspace.path())
        .env_remove("RUST_LOG")
        .env(
            "RALPH_QUEUE_PATH_OVERRIDE",
            parent.path().join("external-queue.json"),
        )
        .env(
            "RALPH_DONE_PATH_OVERRIDE",
            parent.path().join("external-done.json"),
        )
        .args(["config", "paths"])
        .output()?;

    anyhow::ensure!(
        output.status.success(),
        "config paths failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let queue = parse_path_line(&stdout, "queue");
    let done = parse_path_line(&stdout, "done");
    let workspace_root = canonical_or_self(workspace.path());

    assert_eq!(
        canonical_or_self(Path::new(&queue)),
        workspace_root.join(".ralph/queue.jsonc"),
        "queue path should ignore RALPH_QUEUE_PATH_OVERRIDE"
    );
    assert_eq!(
        canonical_or_self(Path::new(&done)),
        workspace_root.join(".ralph/done.jsonc"),
        "done path should ignore RALPH_DONE_PATH_OVERRIDE"
    );

    Ok(())
}

#[test]
fn queue_stop_writes_stop_signal_in_cwd_repo_even_with_override_env() -> Result<()> {
    let parent = test_support::temp_dir_outside_repo();
    test_support::git_init(parent.path())?;

    let workspace = test_support::temp_dir_outside_repo();
    test_support::git_init(workspace.path())?;

    let output = Command::new(test_support::ralph_bin())
        .current_dir(workspace.path())
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", parent.path())
        .env(
            "RALPH_QUEUE_PATH_OVERRIDE",
            parent.path().join("external-queue.json"),
        )
        .env(
            "RALPH_DONE_PATH_OVERRIDE",
            parent.path().join("external-done.json"),
        )
        .args(["queue", "stop"])
        .output()?;

    anyhow::ensure!(
        output.status.success(),
        "queue stop failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        workspace
            .path()
            .join(".ralph/cache/stop_requested")
            .exists(),
        "stop signal should be written to workspace repo"
    );
    assert!(
        !parent.path().join(".ralph/cache/stop_requested").exists(),
        "stop signal should not be written to override-target parent repo"
    );

    Ok(())
}
