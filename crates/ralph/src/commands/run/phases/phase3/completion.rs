//! Purpose: Validate and finalize Phase 3 completion state.
//!
//! Responsibilities:
//! - Load/validate queue state needed for Phase 3 completion checks.
//! - Detect when post-run supervision should fire after the task is archived.
//! - Enforce terminal-status and clean-repo requirements before Phase 3 exits.
//!
//! Scope:
//! - Queue/done snapshot inspection and completion enforcement only.
//! - Review prompt execution and higher-level control flow live in sibling modules.
//!
//! Usage:
//! - Called by `phase3/finalization.rs` and re-exported for runtime tests.
//!
//! Invariants/Assumptions:
//! - Queue validation remains read-only.
//! - Git cleanliness rules differ for done vs rejected tasks exactly as before.

use anyhow::Result;

use crate::commands::run::supervision;
use crate::config;
use crate::contracts::{GitPublishMode, GitRevertMode, TaskStatus};
use crate::{git, queue, runutil};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Phase3TaskSnapshot {
    status: TaskStatus,
    in_done: bool,
}

fn load_phase3_task_snapshot(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<Option<Phase3TaskSnapshot>> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    Ok(
        supervision::find_task_status(&queue_file, &done_file, task_id)
            .map(|(status, _title, in_done)| Phase3TaskSnapshot { status, in_done }),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_phase3_if_done(
    resolved: &config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_publish_mode: GitPublishMode,
    push_policy: crate::commands::run::supervision::PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    ci_continue: Option<supervision::CiContinueContext<'_>>,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
    lfs_check: bool,
    no_progress: bool,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<bool> {
    let should_finalize = load_phase3_task_snapshot(resolved, task_id)?
        .map(|snapshot| snapshot.in_done && snapshot.status == TaskStatus::Done)
        .unwrap_or(false);

    if !should_finalize {
        return Ok(false);
    }

    crate::commands::run::post_run_supervise(
        resolved,
        task_id,
        git_revert_mode,
        git_publish_mode,
        push_policy,
        revert_prompt,
        ci_continue,
        notify_on_complete,
        notify_sound,
        lfs_check,
        no_progress,
        plugins,
    )?;
    Ok(true)
}

pub fn ensure_phase3_completion(
    resolved: &config::Resolved,
    task_id: &str,
    git_publish_mode: GitPublishMode,
) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    let (status, _title, in_done) = supervision::find_task_status(&queue_file, &done_file, task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(crate::error_messages::task_not_found_in_queue_or_done(
                task_id
            ))
        })?;

    if !in_done || !(status == TaskStatus::Done || status == TaskStatus::Rejected) {
        anyhow::bail!(
            "Phase 3 incomplete: task {task_id} is not archived with a terminal status. Run `ralph task done` in Phase 3 before finishing."
        );
    }

    if git_publish_mode != GitPublishMode::Off {
        if status == TaskStatus::Rejected {
            git::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
            )?;
        } else {
            git::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                &[
                    ".ralph/config.jsonc",
                    ".ralph/cache/productivity.json",
                    ".ralph/cache/productivity.jsonc",
                ],
            )?;
        }
    } else {
        log::info!(
            "Git publish mode is off; skipping clean-repo enforcement for Phase 3 completion."
        );
    }
    Ok(())
}
