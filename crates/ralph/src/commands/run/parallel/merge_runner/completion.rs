//! Completion signal handling for merge runner.
//!
//! Responsibilities:
//! - Apply completion signals on the base branch after merge.
//! - Collect queue/done/productivity bytes for downstream sync.
//!
//! Not handled here:
//! - Merge execution (see `mod.rs`).
//! - Conflict resolution (see `conflict.rs`).

use crate::commands::run::PhaseType;
use crate::commands::run::parallel::merge_runner::git_ops::push_branch;
use crate::commands::run::parallel::path_map::map_resolved_path_into_workspace;
use crate::contracts::{MergeRunnerConfig, RunnerCliOptionsPatch};
use crate::{completions, git, outpututil, productivity, queue, runner};
use anyhow::{Context, Result, bail};
use std::path::Path;

use super::BaseSyncWorkspaceCleanupGuard;

/// Container for queue/done/productivity bytes collected during completion.
#[derive(Debug)]
pub(crate) struct QueueSyncBytes {
    pub queue_bytes: Vec<u8>,
    pub done_bytes: Option<Vec<u8>>,
    pub productivity_bytes: Option<Vec<u8>>,
}

/// Apply completion signal on base branch and collect resulting bytes.
pub(crate) fn apply_completion_and_collect_bytes(
    resolved: &crate::config::Resolved,
    workspace_root: &Path,
    workspace_path: Option<&Path>,
    base_branch: &str,
    task_id: &str,
) -> Result<QueueSyncBytes> {
    let base_sync_path = workspace_root.join(".base-sync");
    let _base_sync_cleanup = BaseSyncWorkspaceCleanupGuard::new(workspace_root, &base_sync_path);

    git::ensure_workspace_exists(&resolved.repo_root, &base_sync_path, base_branch)
        .with_context(|| format!("ensure base-sync workspace at {}", base_sync_path.display()))?;

    let workspace_queue_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        &base_sync_path,
        &resolved.queue_path,
        "queue",
    )?;
    let workspace_done_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        &base_sync_path,
        &resolved.done_path,
        "done",
    )?;

    let mut workspace_resolved = resolved.clone();
    workspace_resolved.repo_root = base_sync_path.clone();
    workspace_resolved.queue_path = workspace_queue_path.clone();
    workspace_resolved.done_path = workspace_done_path.clone();
    if workspace_resolved.project_config_path.is_some() {
        workspace_resolved.project_config_path = Some(base_sync_path.join(".ralph/config.json"));
    }

    ensure_completion_signal_in_workspace(
        &base_sync_path,
        workspace_root,
        workspace_path,
        task_id,
    )?;
    let applied =
        crate::commands::run::apply_phase3_completion_signal(&workspace_resolved, task_id)?;
    if applied.is_none() {
        bail!(
            "apply_phase3_completion_signal returned None for {} despite ensure succeeding; this is an unexpected state.",
            task_id
        );
    }

    let task_title =
        task_title_from_queue_done_paths(&workspace_queue_path, &workspace_done_path, task_id)?
            .unwrap_or_else(|| "Parallel completion".to_string());

    let cache_dir = workspace_resolved.repo_root.join(".ralph").join("cache");
    if let Err(err) = productivity::record_task_completion_by_id(task_id, &task_title, &cache_dir) {
        log::debug!(
            "Failed to record productivity for {} in base-sync workspace: {}",
            task_id,
            err
        );
    }

    let status = git::status_porcelain(&base_sync_path)?;
    if !status.trim().is_empty() {
        let message = outpututil::format_task_commit_message(task_id, &task_title);
        match git::commit_all(&base_sync_path, &message) {
            Ok(()) => {
                push_branch(&base_sync_path)?;
            }
            Err(git::GitError::NoChangesToCommit) => {}
            Err(err) => return Err(err.into()),
        }
    }

    let queue_bytes = std::fs::read(&workspace_queue_path)
        .with_context(|| format!("read queue bytes from {}", workspace_queue_path.display()))?;
    let done_bytes =
        if workspace_done_path.exists() {
            Some(std::fs::read(&workspace_done_path).with_context(|| {
                format!("read done bytes from {}", workspace_done_path.display())
            })?)
        } else {
            None
        };
    let productivity_path = base_sync_path
        .join(".ralph")
        .join("cache")
        .join("productivity.json");
    let productivity_bytes = if productivity_path.exists() {
        Some(std::fs::read(&productivity_path).with_context(|| {
            format!(
                "read productivity bytes from {}",
                productivity_path.display()
            )
        })?)
    } else {
        None
    };

    Ok(QueueSyncBytes {
        queue_bytes,
        done_bytes,
        productivity_bytes,
    })
}

/// Ensures a completion signal exists for the task, copying it into base-sync when found.
///
/// Behavior:
/// 1. If signal exists in base_sync_root, return Ok(())
/// 2. Else check the explicit workspace_path (if provided) and the default
///    `{workspace_root}/{task_id}` path; copy the signal into base-sync if found.
/// 3. Else auto-finalize with a minimal completion signal.
fn ensure_completion_signal_in_workspace(
    base_sync_root: &Path,
    workspace_root: &Path,
    workspace_path: Option<&Path>,
    task_id: &str,
) -> Result<()> {
    if completions::read_completion_signal(base_sync_root, task_id)?.is_some() {
        return Ok(());
    }

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(path) = workspace_path {
        candidates.push(path.to_path_buf());
    }
    let default_path = workspace_root.join(task_id.trim());
    if workspace_path
        .map(|path| path != default_path)
        .unwrap_or(true)
    {
        candidates.push(default_path);
    }

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        if let Some(signal) = completions::read_completion_signal(&candidate, task_id)? {
            completions::write_completion_signal(base_sync_root, &signal)?;
            log::info!(
                "Copied completion signal for {} from workspace {} into base-sync.",
                task_id,
                candidate.display()
            );
            return Ok(());
        }
    }

    let fallback = completions::CompletionSignal {
        task_id: task_id.to_string(),
        status: crate::contracts::TaskStatus::Done,
        notes: vec![format!(
            "[ralph] Auto-finalized after merge: completion signal missing for {}.",
            task_id
        )],
        runner_used: None,
        model_used: None,
    };
    completions::write_completion_signal(base_sync_root, &fallback)?;
    log::warn!(
        "Completion signal missing for {}; auto-finalizing as done.",
        task_id
    );
    Ok(())
}

/// Extract task title from queue/done files for a given task_id.
fn task_title_from_queue_done_paths(
    queue_path: &Path,
    done_path: &Path,
    task_id: &str,
) -> Result<Option<String>> {
    let queue_file = queue::load_queue(queue_path)?;
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    let done_file = queue::load_queue_or_default(done_path)?;
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    Ok(None)
}

/// Run the merge runner prompt to resolve conflicts.
pub(crate) fn run_merge_runner_prompt(
    resolved: &crate::config::Resolved,
    merge_runner: &MergeRunnerConfig,
    workspace_path: &Path,
    prompt: &str,
) -> Result<()> {
    let settings = runner::resolve_agent_settings(
        merge_runner.runner.clone(),
        merge_runner.model.clone(),
        merge_runner.reasoning_effort,
        &RunnerCliOptionsPatch::default(),
        None,
        &resolved.config.agent,
    )?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    runner::run_prompt(
        settings.runner.clone(),
        workspace_path,
        bins,
        settings.model.clone(),
        settings.reasoning_effort,
        settings.runner_cli,
        prompt,
        None,
        resolved.config.agent.claude_permission_mode,
        None,
        runner::OutputStream::Terminal,
        PhaseType::Implementation,
        None,
        None,
    )
    .map_err(|err| anyhow::anyhow!("Merge runner failed: {:#}", err))?;

    Ok(())
}
