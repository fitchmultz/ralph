//! Post-run supervision helpers.
//!
//! Handles post-run CI gating, queue/done updates, and git push/commit logic.

use super::logging;
use crate::contracts::{GitRevertMode, QueueFile, TaskStatus};
use crate::gitutil::GitError;
use crate::{gitutil, outpututil, queue, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) fn post_run_supervise(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
) -> Result<()> {
    let label = format!("PostRunSupervise for {}", task_id.trim());
    logging::with_scope(&label, || {
        let status = gitutil::status_porcelain(&resolved.repo_root)?;
        let is_dirty = !status.trim().is_empty();

        let mut queue_file = queue::load_queue(&resolved.queue_path)?;
        let mut done_file = queue::load_queue_or_default(&resolved.done_path)?;
        let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
            None
        } else {
            Some(&done_file)
        };
        queue::validate_queue_set(
            &queue_file,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
        )?;

        let (mut task_status, task_title, mut in_done) =
            find_task_status(&queue_file, &done_file, task_id)
                .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;

        if is_dirty {
            warn_if_modified_lfs(&resolved.repo_root);
            if let Err(err) = run_make_ci(&resolved.repo_root) {
                let outcome = runutil::apply_git_revert_mode(
                    &resolved.repo_root,
                    git_revert_mode,
                    "CI gate failure",
                )?;
                bail!(
                    "{} Error: {:#}",
                    runutil::format_revert_failure_message(
                        "CI gate failed: 'make ci' did not pass after the task completed.",
                        outcome,
                    ),
                    err
                );
            }

            queue_file = queue::load_queue(&resolved.queue_path)?;
            done_file = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done_file)
            };
            queue::validate_queue_set(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;

            let (status_after, _title_after, in_done_after) =
                find_task_status(&queue_file, &done_file, task_id)
                    .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;
            task_status = status_after;
            in_done = in_done_after;

            if task_status != TaskStatus::Done {
                if in_done {
                    let outcome = runutil::apply_git_revert_mode(
                        &resolved.repo_root,
                        git_revert_mode,
                        "Task inconsistency detected",
                    )?;
                    bail!(
                        "{}",
                        runutil::format_revert_failure_message(
                            &format!(
                                "Task inconsistency: task {task_id} is archived in .ralph/done.json but its status is not 'done'. Review the task state in .ralph/done.json."
                            ),
                            outcome,
                        )
                    );
                }
                let now = timeutil::now_utc_rfc3339()?;
                queue::set_status(&mut queue_file, task_id, TaskStatus::Done, &now, None)?;
                queue::save_queue(&resolved.queue_path, &queue_file)?;
            }

            queue::archive_done_tasks(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
            )?;

            let commit_message = outpututil::format_task_commit_message(task_id, &task_title);
            gitutil::commit_all(&resolved.repo_root, &commit_message)?;
            push_if_ahead(&resolved.repo_root)?;
            gitutil::require_clean_repo_ignoring_paths(
                &resolved.repo_root,
                false,
                &[".ralph/queue.json", ".ralph/done.json"],
            )?;
            return Ok(());
        }

        if task_status == TaskStatus::Done && in_done {
            push_if_ahead(&resolved.repo_root)?;
            return Ok(());
        }

        let mut changed = false;
        if task_status != TaskStatus::Done {
            if in_done {
                bail!("Task inconsistency: task {task_id} is archived in .ralph/done.json but its status is not 'done'. Review the task state in .ralph/done.json.");
            }
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(&mut queue_file, task_id, TaskStatus::Done, &now, None)?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            changed = true;
        }

        let report = queue::archive_done_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?;
        if !report.moved_ids.is_empty() {
            changed = true;
        }

        if !changed {
            return Ok(());
        }

        let commit_message = outpututil::format_task_commit_message(task_id, &task_title);
        gitutil::commit_all(&resolved.repo_root, &commit_message)?;
        push_if_ahead(&resolved.repo_root)?;
        gitutil::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            &[".ralph/queue.json", ".ralph/done.json"],
        )?;
        Ok(())
    })
}

fn warn_if_modified_lfs(repo_root: &Path) {
    match gitutil::has_lfs(repo_root) {
        Ok(true) => {}
        Ok(false) => return,
        Err(err) => {
            log::warn!("Git LFS detection failed: {:#}", err);
            return;
        }
    }

    let status_paths = match gitutil::status_paths(repo_root) {
        Ok(paths) => paths,
        Err(err) => {
            log::warn!("Unable to read git status for LFS warning: {:#}", err);
            return;
        }
    };

    if status_paths.is_empty() {
        return;
    }

    let lfs_files = match gitutil::list_lfs_files(repo_root) {
        Ok(files) => files,
        Err(err) => {
            log::warn!("Unable to list LFS files: {:#}", err);
            return;
        }
    };

    if lfs_files.is_empty() {
        log::warn!(
            "Git LFS detected but no tracked files were listed; review LFS changes manually."
        );
        return;
    }

    let modified = gitutil::filter_modified_lfs_files(&status_paths, &lfs_files);
    if modified.is_empty() {
        return;
    }

    log::warn!("Modified Git LFS files detected: {}", modified.join(", "));
}

fn push_if_ahead(repo_root: &Path) -> Result<()> {
    match gitutil::is_ahead_of_upstream(repo_root) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
        }
        Err(GitError::NoUpstream) | Err(GitError::NoUpstreamConfigured) => {
            log::warn!("skipping push (no upstream configured)");
            return Ok(());
        }
        Err(err) => {
            return Err(anyhow!("upstream check failed: {:#}", err));
        }
    }
    if let Err(err) = gitutil::push_upstream(repo_root) {
        bail!("Git push failed: the repository has unpushed commits but the push operation failed. Push manually to sync with upstream. Error: {:#}", err);
    }
    Ok(())
}

pub(super) fn find_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Option<(TaskStatus, String, bool)> {
    let needle = task_id.trim();
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), false));
    }
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), true));
    }
    None
}

pub(super) fn run_make_ci(repo_root: &Path) -> Result<()> {
    logging::with_scope("CI gate (make ci)", || {
        let status = Command::new("make")
            .arg("ci")
            .current_dir(repo_root)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("run make ci in {}", repo_root.display()))?;

        if status.success() {
            return Ok(());
        }

        bail!("CI failed: 'make ci' exited with code {:?}. Fix the linting, type-checking, or test failures before proceeding.", status.code())
    })
}
