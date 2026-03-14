//! CI-gate and queue-maintenance-focused post-run supervision scenarios.
//!
//! Responsibilities:
//! - Validate CI enforcement boundaries and maintenance-only queue repair behavior.
//!
//! Not handled here:
//! - Upstream push semantics once git finalization succeeds.
//! - Dirty-repo archival behavior unrelated to CI/maintenance triggers.

use super::super::support::{make_task, resolved_for_repo, write_done_tasks, write_queue};
use crate::commands::run::supervision::{PushPolicy, post_run_supervise};
use crate::contracts::{CiGateConfig, GitPublishMode, GitRevertMode, TaskStatus};
use crate::queue;
use crate::testsupport::git as git_test;
use tempfile::TempDir;

fn archived_done_task_with_non_utc_timestamps() -> crate::contracts::Task {
    let mut task = make_task("RQ-0001", "Archived task", TaskStatus::Done);
    task.created_at = Some("2026-01-18T00:00:00-07:00".to_string());
    task.updated_at = Some("2026-01-18T00:00:00-07:00".to_string());
    task.completed_at = Some("2026-01-18T00:05:00-07:00".to_string());
    task
}

#[test]
fn post_run_supervise_runs_ci_for_clean_repo_when_queue_mutation_is_pending() -> anyhow::Result<()>
{
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(CiGateConfig {
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
        GitPublishMode::Off,
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
    write_done_tasks(
        temp.path(),
        vec![archived_done_task_with_non_utc_timestamps()],
    )?;
    queue::save_queue(
        &temp.path().join(".ralph/queue.jsonc"),
        &crate::contracts::QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    git_test::commit_all(temp.path(), "init")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(CiGateConfig {
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
        GitPublishMode::Off,
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
fn post_run_supervise_skips_ci_for_clean_already_archived_done_noop() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_done_tasks(
        temp.path(),
        vec![make_task("RQ-0001", "Archived task", TaskStatus::Done)],
    )?;
    queue::save_queue(
        &temp.path().join(".ralph/queue.jsonc"),
        &crate::contracts::QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    git_test::commit_all(temp.path(), "init")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            "import sys; sys.stderr.write('ci should have been skipped\\n'); raise SystemExit(2)"
                .to_string(),
        ]),
    });

    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    anyhow::ensure!(
        queue_file.tasks.is_empty(),
        "expected queue to remain empty"
    );
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|task| task.id == "RQ-0001"),
        "expected archived done entry to remain intact"
    );
    Ok(())
}

#[test]
fn post_run_supervise_successful_maintenance_repair_publish_off_leaves_dirty_repo()
-> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_done_tasks(
        temp.path(),
        vec![archived_done_task_with_non_utc_timestamps()],
    )?;
    queue::save_queue(
        &temp.path().join(".ralph/queue.jsonc"),
        &crate::contracts::QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    git_test::commit_all(temp.path(), "init")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        GitPublishMode::Off,
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
    let repaired = done_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("expected archived task");
    let completed_at = repaired
        .completed_at
        .as_deref()
        .expect("completed_at should remain populated");
    anyhow::ensure!(completed_at.ends_with('Z'));
    anyhow::ensure!(!completed_at.contains("-07:00"));
    crate::timeutil::parse_rfc3339(completed_at)?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(
        !status.trim().is_empty(),
        "expected maintenance-only repair to leave repo dirty when publish mode is off"
    );
    Ok(())
}
