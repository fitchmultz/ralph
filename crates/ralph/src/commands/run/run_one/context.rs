//! Run-one context preparation.
//!
//! Purpose:
//! - Run-one context preparation.
//!
//! Responsibilities:
//! - Prepare the context for run-one execution (lock, queue, config).
//! - Keep acquired queue locks alive for the full run-one critical section.
//! - Handle Ctrl+C state initialization and pre-run interrupt detection.
//! - Load and validate queues.
//! - Resolve git and prompt policy configuration.
//!
//! Not handled here:
//! - Task execution setup (see execution_setup.rs).
//! - Phase execution (see phase_execution.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Callers pass the correct `QueueLockMode` for their context.

use super::QueueLockMode;
use super::orchestration::RunOneContext;
use crate::agent::AgentOverrides;
use crate::commands::run::{phases::PostRunMode, supervision::PushPolicy};
use crate::config;
use crate::contracts::{GitPublishMode, GitRevertMode};
use crate::{promptflow, queue};
use anyhow::Result;

/// Prepare the context for run-one execution.
///
/// Handles Ctrl+C state initialization and pre-run interrupt detection,
/// lock acquisition, queue loading/validation, and configuration resolution.
pub(crate) fn prepare_run_one_context(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    lock_mode: QueueLockMode,
    parallel_target_branch: Option<&str>,
) -> Result<RunOneContext> {
    // Handle Ctrl+C state initialization and pre-run interrupt detection.
    let ctrlc = crate::runner::ctrlc_state()
        .map_err(|e| anyhow::anyhow!("Ctrl-C handler initialization failed: {}", e))?;

    if ctrlc.interrupted.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(crate::runutil::RunAbort::new(
            crate::runutil::RunAbortReason::Interrupted,
            "Ctrl+C was pressed before task execution started",
        )
        .into());
    }

    ctrlc
        .interrupted
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let queue_lock = match lock_mode {
        QueueLockMode::Acquire | QueueLockMode::AcquireAllowUpstream => Some(
            queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?,
        ),
        QueueLockMode::Held => None,
    };

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        Some(&done),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(agent_overrides, resolved);

    let git_revert_mode = agent_overrides
        .git_revert_mode
        .or(resolved.config.agent.git_revert_mode)
        .unwrap_or(GitRevertMode::Ask);

    let git_publish_mode = agent_overrides
        .git_publish_mode
        .or_else(|| resolved.config.agent.effective_git_publish_mode())
        .unwrap_or(GitPublishMode::Off);

    let push_policy = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PushPolicy::AllowCreateUpstream,
        QueueLockMode::Acquire | QueueLockMode::Held => PushPolicy::RequireUpstream,
    };

    let post_run_mode = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => PostRunMode::ParallelWorker,
        QueueLockMode::Acquire | QueueLockMode::Held => PostRunMode::Normal,
    };

    let parallel_target_branch = match lock_mode {
        QueueLockMode::AcquireAllowUpstream => {
            let branch = parallel_target_branch
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "parallel worker requires explicit target branch (--parallel-target-branch)"
                    )
                })?;
            Some(branch.to_string())
        }
        QueueLockMode::Acquire | QueueLockMode::Held => None,
    };

    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: repoprompt_flags.plan_required,
        repoprompt_tool_injection: repoprompt_flags.tool_injection,
    };

    Ok(RunOneContext {
        _queue_lock: queue_lock,
        queue_file,
        done,
        git_revert_mode,
        git_publish_mode,
        push_policy,
        post_run_mode,
        parallel_target_branch,
        policy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, QueueFile, Task, TaskStatus};
    use crate::testsupport::{INTERRUPT_TEST_MUTEX, reset_ctrlc_interrupt_flag};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn resolved_with_repo_root(repo_root: PathBuf) -> config::Resolved {
        let mut cfg = Config::default();
        cfg.queue.file = Some(PathBuf::from(".ralph/queue.json"));
        cfg.queue.done_file = Some(PathBuf::from(".ralph/done.json"));
        cfg.agent.notification.enabled = Some(false);

        config::Resolved {
            config: cfg,
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
        }
    }

    fn task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["rust".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            estimated_minutes: None,
            actual_minutes: None,
            parent_id: None,
        }
    }

    fn write_queues(resolved: &config::Resolved) -> anyhow::Result<()> {
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        queue::save_queue(
            &resolved.queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task()],
            },
        )?;
        queue::save_queue(&resolved.done_path, &QueueFile::default())?;
        Ok(())
    }

    #[test]
    fn acquired_queue_lock_lives_until_run_one_context_drop() -> anyhow::Result<()> {
        let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
        let _interrupt_guard = interrupt_mutex.lock().unwrap();
        reset_ctrlc_interrupt_flag();

        let temp = tempfile::TempDir::new()?;
        let resolved = resolved_with_repo_root(temp.path().to_path_buf());
        write_queues(&resolved)?;

        let ctx = prepare_run_one_context(
            &resolved,
            &AgentOverrides::default(),
            false,
            QueueLockMode::Acquire,
            None,
        )?;

        let err = queue::acquire_queue_lock(&resolved.repo_root, "contender", false)
            .expect_err("expected context-owned queue lock to block contenders");
        assert!(
            crate::commands::run::queue_lock::is_queue_lock_already_held_error(&err),
            "expected queue-lock contention while context is alive, got: {err:#}"
        );

        drop(ctx);

        let _lock_after_drop =
            queue::acquire_queue_lock(&resolved.repo_root, "after context drop", false)?;

        Ok(())
    }
}
