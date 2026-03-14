//! Post-run supervision coverage for queue, archive, and git finalization behavior.
//!
//! Responsibilities:
//! - Validate task archival, commit/push behavior, and productivity-file handling.
//! - Keep dirty-vs-clean supervision regressions localized to post-run flows.
//!
//! Not handled here:
//! - Continue-session resume fallback logic.
//! - Parallel-worker bookkeeping restore.
//!
//! Invariants/assumptions:
//! - Tests run against disposable git repositories.
//! - Queue fixtures always archive `RQ-0001`.

use super::support::{resolved_for_repo, write_queue};
use crate::commands::run::supervision::{PushPolicy, post_run_supervise};
use crate::contracts::GitRevertMode;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::testsupport::git as git_test;
use tempfile::TempDir;

#[test]
fn post_run_supervise_commits_and_cleans_when_enabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::CommitAndPush,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(status.trim().is_empty(), "expected clean repo");

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|task| task.id == "RQ-0001"),
        "expected task in done archive"
    );
    Ok(())
}

#[test]
fn post_run_supervise_skips_commit_when_disabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(!status.trim().is_empty(), "expected dirty repo");
    Ok(())
}

#[test]
fn post_run_supervise_runs_ci_for_clean_repo_when_queue_mutation_is_pending() -> anyhow::Result<()>
{
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            "import sys; print('CI failing'); sys.stderr.write('clean repo failure\\n'); raise SystemExit(2)"
                .to_string(),
        ]),
    });

    let err = post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )
    .expect_err("expected CI failure before queue mutation");
    assert!(format!("{err:#}").contains("CI gate failed"));

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let task = queue_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("task should remain in queue after CI failure");
    assert_eq!(task.status, TaskStatus::Todo);
    assert!(
        queue::load_queue_or_default(&resolved.done_path)?
            .tasks
            .is_empty()
    );

    Ok(())
}

#[test]
fn post_run_supervise_runs_ci_after_queue_maintenance_dirties_repo() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;

    queue::save_queue(
        &temp.path().join(".ralph/queue.jsonc"),
        &crate::contracts::QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    queue::save_queue(
        &temp.path().join(".ralph/done.jsonc"),
        &crate::contracts::QueueFile {
            version: 1,
            tasks: vec![crate::contracts::Task {
                id: "RQ-0001".to_string(),
                status: TaskStatus::Done,
                title: "Archived task".to_string(),
                description: None,
                priority: crate::contracts::TaskPriority::Medium,
                tags: vec![],
                scope: vec![],
                evidence: vec![],
                plan: vec![],
                notes: vec![],
                request: None,
                agent: None,
                created_at: Some("2026-01-18T00:00:00-07:00".to_string()),
                updated_at: Some("2026-01-18T00:00:00-07:00".to_string()),
                completed_at: Some("2026-01-18T00:05:00-07:00".to_string()),
                started_at: None,
                scheduled_start: None,
                depends_on: vec![],
                blocks: vec![],
                relates_to: vec![],
                duplicates: None,
                custom_fields: std::collections::HashMap::new(),
                estimated_minutes: None,
                actual_minutes: None,
                parent_id: None,
            }],
        },
    )?;
    git_test::commit_all(temp.path(), "init")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            "import sys; sys.stderr.write('maintenance repair failure\\n'); raise SystemExit(2)"
                .to_string(),
        ]),
    });

    let err = post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )
    .expect_err("expected CI failure after maintenance dirtied the repo");
    assert!(format!("{err:#}").contains("CI gate failed"));

    Ok(())
}

#[test]
fn post_run_supervise_backfills_missing_completed_at() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Done)?;
    git_test::commit_all(temp.path(), "init")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let task = done_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("expected task in done archive");
    let completed_at = task
        .completed_at
        .as_deref()
        .expect("completed_at should be stamped");

    crate::timeutil::parse_rfc3339(completed_at)?;
    Ok(())
}

#[test]
fn post_run_supervise_errors_on_push_failure_when_enabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
    let missing_remote = temp.path().join("missing-remote");
    git_test::git_run(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote.to_str().unwrap(),
        ],
    )?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    let err = post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::CommitAndPush,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )
    .expect_err("expected push failure");
    assert!(format!("{err:#}").contains("Git push failed"));
    Ok(())
}

#[test]
fn post_run_supervise_skips_push_when_disabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
    let missing_remote = temp.path().join("missing-remote");
    git_test::git_run(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote.to_str().unwrap(),
        ],
    )?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;
    Ok(())
}

#[test]
fn post_run_supervise_allows_productivity_json_dirty() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Done)?;
    git_test::commit_all(temp.path(), "init")?;

    let cache_dir = temp.path().join(".ralph").join("cache");
    std::fs::create_dir_all(&cache_dir)?;
    std::fs::write(
        cache_dir.join("productivity.json"),
        r#"{"version":1,"total_completed":1}"#,
    )?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        crate::contracts::GitPublishMode::CommitAndPush,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|task| task.id == "RQ-0001"),
        "expected task in done archive"
    );

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(
        status.trim().is_empty(),
        "expected clean repo after commit, but found: {status}"
    );
    Ok(())
}
