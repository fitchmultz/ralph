//! Revert-mode-focused post-run supervision scenarios.
//!
//! Purpose:
//! - Revert-mode-focused post-run supervision scenarios.
//!
//! Responsibilities:
//! - Validate CI-failure revert behavior before queue mutation occurs.
//! - Exercise runtime prompt handling when supervision asks whether to keep dirty changes.
//!
//! Not handled here:
//! - Queue-op defensive inconsistency helpers (covered in `queue_ops.rs` unit tests).
//! - Publish-mode success paths once supervision completes.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::super::support::{resolved_for_repo, write_queue};
use crate::commands::run::supervision::{PushPolicy, post_run_supervise};
use crate::contracts::{CiGateConfig, GitPublishMode, GitRevertMode, TaskStatus};
use crate::queue;
use crate::runutil::{RevertDecision, RevertPromptHandler};
use crate::testsupport::git as git_test;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[test]
fn post_run_supervise_ci_failure_enabled_reverts_dirty_repo_before_queue_mutation()
-> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "dirty before ci\n")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            "import sys; sys.stderr.write('worker ci failed\\n'); raise SystemExit(2)".to_string(),
        ]),
    });

    let err = post_run_supervise(
        &resolved,
        None,
        "RQ-0001",
        GitRevertMode::Enabled,
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
    .expect_err("expected CI failure");

    let message = format!("{err:#}");
    assert!(message.contains("CI gate failed"));
    assert!(message.contains("Uncommitted changes were reverted."));

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    assert!(
        status.trim().is_empty(),
        "expected clean worktree: {status}"
    );
    assert!(!temp.path().join("work.txt").exists());

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
fn post_run_supervise_ci_failure_ask_uses_prompt_handler_and_keeps_dirty_repo() -> anyhow::Result<()>
{
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "dirty before ci\n")?;

    let mut resolved = resolved_for_repo(temp.path());
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            "import sys; sys.stderr.write('worker ci failed\\n'); raise SystemExit(2)".to_string(),
        ]),
    });

    let seen_labels = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_labels_for_prompt = Arc::clone(&seen_labels);
    let prompt: RevertPromptHandler = Arc::new(move |context| {
        seen_labels_for_prompt
            .lock()
            .expect("prompt label mutex")
            .push(context.label.clone());
        Ok(RevertDecision::Keep)
    });

    let err = post_run_supervise(
        &resolved,
        None,
        "RQ-0001",
        GitRevertMode::Ask,
        GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        Some(prompt),
        None,
        None,
        None,
        false,
        false,
        None,
    )
    .expect_err("expected CI failure");

    let message = format!("{err:#}");
    assert!(message.contains("CI gate failed"));
    assert!(message.contains("Revert skipped (user chose to keep changes)"));

    let labels = seen_labels.lock().expect("prompt label mutex");
    assert_eq!(labels.as_slice(), ["CI gate failure"]);

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    assert!(
        status.contains("work.txt"),
        "expected dirty worktree: {status}"
    );
    assert!(temp.path().join("work.txt").exists());

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let task = queue_file
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("task should remain in queue after CI failure");
    assert_eq!(task.status, TaskStatus::Todo);

    Ok(())
}
