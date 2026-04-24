//! Machine-owned parallel integration bookkeeping.
//!
//! Purpose:
//! - Rebuild shared queue/done files from the latest target branch during worker integration.
//!
//! Responsibilities:
//! - Rebase the worker workspace onto the current target branch.
//! - Restore queue/done files from `origin/<target>` before applying the current task archive.
//! - Commit the deterministic bookkeeping state and push the worker branch.
//!
//! Scope:
//! - Parallel-worker integration only.
//! - Human conflict resolution remains in the agent continuation loop.
//!
//! Usage:
//! - Called by `driver.rs` after the agent has completed implementation integration work.
//!
//! Invariants/Assumptions:
//! - Resolved queue/done paths are workspace-local paths under `resolved.repo_root`.
//! - Queue/done state in the target branch is the shared source of truth.

use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::config::Resolved;
use crate::contracts::{QueueFile, TaskStatus};
use crate::git::error::git_output;
use crate::git::{self, GitError};
use crate::{outpututil, queue, timeutil};

use super::compliance::{ComplianceResult, run_compliance_checks};
use super::types::IntegrationConfig;

const MACHINE_PUSH_RACE_RETRIES: u32 = 5;

#[derive(Debug, Clone)]
pub(crate) struct MachineIntegrationAttempt {
    pub(crate) compliance: ComplianceResult,
    pub(crate) pushed: bool,
    pub(crate) push_error: Option<String>,
}

pub(crate) fn finalize_bookkeeping_and_push(
    resolved: &Resolved,
    task_id: &str,
    config: &IntegrationConfig,
) -> Result<MachineIntegrationAttempt> {
    let repo_root = resolved.repo_root.as_path();
    let mut last_push_error = None;

    for attempt in 1..=MACHINE_PUSH_RACE_RETRIES {
        if let Err(err) = rebase_on_latest_target(repo_root, &config.target_branch) {
            return failed_attempt(
                repo_root,
                resolved,
                task_id,
                format!("machine rebase failed: {err:#}"),
            );
        }

        let followup_report =
            match rebuild_bookkeeping_from_target(resolved, task_id, &config.target_branch) {
                Ok(report) => report,
                Err(err) => {
                    return failed_attempt(
                        repo_root,
                        resolved,
                        task_id,
                        format!("machine bookkeeping reconciliation failed: {err:#}"),
                    );
                }
            };

        if let Err(err) = commit_pending_integration_changes(repo_root, task_id) {
            return failed_attempt(
                repo_root,
                resolved,
                task_id,
                format!("machine integration commit failed: {err:#}"),
            );
        }

        let compliance = run_compliance_checks(repo_root, resolved, task_id, config.ci_enabled)?;
        if !compliance.all_passed() {
            return Ok(MachineIntegrationAttempt {
                compliance,
                pushed: false,
                push_error: None,
            });
        }

        match git::push_head_to_branch(repo_root, "origin", &config.target_branch) {
            Ok(()) => {
                let cleanup_error = if followup_report.is_some() {
                    remove_applied_followup_proposal(resolved, task_id)
                        .err()
                        .map(|err| format!("machine follow-up proposal cleanup failed: {err:#}"))
                } else {
                    None
                };
                return Ok(MachineIntegrationAttempt {
                    compliance,
                    pushed: true,
                    push_error: cleanup_error,
                });
            }
            Err(err) => {
                let message = format!("machine push failed on attempt {attempt}: {err:#}");
                if is_retryable_push_race(&err) && attempt < MACHINE_PUSH_RACE_RETRIES {
                    log::info!("{message}; retrying after refreshing target branch");
                    last_push_error = Some(message);
                    continue;
                }

                return Ok(MachineIntegrationAttempt {
                    compliance,
                    pushed: false,
                    push_error: Some(message),
                });
            }
        }
    }

    let compliance = run_compliance_checks(repo_root, resolved, task_id, false)?;
    Ok(MachineIntegrationAttempt {
        compliance,
        pushed: false,
        push_error: Some(last_push_error.unwrap_or_else(|| {
            format!("machine push exhausted {MACHINE_PUSH_RACE_RETRIES} retry attempts")
        })),
    })
}

fn failed_attempt(
    repo_root: &Path,
    resolved: &Resolved,
    task_id: &str,
    reason: String,
) -> Result<MachineIntegrationAttempt> {
    let compliance = run_compliance_checks(repo_root, resolved, task_id, false)?;
    Ok(MachineIntegrationAttempt {
        compliance,
        pushed: false,
        push_error: Some(reason),
    })
}

fn rebase_on_latest_target(repo_root: &Path, target_branch: &str) -> Result<()> {
    let remote_ref = format!("origin/{}", target_branch.trim());
    git::fetch_branch(repo_root, "origin", target_branch)
        .with_context(|| format!("fetch origin/{target_branch} before machine integration"))?;
    git::rebase_onto(repo_root, &remote_ref)
        .with_context(|| format!("rebase worker workspace onto {remote_ref}"))?;
    Ok(())
}

pub(super) fn rebuild_bookkeeping_from_target(
    resolved: &Resolved,
    task_id: &str,
    target_branch: &str,
) -> Result<Option<queue::FollowupApplyReport>> {
    let repo_root = resolved.repo_root.as_path();
    let remote_ref = format!("origin/{}", target_branch.trim());

    restore_bookkeeping_path_from_ref(repo_root, &remote_ref, &resolved.queue_path, "queue")?;
    restore_bookkeeping_path_from_ref(repo_root, &remote_ref, &resolved.done_path, "done")?;

    archive_current_task(resolved, task_id)?;
    let report = queue::apply_default_followups_if_present_with_removal(resolved, task_id, false)
        .context("apply parallel worker follow-up proposal")?;
    if let Some(report) = &report {
        log::info!(
            "applied {} follow-up task(s) proposed by {} during machine integration",
            report.created_tasks.len(),
            task_id
        );
    }
    Ok(report)
}

fn remove_applied_followup_proposal(resolved: &Resolved, task_id: &str) -> Result<()> {
    queue::remove_default_followups_proposal_if_present(&resolved.repo_root, task_id)
        .context("remove applied parallel worker follow-up proposal")
}

fn restore_bookkeeping_path_from_ref(
    repo_root: &Path,
    remote_ref: &str,
    path: &Path,
    label: &str,
) -> Result<()> {
    let rel = repo_relative_path(repo_root, path, label)?;
    if !path_exists_in_ref(repo_root, remote_ref, &rel)? {
        log::debug!(
            "{} bookkeeping path {} is not tracked in {}; leaving workspace copy intact",
            label,
            rel,
            remote_ref
        );
        return Ok(());
    }

    let output = git_output(repo_root, &["checkout", remote_ref, "--", &rel])
        .with_context(|| format!("restore {label} bookkeeping from {remote_ref}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "restore {} bookkeeping path {} from {} failed: {}",
        label,
        rel,
        remote_ref,
        stderr.trim()
    );
}

fn archive_current_task(resolved: &Resolved, task_id: &str) -> Result<()> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        bail!("task id is empty");
    }

    let mut active = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("load queue {}", resolved.queue_path.display()))?;
    let mut done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("load done {}", resolved.done_path.display()))?;

    match locate_task(&active, &done, task_id) {
        TaskLocation::Active => {
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(&mut active, task_id, TaskStatus::Done, &now, None)?;
            queue::archive_terminal_tasks_in_memory(&mut active, &mut done, &now)?;
        }
        TaskLocation::Done => {
            ensure_done_task_is_terminal(&done, task_id)?;
            active.tasks.retain(|task| task.id.trim() != task_id);
        }
        TaskLocation::Missing => {
            bail!(
                "{}",
                crate::error_messages::task_not_found_in_queue_or_done(task_id)
            );
        }
    }

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &active,
        Some(&done),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate reconciled queue/done state")?;

    queue::save_queue(&resolved.done_path, &done)
        .with_context(|| format!("save done {}", resolved.done_path.display()))?;
    queue::save_queue(&resolved.queue_path, &active)
        .with_context(|| format!("save queue {}", resolved.queue_path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskLocation {
    Active,
    Done,
    Missing,
}

fn locate_task(active: &QueueFile, done: &QueueFile, task_id: &str) -> TaskLocation {
    if done.tasks.iter().any(|task| task.id.trim() == task_id) {
        return TaskLocation::Done;
    }
    if active.tasks.iter().any(|task| task.id.trim() == task_id) {
        return TaskLocation::Active;
    }
    TaskLocation::Missing
}

fn ensure_done_task_is_terminal(done: &QueueFile, task_id: &str) -> Result<()> {
    let task = done
        .tasks
        .iter()
        .find(|task| task.id.trim() == task_id)
        .ok_or_else(|| anyhow::anyhow!("task {} missing from done queue", task_id))?;

    if task.status != TaskStatus::Done {
        bail!(
            "task {} exists in done but status is {:?}, expected done",
            task_id,
            task.status
        );
    }

    Ok(())
}

fn commit_pending_integration_changes(repo_root: &Path, task_id: &str) -> Result<()> {
    let commit_message = format_machine_bookkeeping_commit_message(task_id);
    match git::commit_all(repo_root, &commit_message) {
        Ok(()) => Ok(()),
        Err(GitError::NoChangesToCommit) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn format_machine_bookkeeping_commit_message(task_id: &str) -> String {
    let raw = format!("ralph: archive {} queue bookkeeping", task_id.trim());
    let scrubbed = raw.replace(['\n', '\r', '\t'], " ");
    let squashed = scrubbed.split_whitespace().collect::<Vec<&str>>().join(" ");
    outpututil::truncate_chars(&squashed, 100)
}

fn path_exists_in_ref(repo_root: &Path, remote_ref: &str, rel_path: &str) -> Result<bool> {
    let spec = format!("{remote_ref}:{rel_path}");
    let output = git_output(repo_root, &["cat-file", "-e", &spec])
        .with_context(|| format!("check whether {spec} exists"))?;
    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(128) {
        return Ok(false);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("git cat-file failed for {}: {}", spec, stderr.trim())
}

fn repo_relative_path(repo_root: &Path, path: &Path, label: &str) -> Result<String> {
    let rel = path.strip_prefix(repo_root).with_context(|| {
        format!(
            "{} path {} is not under repo root {}",
            label,
            path.display(),
            repo_root.display()
        )
    })?;
    if rel.as_os_str().is_empty() {
        bail!("{} path resolves to repository root", label);
    }
    Ok(rel.to_string_lossy().to_string())
}

fn is_retryable_push_race(err: &GitError) -> bool {
    let text = format!("{err:#}").to_lowercase();
    text.contains("non-fast-forward")
        || text.contains("fetch first")
        || text.contains("stale info")
        || text.contains("failed to push some refs")
        || text.contains("rejected")
}
