//! Parallel-worker bookkeeping restore regressions.
//!
//! Purpose:
//! - Parallel-worker bookkeeping restore regressions.
//!
//! Responsibilities:
//! - Verify worker post-run supervision restores queue/done/productivity bookkeeping files.
//! - Exercise worker-specific failure handling, CI diagnostics, and publish behavior.
//!
//! Not handled here:
//! - Regular post-run supervision commit/push behavior.
//! - Continue-session resume flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The test repo snapshots queue, done, and productivity files before dirtying them.

use super::support::{resolved_for_repo, write_queue};
use crate::commands::run::supervision::{PushPolicy, post_run_supervise_parallel_worker};
use crate::contracts::{CiGateConfig, GitPublishMode, GitRevertMode, QueueFile, TaskStatus};
use crate::queue;
use crate::testsupport::git as git_test;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn init_parallel_worker_repo(
    repo_root: &Path,
) -> anyhow::Result<(crate::config::Resolved, PathBuf, String, String, String)> {
    git_test::init_repo(repo_root)?;

    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    write_queue(repo_root, TaskStatus::Todo)?;
    queue::save_queue(
        &repo_root.join(".ralph/done.jsonc"),
        &QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    let productivity_path = cache_dir.join("productivity.json");
    std::fs::write(&productivity_path, r#"{"stats":[]}"#)?;
    git_test::commit_all(repo_root, "init queue/done/productivity")?;

    let resolved = resolved_for_repo(repo_root);
    let queue_before = std::fs::read_to_string(&resolved.queue_path)?;
    let done_before = std::fs::read_to_string(&resolved.done_path)?;
    let productivity_before = std::fs::read_to_string(&productivity_path)?;

    Ok((
        resolved,
        productivity_path,
        queue_before,
        done_before,
        productivity_before,
    ))
}

fn configure_tracking_remote(repo_root: &Path) -> anyhow::Result<TempDir> {
    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        repo_root,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(repo_root, &["push", "-u", "origin", &branch])?;
    Ok(remote)
}

fn upstream_counts(repo_root: &Path) -> anyhow::Result<(u32, u32)> {
    let counts = git_test::git_output(
        repo_root,
        &["rev-list", "--left-right", "--count", "@{u}...HEAD"],
    )?;
    let mut parts = counts.split_whitespace();
    let behind = parts
        .next()
        .expect("behind count")
        .parse()
        .expect("numeric behind count");
    let ahead = parts
        .next()
        .expect("ahead count")
        .parse()
        .expect("numeric ahead count");
    Ok((behind, ahead))
}

fn head_commit_subject(repo_root: &Path) -> anyhow::Result<String> {
    git_test::git_output(repo_root, &["log", "-1", "--pretty=%s"])
}

#[test]
fn post_run_parallel_worker_restores_bookkeeping_without_signals() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    let (resolved, productivity_path, queue_before, done_before, productivity_before) =
        init_parallel_worker_repo(repo_root)?;

    std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
    std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
    std::fs::write(&productivity_path, r#"{"stats":["changed"]}"#)?;

    post_run_supervise_parallel_worker(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        false,
        None,
    )?;

    assert_eq!(std::fs::read_to_string(&resolved.queue_path)?, queue_before);
    assert_eq!(std::fs::read_to_string(&resolved.done_path)?, done_before);
    assert_eq!(
        std::fs::read_to_string(&productivity_path)?,
        productivity_before
    );

    let status_paths = crate::git::status_paths(repo_root)?;
    let queue_rel = resolved
        .queue_path
        .strip_prefix(repo_root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let done_rel = resolved
        .done_path
        .strip_prefix(repo_root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let productivity_rel = productivity_path
        .strip_prefix(repo_root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    assert!(
        !status_paths.contains(&queue_rel),
        "queue.jsonc should be restored to HEAD"
    );
    assert!(
        !status_paths.contains(&done_rel),
        "done.jsonc should be restored to HEAD"
    );
    assert!(
        !status_paths.contains(&productivity_rel),
        "productivity.json should be restored to HEAD"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn post_run_parallel_worker_errors_when_bookkeeping_restore_fails() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    let (resolved, productivity_path, _queue_before, _done_before, _productivity_before) =
        init_parallel_worker_repo(repo_root)?;

    std::fs::write(&productivity_path, r#"{"stats":["changed"]}"#)?;

    let cache_dir = productivity_path
        .parent()
        .expect("productivity parent")
        .to_path_buf();
    let original_dir_mode = std::fs::metadata(&cache_dir)?.permissions().mode();
    let original_file_mode = std::fs::metadata(&productivity_path)?.permissions().mode();

    std::fs::set_permissions(&productivity_path, std::fs::Permissions::from_mode(0o444))?;
    std::fs::set_permissions(&cache_dir, std::fs::Permissions::from_mode(0o555))?;

    let result = post_run_supervise_parallel_worker(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        false,
        None,
    );

    std::fs::set_permissions(
        &cache_dir,
        std::fs::Permissions::from_mode(original_dir_mode),
    )?;
    if productivity_path.exists() {
        std::fs::set_permissions(
            &productivity_path,
            std::fs::Permissions::from_mode(original_file_mode),
        )?;
    }

    let err = result.expect_err("expected restore failure");
    assert!(
        format!("{err:#}").contains("restore queue/done/productivity to HEAD"),
        "unexpected error: {err:#}"
    );
    Ok(())
}

#[test]
fn post_run_parallel_worker_writes_ci_failure_marker_on_ci_error() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    let (mut resolved, _productivity_path, _queue_before, _done_before, _productivity_before) =
        init_parallel_worker_repo(repo_root)?;

    std::fs::write(repo_root.join("work.txt"), "dirty worker change\n")?;
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            "import sys; sys.stderr.write('parallel worker CI failed\\n'); raise SystemExit(2)"
                .to_string(),
        ]),
    });

    let err = post_run_supervise_parallel_worker(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        false,
        None,
    )
    .expect_err("expected CI failure");

    assert!(format!("{err:#}").contains("CI gate failed"));

    let marker_path = repo_root.join(crate::commands::run::parallel::CI_FAILURE_MARKER_FILE);
    let raw = std::fs::read_to_string(&marker_path)?;
    let payload: serde_json::Value = serde_json::from_str(&raw)?;
    assert_eq!(payload["task_id"], "RQ-0001");
    assert!(
        payload["error"]
            .as_str()
            .expect("error string")
            .contains("CI gate failed")
    );

    let status = git_test::git_output(repo_root, &["status", "--porcelain"])?;
    assert!(
        status.contains("work.txt"),
        "expected dirty worker change: {status}"
    );
    Ok(())
}

#[test]
fn post_run_parallel_worker_commit_and_push_restores_bookkeeping_and_publishes_real_changes()
-> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    let (resolved, productivity_path, queue_before, done_before, productivity_before) =
        init_parallel_worker_repo(repo_root)?;
    let _remote = configure_tracking_remote(repo_root)?;

    std::fs::write(repo_root.join("work.txt"), "worker implementation\n")?;
    std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
    std::fs::write(
        &resolved.done_path,
        r#"{"version":1,"tasks":[{"id":"RQ-0001"}]}"#,
    )?;
    std::fs::write(&productivity_path, r#"{"stats":["changed"]}"#)?;

    post_run_supervise_parallel_worker(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::CommitAndPush,
        PushPolicy::RequireUpstream,
        None,
        None,
        false,
        None,
    )?;

    assert_eq!(std::fs::read_to_string(&resolved.queue_path)?, queue_before);
    assert_eq!(std::fs::read_to_string(&resolved.done_path)?, done_before);
    assert_eq!(
        std::fs::read_to_string(&productivity_path)?,
        productivity_before
    );
    assert_eq!(upstream_counts(repo_root)?, (0, 0));
    assert_eq!(head_commit_subject(repo_root)?, "RQ-0001: Test task");

    let status = git_test::git_output(repo_root, &["status", "--porcelain"])?;
    assert!(status.trim().is_empty(), "expected clean repo: {status}");
    assert_eq!(
        std::fs::read_to_string(repo_root.join("work.txt"))?,
        "worker implementation\n"
    );

    Ok(())
}
