//! Parallel worker supervision.
//!
//! Responsibilities:
//! - Post-run supervision for parallel workers without mutating queue/done.
//! - Restore shared bookkeeping files (queue, done, productivity).
//! - Write CI failure marker for coordinator diagnostics.
//!
//! Not handled here:
//! - Standard post-run supervision (see mod.rs).
//! - CI gate with continue session (see ci.rs).
//!
//! Invariants/assumptions:
//! - Called after parallel worker task execution completes.

use crate::contracts::GitRevertMode;
use crate::git;
use crate::promptflow;
use crate::queue;
use crate::runutil;
use crate::timeutil;
use anyhow::{Context, Result};
use std::io::Write as _;

use super::CiContinueContext;
use super::PushPolicy;
use super::enforce_post_run_ci_gate;
use super::git_ops::{finalize_git_state, warn_if_modified_lfs};

const PARALLEL_BOOKKEEPING_PATHS: [&str; 14] = [
    ".ralph/queue.json",
    ".ralph/queue.jsonc",
    ".ralph/done.json",
    ".ralph/done.jsonc",
    ".ralph/cache/productivity.json",
    ".ralph/cache/productivity.jsonc",
    ".ralph/cache/plans/",
    ".ralph/cache/phase2_final/",
    ".ralph/cache/session.json",
    ".ralph/cache/session.jsonc",
    ".ralph/cache/migrations.json",
    ".ralph/cache/migrations.jsonc",
    ".ralph/cache/parallel/",
    ".ralph/logs/",
];

/// Post-run supervision for parallel workers.
///
/// Restores shared bookkeeping files and commits/pushes only the worker's
/// task changes without mutating workspace-local queue/done clones.
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
            enforce_post_run_ci_gate(
                resolved,
                git_revert_mode,
                revert_prompt.as_ref(),
                ci_continue,
                plugins,
                |err| {
                    write_ci_failure_marker(
                        &resolved.repo_root,
                        task_id,
                        &format!("CI gate failed: {:#}", err),
                    );
                },
            )?;
        }

        restore_parallel_worker_bookkeeping(resolved, task_id)?;

        let mut status = git::status_porcelain(&resolved.repo_root)?;
        let mut bookkeeping_lines = collect_bookkeeping_status_lines(&status);
        if !bookkeeping_lines.is_empty() {
            // Defensive retry: if any parallel bookkeeping files still show up in status,
            // restore once more and fail fast if they remain dirty.
            restore_parallel_worker_bookkeeping(resolved, task_id)?;
            status = git::status_porcelain(&resolved.repo_root)?;
            bookkeeping_lines = collect_bookkeeping_status_lines(&status);
            if !bookkeeping_lines.is_empty() {
                anyhow::bail!(
                    "parallel bookkeeping files remained dirty after restore: {}",
                    bookkeeping_lines.join(", ")
                );
            }
        }

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

fn restore_parallel_worker_bookkeeping(
    resolved: &crate::config::Resolved,
    task_id: &str,
) -> Result<()> {
    // Always restore workspace-local bookkeeping files so they are excluded
    // from worker commits and rebases.
    let workspace_queue_path = resolved.repo_root.join(".ralph").join("queue.json");
    let workspace_queue_jsonc_path = resolved.repo_root.join(".ralph").join("queue.jsonc");
    let workspace_done_path = resolved.repo_root.join(".ralph").join("done.json");
    let workspace_done_jsonc_path = resolved.repo_root.join(".ralph").join("done.jsonc");
    let productivity_path = resolved
        .repo_root
        .join(".ralph")
        .join("cache")
        .join("productivity.json");
    let productivity_jsonc_path = resolved
        .repo_root
        .join(".ralph")
        .join("cache")
        .join("productivity.jsonc");
    let paths = vec![
        workspace_queue_path,
        workspace_queue_jsonc_path,
        workspace_done_path,
        workspace_done_jsonc_path,
        productivity_path,
        productivity_jsonc_path,
    ];
    git::restore_tracked_paths_to_head(&resolved.repo_root, &paths)
        .context("restore queue/done/productivity to HEAD")?;
    remove_parallel_worker_generated_artifacts(&resolved.repo_root, task_id)?;
    Ok(())
}

fn remove_parallel_worker_generated_artifacts(
    repo_root: &std::path::Path,
    task_id: &str,
) -> Result<()> {
    cleanup_plan_cache(repo_root, task_id)?;

    let generated_paths = [
        repo_root.join(".ralph/cache/phase2_final"),
        repo_root.join(".ralph/cache/session.json"),
        repo_root.join(".ralph/cache/session.jsonc"),
        repo_root.join(".ralph/cache/migrations.json"),
        repo_root.join(".ralph/cache/migrations.jsonc"),
        repo_root.join(".ralph/cache/parallel"),
        repo_root.join(".ralph/logs"),
    ];

    for path in generated_paths {
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .with_context(|| format!("remove generated directory {}", path.display()))?;
        } else {
            std::fs::remove_file(&path)
                .with_context(|| format!("remove generated file {}", path.display()))?;
        }
    }

    Ok(())
}

fn cleanup_plan_cache(repo_root: &std::path::Path, task_id: &str) -> Result<()> {
    let trimmed = task_id.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let plan_path = promptflow::plan_cache_path(repo_root, trimmed);

    if plan_path.exists() {
        if plan_path.is_dir() {
            std::fs::remove_dir_all(&plan_path).with_context(|| {
                format!("remove generated plan directory {}", plan_path.display())
            })?;
        } else {
            std::fs::remove_file(&plan_path)
                .with_context(|| format!("remove generated plan cache {}", plan_path.display()))?;
        }
    }

    git::restore_tracked_paths_to_head(repo_root, &[plan_path])
        .context("restore tracked plan cache to HEAD")?;

    Ok(())
}

fn collect_bookkeeping_status_lines(status: &str) -> Vec<String> {
    status
        .lines()
        .filter(|line| {
            PARALLEL_BOOKKEEPING_PATHS
                .iter()
                .any(|path| line.contains(path))
        })
        .map(std::string::ToString::to_string)
        .collect()
}

/// Write a marker file indicating CI gate failure.
/// The coordinator can inspect this marker for CI failure diagnostics.
fn write_ci_failure_marker(workspace_path: &std::path::Path, task_id: &str, error_message: &str) {
    let content = serde_json::json!({
        "task_id": task_id,
        "timestamp": timeutil::now_utc_rfc3339_or_fallback(),
        "error": error_message
    });

    let primary_marker =
        workspace_path.join(crate::commands::run::parallel::CI_FAILURE_MARKER_FILE);
    if write_marker_file(&primary_marker, &content) {
        log::debug!(
            "Wrote CI failure marker for task {} at {}",
            task_id,
            primary_marker.display()
        );
        return;
    }

    let fallback_marker =
        workspace_path.join(crate::commands::run::parallel::CI_FAILURE_MARKER_FALLBACK_FILE);
    if write_marker_file(&fallback_marker, &content) {
        log::warn!(
            "Primary CI failure marker unavailable; wrote fallback marker for task {} at {}",
            task_id,
            fallback_marker.display()
        );
        return;
    }

    log::error!(
        "Failed to write both primary and fallback CI failure markers for task {}",
        task_id
    );
}

fn write_marker_file(path: &std::path::Path, content: &serde_json::Value) -> bool {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        log::warn!("Failed to create marker parent directory: {}", e);
        return false;
    }
    match std::fs::File::create(path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(content.to_string().as_bytes()) {
                log::warn!("Failed to write marker file {}: {}", path.display(), e);
                false
            } else {
                true
            }
        }
        Err(e) => {
            log::warn!("Failed to create marker file {}: {}", path.display(), e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Config;
    use crate::testsupport::git as git_test;

    #[test]
    fn write_ci_failure_marker_creates_expected_json_payload() {
        let temp = tempfile::TempDir::new().unwrap();

        write_ci_failure_marker(temp.path(), "RQ-1234", "CI gate failed");

        let marker_path = temp
            .path()
            .join(crate::commands::run::parallel::CI_FAILURE_MARKER_FILE);
        assert!(marker_path.exists(), "marker file should exist");

        let raw = std::fs::read_to_string(marker_path).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(payload["task_id"], "RQ-1234");
        assert_eq!(payload["error"], "CI gate failed");
        assert!(payload["timestamp"].as_str().is_some());
    }

    #[test]
    fn write_ci_failure_marker_overwrites_existing_marker_contents() {
        let temp = tempfile::TempDir::new().unwrap();
        let marker_path = temp
            .path()
            .join(crate::commands::run::parallel::CI_FAILURE_MARKER_FILE);
        std::fs::create_dir_all(marker_path.parent().unwrap()).unwrap();
        std::fs::write(&marker_path, r#"{"task_id":"RQ-0001","error":"old"}"#).unwrap();

        write_ci_failure_marker(temp.path(), "RQ-9999", "new failure");

        let raw = std::fs::read_to_string(marker_path).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(payload["task_id"], "RQ-9999");
        assert_eq!(payload["error"], "new failure");
    }

    #[test]
    fn write_ci_failure_marker_uses_fallback_when_primary_path_is_unusable() {
        let temp = tempfile::TempDir::new().unwrap();
        let primary_parent = temp.path().join(".ralph");
        std::fs::write(&primary_parent, "not-a-directory").unwrap();

        write_ci_failure_marker(temp.path(), "RQ-8888", "ci fallback");

        let fallback = temp
            .path()
            .join(crate::commands::run::parallel::CI_FAILURE_MARKER_FALLBACK_FILE);
        assert!(fallback.exists(), "fallback marker should exist");

        let raw = std::fs::read_to_string(fallback).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(payload["task_id"], "RQ-8888");
        assert_eq!(payload["error"], "ci fallback");
    }

    #[test]
    fn restore_bookkeeping_uses_workspace_paths_when_coordinator_paths_are_overridden() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_root = temp.path().join("workspace");
        std::fs::create_dir_all(repo_root.join(".ralph/cache")).unwrap();
        git_test::init_repo(&repo_root).unwrap();

        let workspace_queue = repo_root.join(".ralph/queue.json");
        let workspace_done = repo_root.join(".ralph/done.json");
        let productivity = repo_root.join(".ralph/cache/productivity.json");
        std::fs::write(&workspace_queue, "{\"version\":1,\"tasks\":[]}").unwrap();
        std::fs::write(&workspace_done, "{\"version\":1,\"tasks\":[]}").unwrap();
        std::fs::write(&productivity, "{\"stats\":[]}").unwrap();
        git_test::commit_all(&repo_root, "init bookkeeping").unwrap();

        let coordinator_root = temp.path().join("coordinator");
        std::fs::create_dir_all(coordinator_root.join(".ralph")).unwrap();
        let coordinator_queue = coordinator_root.join(".ralph/queue.json");
        let coordinator_done = coordinator_root.join(".ralph/done.json");
        std::fs::write(
            &coordinator_queue,
            "{\"version\":1,\"tasks\":[{\"id\":\"RQ-1\"}]}",
        )
        .unwrap();
        std::fs::write(&coordinator_done, "{\"version\":1,\"tasks\":[]}").unwrap();

        // Dirty workspace-local bookkeeping files.
        std::fs::write(
            &workspace_queue,
            "{\"version\":1,\"tasks\":[{\"id\":\"W\"}]}",
        )
        .unwrap();
        std::fs::write(
            &workspace_done,
            "{\"version\":1,\"tasks\":[{\"id\":\"W\"}]}",
        )
        .unwrap();
        std::fs::write(&productivity, "{\"stats\":[\"dirty\"]}").unwrap();

        let resolved = crate::config::Resolved {
            config: Config::default(),
            repo_root: repo_root.clone(),
            queue_path: coordinator_queue,
            done_path: coordinator_done,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        restore_parallel_worker_bookkeeping(&resolved, "RQ-0001").unwrap();

        // Workspace files restored to committed content.
        assert_eq!(
            std::fs::read_to_string(&workspace_queue).unwrap(),
            "{\"version\":1,\"tasks\":[]}"
        );
        assert_eq!(
            std::fs::read_to_string(&workspace_done).unwrap(),
            "{\"version\":1,\"tasks\":[]}"
        );
        assert_eq!(
            std::fs::read_to_string(&productivity).unwrap(),
            "{\"stats\":[]}"
        );
    }

    #[test]
    fn collect_bookkeeping_status_lines_matches_tracked_paths() {
        let status = "\
 M .ralph/queue.json
M  src/lib.rs
 R .ralph/done.json -> .ralph/done-old.json
?? scratch.txt
";

        let matches = collect_bookkeeping_status_lines(status);
        assert_eq!(matches.len(), 2);
        assert!(matches[0].contains(".ralph/queue.json"));
        assert!(matches[1].contains(".ralph/done.json"));
    }

    #[test]
    fn collect_bookkeeping_status_lines_ignores_non_bookkeeping_changes() {
        let status = "\
M  src/lib.rs
A  docs/notes.md
?? temp.log
";

        let matches = collect_bookkeeping_status_lines(status);
        assert!(matches.is_empty());
    }

    #[test]
    fn collect_bookkeeping_status_lines_matches_generated_cache_paths() {
        let status = "\
?? .ralph/cache/plans/RQ-0001.md
?? .ralph/cache/phase2_final/RQ-0001.md
?? .ralph/logs/parallel-debug.log
M  src/lib.rs
";

        let matches = collect_bookkeeping_status_lines(status);
        assert_eq!(matches.len(), 3);
        assert!(matches[0].contains(".ralph/cache/plans/RQ-0001.md"));
        assert!(matches[1].contains(".ralph/cache/phase2_final/RQ-0001.md"));
        assert!(matches[2].contains(".ralph/logs/parallel-debug.log"));
    }

    #[test]
    fn restore_bookkeeping_removes_generated_worker_cache_artifacts() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_root = temp.path().join("workspace");
        std::fs::create_dir_all(repo_root.join(".ralph/cache")).unwrap();
        git_test::init_repo(&repo_root).unwrap();

        let workspace_queue = repo_root.join(".ralph/queue.json");
        let workspace_done = repo_root.join(".ralph/done.json");
        let productivity = repo_root.join(".ralph/cache/productivity.json");
        std::fs::write(&workspace_queue, "{\"version\":1,\"tasks\":[]}").unwrap();
        std::fs::write(&workspace_done, "{\"version\":1,\"tasks\":[]}").unwrap();
        std::fs::write(&productivity, "{\"stats\":[]}").unwrap();
        git_test::commit_all(&repo_root, "init bookkeeping").unwrap();

        let generated_plan = repo_root.join(".ralph/cache/plans/RQ-0001.md");
        let generated_phase2 = repo_root.join(".ralph/cache/phase2_final/RQ-0001.md");
        let generated_session = repo_root.join(".ralph/cache/session.json");
        let generated_logs = repo_root.join(".ralph/logs/parallel.log");
        std::fs::create_dir_all(generated_plan.parent().unwrap()).unwrap();
        std::fs::create_dir_all(generated_phase2.parent().unwrap()).unwrap();
        std::fs::create_dir_all(generated_logs.parent().unwrap()).unwrap();
        std::fs::write(&generated_plan, "plan").unwrap();
        std::fs::write(&generated_phase2, "phase2").unwrap();
        std::fs::write(&generated_session, "{\"task\":\"RQ-0001\"}").unwrap();
        std::fs::write(&generated_logs, "debug").unwrap();

        let resolved = crate::config::Resolved {
            config: Config::default(),
            repo_root: repo_root.clone(),
            queue_path: workspace_queue.clone(),
            done_path: workspace_done.clone(),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        restore_parallel_worker_bookkeeping(&resolved, "RQ-0001").unwrap();

        assert!(!generated_plan.exists());
        assert!(!generated_phase2.exists());
        assert!(!generated_session.exists());
        assert!(!generated_logs.exists());
    }

    #[test]
    fn restore_bookkeeping_restores_tracked_plan_cache() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo_root = temp.path().join("workspace");
        std::fs::create_dir_all(repo_root.join(".ralph/cache")).unwrap();
        git_test::init_repo(&repo_root).unwrap();

        let workspace_queue = repo_root.join(".ralph/queue.json");
        let workspace_done = repo_root.join(".ralph/done.json");
        let productivity = repo_root.join(".ralph/cache/productivity.json");
        std::fs::write(&workspace_queue, "{\"version\":1,\"tasks\":[]}").unwrap();
        std::fs::write(&workspace_done, "{\"version\":1,\"tasks\":[]}").unwrap();
        std::fs::write(&productivity, "{\"stats\":[]}").unwrap();
        git_test::commit_all(&repo_root, "init bookkeeping").unwrap();

        let plan_path = repo_root.join(".ralph/cache/plans/RQ-0001.md");
        std::fs::create_dir_all(plan_path.parent().unwrap()).unwrap();
        std::fs::write(&plan_path, "initial plan").unwrap();
        git_test::commit_all(&repo_root, "track plan cache").unwrap();

        std::fs::write(&plan_path, "generated plan").unwrap();

        let resolved = crate::config::Resolved {
            config: Config::default(),
            repo_root: repo_root.clone(),
            queue_path: workspace_queue.clone(),
            done_path: workspace_done.clone(),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        restore_parallel_worker_bookkeeping(&resolved, "RQ-0001").unwrap();

        assert_eq!(std::fs::read_to_string(&plan_path).unwrap(), "initial plan");
    }
}
