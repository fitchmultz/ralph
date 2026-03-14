//! Post-run supervision orchestration.
//!
//! Responsibilities:
//! - Orchestrate post-run workflow: CI gate, queue updates, git operations, notifications.
//! - Manage ContinueSession for session resumption.
//! - Coordinate celebration triggers and productivity stats.
//! - Provide parallel-worker supervision without mutating queue/done.
//!
//! Not handled here:
//! - Individual concern implementations (see queue_ops.rs, git_ops.rs, ci.rs, notify.rs).
//! - Runner process execution (handled by phases module).
//! - Continue session implementation (see continue_session.rs).
//! - Parallel worker supervision details (see parallel_worker.rs).
//!
//! Invariants/assumptions:
//! - post_run_supervise is called after task execution completes.
//! - Queue files are valid and accessible.
//! - CI enforcement applies whenever the repo is already dirty or supervision would mutate queue/done.

use crate::celebrations;
use crate::contracts::{GitPublishMode, GitRevertMode};
use crate::git;
use crate::notification;
use crate::productivity;
use crate::queue;
use crate::runutil;
use anyhow::{Context, Result, anyhow, ensure};

mod ci;
mod git_ops;
mod notify;
mod queue_ops;

mod continue_session;
#[cfg(test)]
mod parallel_worker;

#[cfg(test)]
mod tests;

// Re-export items needed by run/mod.rs and other modules
pub(crate) use ci::{
    capture_ci_gate_result, ci_gate_command_label, run_ci_gate, run_ci_gate_with_continue_session,
};
use git_ops::{finalize_git_state, push_if_ahead, warn_if_modified_lfs};
use notify::build_notification_config;
pub(crate) use queue_ops::find_task_status;
use queue_ops::{
    build_post_run_queue_mutation_plan, ensure_task_done_clean_or_bail,
    ensure_task_done_dirty_or_revert, maintain_and_validate_queues,
};

// Re-export from submodules
pub(crate) use continue_session::{CiContinueContext, ContinueSession, resume_continue_session};
#[cfg(test)]
pub(crate) use parallel_worker::post_run_supervise_parallel_worker;

use super::logging;

/// Policy for pushing git commits after a run completes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PushPolicy {
    /// Require an existing upstream; skip push if none is configured.
    RequireUpstream,
    /// Allow creating an upstream (e.g., `git push -u origin HEAD`) when missing.
    AllowCreateUpstream,
}

pub(super) fn enforce_post_run_ci_gate<F>(
    resolved: &crate::config::Resolved,
    git_revert_mode: GitRevertMode,
    revert_prompt: Option<&runutil::RevertPromptHandler>,
    ci_continue: Option<CiContinueContext<'_>>,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
    mut on_ci_failure: F,
) -> Result<()>
where
    F: FnMut(&anyhow::Error),
{
    let mut ci_continue = ci_continue;
    let ci_gate_result = if let Some(ci_continue) = ci_continue.as_mut() {
        let continue_session = &mut *ci_continue.continue_session;
        let on_resume = &mut *ci_continue.on_resume;
        ci::run_ci_gate_with_continue_session(
            resolved,
            git_revert_mode,
            revert_prompt,
            continue_session,
            |output, elapsed| on_resume(output, elapsed),
            plugins,
        )
    } else {
        run_ci_gate(resolved).map(|_| ())
    };

    if let Err(err) = ci_gate_result {
        on_ci_failure(&err);
        let outcome = runutil::apply_git_revert_mode(
            &resolved.repo_root,
            git_revert_mode,
            "CI gate failure",
            revert_prompt,
        )?;
        anyhow::bail!(
            "{} Error: {:#}",
            runutil::format_revert_failure_message(
                &format!(
                    "CI gate failed: '{}' did not pass after the task completed.",
                    ci_gate_command_label(resolved)
                ),
                outcome,
            ),
            err
        );
    }

    Ok(())
}

/// Main post-run supervision entry point.
///
/// Orchestrates the post-run workflow:
/// 1. Repair/load queue state and detect whether repo or queue/done will change
/// 2. Run CI gate when existing or pending mutations require enforcement
/// 3. Update queue/done files
/// 4. Commit and push if enabled
/// 5. Trigger notifications and celebrations
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_run_supervise(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_publish_mode: GitPublishMode,
    push_policy: PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    ci_continue: Option<CiContinueContext<'_>>,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
    lfs_check: bool,
    no_progress: bool,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<()> {
    let label = format!("PostRunSupervise for {}", task_id.trim());
    logging::with_scope(&label, || {
        let (mut queue_file, mut done_file) =
            maintain_and_validate_queues(resolved).context("Initial queue maintenance failed")?;
        let mut repo_dirty = !git::status_porcelain(&resolved.repo_root)?
            .trim()
            .is_empty();
        let mut queue_plan = build_post_run_queue_mutation_plan(&queue_file, &done_file, task_id)?;
        let task_title = queue_plan.task_title.clone();

        if repo_dirty || queue_plan.will_mutate_queue_files() {
            if repo_dirty && let Err(err) = warn_if_modified_lfs(&resolved.repo_root, lfs_check) {
                return Err(anyhow!(
                    "LFS validation failed: {}. Use --lfs-check to enable strict validation or fix the LFS issues.",
                    err
                ));
            }
            enforce_post_run_ci_gate(
                resolved,
                git_revert_mode,
                revert_prompt.as_ref(),
                ci_continue,
                plugins,
                |_| {},
            )?;

            let (q, d) = maintain_and_validate_queues(resolved)
                .context("Post-CI queue maintenance failed")?;
            queue_file = q;
            done_file = d;
            repo_dirty = !git::status_porcelain(&resolved.repo_root)?
                .trim()
                .is_empty();
            queue_plan = build_post_run_queue_mutation_plan(&queue_file, &done_file, task_id)?;
        }

        if !repo_dirty && !queue_plan.will_mutate_queue_files() {
            ensure!(
                queue_plan.task_already_archived_done(),
                "Post-run supervision reached a no-op state for task {} without an archived done entry.",
                task_id.trim()
            );

            if git_publish_mode == GitPublishMode::CommitAndPush {
                push_if_ahead(&resolved.repo_root, push_policy).context("Git push failed")?;
            } else {
                log::info!(
                    "Git publish mode is {}; skipping push.",
                    git_publish_mode.as_str()
                );
            }

            let notify_config =
                build_notification_config(resolved, notify_on_complete, notify_sound);
            notification::notify_task_complete(task_id, &task_title, &notify_config);
            trigger_celebration(resolved, task_id, &task_title, no_progress);
            return Ok(());
        }

        let mut changed = false;
        if repo_dirty {
            ensure_task_done_dirty_or_revert(
                resolved,
                &mut queue_file,
                task_id,
                queue_plan.task_status,
                queue_plan.in_done,
                git_revert_mode,
                revert_prompt.as_ref(),
            )
            .context("Ensuring task is marked Done (dirty repo) failed")?;
        } else {
            changed = ensure_task_done_clean_or_bail(
                resolved,
                &mut queue_file,
                task_id,
                queue_plan.task_status,
                queue_plan.in_done,
            )
            .context("Ensuring task is marked Done (clean repo) failed")?;
        }

        let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
        let report = queue::archive_terminal_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )
        .context("Queue archiving failed")?;
        if !report.moved_ids.is_empty() {
            changed = true;
        }

        if !repo_dirty && !changed {
            return Ok(());
        }

        // Trigger celebration and record productivity stats BEFORE git commit
        // so productivity.json gets committed along with other changes
        trigger_celebration(resolved, task_id, &task_title, no_progress);

        finalize_git_state(
            resolved,
            task_id,
            &task_title,
            git_publish_mode,
            push_policy,
        )
        .context("Git finalization failed")?;

        let notify_config = build_notification_config(resolved, notify_on_complete, notify_sound);
        notification::notify_task_complete(task_id, &task_title, &notify_config);

        Ok(())
    })
}

/// Trigger celebration and record productivity stats for task completion.
fn trigger_celebration(
    resolved: &crate::config::Resolved,
    task_id: &str,
    task_title: &str,
    no_progress: bool,
) {
    let cache_dir = resolved.repo_root.join(".ralph").join("cache");
    match productivity::record_task_completion_by_id(task_id, task_title, &cache_dir) {
        Ok(result) => {
            if celebrations::should_celebrate(no_progress) {
                let celebration =
                    celebrations::celebrate_task_completion(task_id, task_title, &result);
                println!("{}", celebration);
            }

            // Mark milestone as celebrated if one was achieved
            if let Some(threshold) = result.milestone_achieved
                && let Err(err) = productivity::mark_milestone_celebrated(&cache_dir, threshold)
            {
                log::debug!("Failed to mark milestone as celebrated: {}", err);
            }
        }
        Err(err) => {
            log::debug!("Failed to record productivity stats: {}", err);
            if celebrations::should_celebrate(no_progress) {
                let celebration = celebrations::celebrate_standard(task_id, task_title);
                println!("{}", celebration);
            }
        }
    }
}
