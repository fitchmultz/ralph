//! Parallel worker supervision.
//!
//! Responsibilities:
//! - Post-run supervision for parallel workers without mutating queue/done.
//! - Restore shared bookkeeping files (queue, done, productivity).
//!
//! Not handled here:
//! - Standard post-run supervision (see mod.rs).
//! - CI gate with continue session (see ci.rs).
//!
//! Invariants/assumptions:
//! - Called after parallel worker task execution completes.

use crate::contracts::GitRevertMode;
use crate::git;
use crate::queue;
use crate::runutil;
use anyhow::{Context, Result};

use super::CiContinueContext;
use super::PushPolicy;
use super::ci::{ci_gate_command_label, run_ci_gate, run_ci_gate_with_continue_session};
use super::git_ops::{finalize_git_state, warn_if_modified_lfs};

/// Post-run supervision for parallel workers.
///
/// Restores shared bookkeeping files and commits/pushes only the worker's
/// task changes without mutating queue/done. Task finalization is handled
/// by the merge-agent subprocess in the coordinator process.
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_run_supervise_parallel_worker(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    ci_continue: Option<CiContinueContext<'_>>,
    lfs_check: bool,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<()> {
    let label = format!("PostRunSuperviseParallelWorker for {}", task_id.trim());
    super::logging::with_scope(&label, || {
        let status = git::status_porcelain(&resolved.repo_root)?;
        let is_dirty = !status.trim().is_empty();

        if is_dirty {
            if let Err(err) = warn_if_modified_lfs(&resolved.repo_root, lfs_check) {
                return Err(anyhow::anyhow!(
                    "LFS validation failed: {}. Use --lfs-check to enable strict validation or fix the LFS issues.",
                    err
                ));
            }
            let mut ci_continue = ci_continue;
            if let Some(ci_continue) = ci_continue.as_mut() {
                let continue_session = &mut *ci_continue.continue_session;
                let on_resume = &mut *ci_continue.on_resume;
                if continue_session
                    .session_id
                    .as_deref()
                    .unwrap_or("")
                    .is_empty()
                {
                    log::warn!(
                        "CI gate continue requested but no session id; falling back to standard CI gate handling."
                    );
                    let result = run_ci_gate(resolved)?;
                    if !result.success {
                        let outcome = runutil::apply_git_revert_mode(
                            &resolved.repo_root,
                            git_revert_mode,
                            "CI gate failure",
                            revert_prompt.as_ref(),
                        )?;
                        let exit_code_display = result.exit_code.unwrap_or(-1);
                        anyhow::bail!(
                            "{} Error: CI failed with exit code {exit_code_display}",
                            runutil::format_revert_failure_message(
                                &format!(
                                    "CI gate failed: '{}' did not pass after the task completed.",
                                    ci_gate_command_label(resolved)
                                ),
                                outcome,
                            ),
                        );
                    }
                } else if let Err(err) = run_ci_gate_with_continue_session(
                    resolved,
                    git_revert_mode,
                    revert_prompt.as_ref(),
                    continue_session,
                    |output, elapsed| on_resume(output, elapsed),
                    plugins,
                ) {
                    let outcome = runutil::apply_git_revert_mode(
                        &resolved.repo_root,
                        git_revert_mode,
                        "CI gate failure",
                        revert_prompt.as_ref(),
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
            } else {
                let result = run_ci_gate(resolved)?;
                if !result.success {
                    let outcome = runutil::apply_git_revert_mode(
                        &resolved.repo_root,
                        git_revert_mode,
                        "CI gate failure",
                        revert_prompt.as_ref(),
                    )?;
                    let exit_code_display = result.exit_code.unwrap_or(-1);
                    anyhow::bail!(
                        "{} Error: CI failed with exit code {exit_code_display}",
                        runutil::format_revert_failure_message(
                            &format!(
                                "CI gate failed: '{}' did not pass after the task completed.",
                                ci_gate_command_label(resolved)
                            ),
                            outcome,
                        ),
                    );
                }
            }
        }

        restore_parallel_worker_bookkeeping(resolved)?;

        let status = git::status_porcelain(&resolved.repo_root)?;
        if status.trim().is_empty() {
            return Ok(());
        }

        if git_commit_push_enabled {
            let task_title = task_title_from_queue_or_done(resolved, task_id)?.unwrap_or_default();
            finalize_git_state(
                resolved,
                task_id,
                &task_title,
                git_commit_push_enabled,
                push_policy,
            )
            .context("Git finalization failed")?;
        } else {
            log::info!("Auto git commit/push disabled; leaving repo dirty after worker run.");
        }

        Ok(())
    })
}

fn task_title_from_queue_or_done(
    resolved: &crate::config::Resolved,
    task_id: &str,
) -> Result<Option<String>> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    Ok(None)
}

fn restore_parallel_worker_bookkeeping(resolved: &crate::config::Resolved) -> Result<()> {
    let productivity_path = resolved
        .repo_root
        .join(".ralph")
        .join("cache")
        .join("productivity.json");
    let paths = vec![
        resolved.queue_path.clone(),
        resolved.done_path.clone(),
        productivity_path,
    ];
    git::restore_tracked_paths_to_head(&resolved.repo_root, &paths)
        .context("restore queue/done/productivity to HEAD")?;
    Ok(())
}
