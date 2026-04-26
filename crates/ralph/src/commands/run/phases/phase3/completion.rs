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
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

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
    queue_lock: Option<&crate::lock::DirLock>,
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

    let followups_path = queue::default_followups_path(&resolved.repo_root, task_id);
    let queue_followup_inspection = match queue::load_queue(&resolved.queue_path) {
        Ok(queue_file) => {
            let depends_on_count = queue_file
                .tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Todo)
                .filter(|task| {
                    task.depends_on
                        .iter()
                        .any(|dependency| dependency.trim() == task_id)
                })
                .count();
            let relates_to_count = queue_file
                .tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Todo)
                .filter(|task| {
                    task.relates_to
                        .iter()
                        .any(|related| related.trim() == task_id)
                })
                .count();
            serde_json::json!({
                "dependsOnCount": depends_on_count,
                "relatesToCount": relates_to_count,
            })
        }
        Err(err) => serde_json::json!({
            "inspectionError": format!("{err:#}"),
        }),
    };
    // #region agent log
    append_debug_log(
        "H4",
        "crates/ralph/src/commands/run/phases/phase3/completion.rs:finalize_phase3_if_done",
        "phase3 finalization followups inspection",
        serde_json::json!({
            "taskId": task_id,
            "followupsProposalPath": followups_path.display().to_string(),
            "followupsProposalExists": followups_path.exists(),
            "queueFollowupInspection": queue_followup_inspection,
        }),
    );
    // #endregion

    match apply_followups_if_present_for_finalization(resolved, task_id) {
        Ok(Some(report)) => {
            // #region agent log
            append_debug_log(
                "H2",
                "crates/ralph/src/commands/run/phases/phase3/completion.rs:finalize_phase3_if_done",
                "phase3 auto-applied followups proposal",
                serde_json::json!({
                    "taskId": task_id,
                    "applied": true,
                    "createdTasksCount": report.created_tasks.len(),
                    "proposalPath": report.proposal_path.as_str(),
                }),
            );
            // #endregion
            log::info!(
                "Applied {} follow-up task(s) for {} from {}",
                report.created_tasks.len(),
                task_id,
                report.proposal_path
            );
        }
        Ok(None) => {
            // #region agent log
            append_debug_log(
                "H2",
                "crates/ralph/src/commands/run/phases/phase3/completion.rs:finalize_phase3_if_done",
                "phase3 found no followups proposal to apply",
                serde_json::json!({
                    "taskId": task_id,
                    "applied": false,
                }),
            );
            // #endregion
        }
        Err(err) => {
            // #region agent log
            append_debug_log(
                "H2",
                "crates/ralph/src/commands/run/phases/phase3/completion.rs:finalize_phase3_if_done",
                "phase3 followups auto-apply failed",
                serde_json::json!({
                    "taskId": task_id,
                    "applied": false,
                    "error": format!("{err:#}"),
                }),
            );
            // #endregion
            return Err(err);
        }
    }

    crate::commands::run::post_run_supervise(
        resolved,
        queue_lock,
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

fn apply_followups_if_present_for_finalization(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<Option<queue::FollowupApplyReport>> {
    queue::apply_default_followups_if_present_with_removal(resolved, task_id, true)
}

fn append_debug_log(hypothesis_id: &str, location: &str, message: &str, data: serde_json::Value) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let payload = serde_json::json!({
        "sessionId": "f05fb4",
        "runId": "pre-fix",
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": timestamp,
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/Users/mitchfultz/Projects/AI/ralph/.cursor/debug-f05fb4.log")
        && let Ok(line) = serde_json::to_string(&payload)
    {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::apply_followups_if_present_for_finalization;
    use crate::config;
    use crate::contracts::{Config, QueueFile, Task, TaskPriority, TaskStatus};
    use crate::queue;
    use anyhow::Result;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn resolved_for_repo(repo_root: PathBuf) -> config::Resolved {
        config::Resolved {
            config: Config::default(),
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.jsonc"),
            done_path: repo_root.join(".ralph/done.jsonc"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
        }
    }

    fn done_task(task_id: &str) -> Task {
        Task {
            id: task_id.to_string(),
            status: TaskStatus::Done,
            title: "Completed parent task".to_string(),
            description: Some("Parent task for follow-up apply coverage.".to_string()),
            priority: TaskPriority::High,
            tags: vec!["tests".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["runtime observation".to_string()],
            plan: vec!["validate follow-up auto-apply path".to_string()],
            notes: vec![],
            request: Some("Keep follow-up task creation deterministic".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: Some("2026-01-18T00:00:00Z".to_string()),
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }
    }

    #[test]
    fn apply_followups_for_finalization_applies_and_removes_default_proposal() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = resolved_for_repo(temp.path().to_path_buf());
        std::fs::create_dir_all(temp.path().join(".ralph/cache/followups"))?;

        queue::save_queue(
            &resolved.queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )?;
        queue::save_queue(
            &resolved.done_path,
            &QueueFile {
                version: 1,
                tasks: vec![done_task("RQ-0001")],
            },
        )?;

        let proposal_path = queue::default_followups_path(&resolved.repo_root, "RQ-0001");
        let proposal_doc = serde_json::json!({
            "version": 1,
            "source_task_id": "RQ-0001",
            "tasks": [
                {
                    "key": "quickagent-doc",
                    "title": "Write QuickAgent doc update",
                    "description": "Capture actionable guidance from the roadmap deep dive.",
                    "priority": "medium",
                    "tags": ["docs"],
                    "scope": ["docs/"],
                    "evidence": ["roadmap findings"],
                    "plan": ["draft", "review", "publish"],
                    "depends_on_keys": [],
                    "independence_rationale": "Independent documentation follow-up."
                }
            ]
        });
        std::fs::write(&proposal_path, serde_json::to_string_pretty(&proposal_doc)?)?;

        let report = apply_followups_if_present_for_finalization(&resolved, "RQ-0001")?
            .expect("expected follow-up proposal to be applied");
        assert_eq!(report.created_tasks.len(), 1);
        assert!(
            !proposal_path.exists(),
            "proposal file should be removed after apply"
        );

        let queue_after = queue::load_queue(&resolved.queue_path)?;
        assert_eq!(queue_after.tasks.len(), 1);
        assert_eq!(queue_after.tasks[0].status, TaskStatus::Todo);
        assert!(
            queue_after.tasks[0]
                .relates_to
                .iter()
                .any(|related| related == "RQ-0001")
        );
        Ok(())
    }

    #[test]
    fn apply_followups_for_finalization_without_proposal_is_noop() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = resolved_for_repo(temp.path().to_path_buf());
        std::fs::create_dir_all(temp.path().join(".ralph/cache/followups"))?;

        queue::save_queue(
            &resolved.queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )?;
        queue::save_queue(
            &resolved.done_path,
            &QueueFile {
                version: 1,
                tasks: vec![done_task("RQ-0001")],
            },
        )?;

        let report = apply_followups_if_present_for_finalization(&resolved, "RQ-0001")?;
        assert!(report.is_none(), "expected no-op without proposal file");
        Ok(())
    }
}
