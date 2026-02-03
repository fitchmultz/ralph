//! Parallel run loop supervisor and worker orchestration.
//!
//! Responsibilities:
//! - Select runnable tasks and spawn parallel workers in git workspaces (clone-based).
//! - Create PRs on success/failure and optionally dispatch merge runner work.
//! - Track in-flight workers and coordinate cleanup after merges.
//!
//! Not handled here:
//! - CLI parsing (see `crate::cli::run`).
//! - Task execution details (delegated to `ralph run one` workers).
//! - Merge conflict resolution logic (see `merge_runner`).
//! - Worker lifecycle management (see `worker`).
//! - State synchronization (see `sync`).
//! - CLI argument builders (see `args`).
//!
//! Invariants/assumptions:
//! - Queue order is authoritative for task selection.
//! - Workers run in isolated workspaces with dedicated branches.
//! - PR creation relies on authenticated `gh` CLI access.

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::{ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, ParallelMergeWhen};
use crate::{git, promptflow, runutil, signal, timeutil};
use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

mod args;
mod cleanup_guard;
mod merge_runner;
mod path_map;
mod state;
mod sync;
mod worker;

use cleanup_guard::ParallelCleanupGuard;
use merge_runner::{MergeQueueSource, MergeResult};
use sync::{commit_failure_changes, ensure_branch_pushed, sync_ralph_state};
use worker::{WorkerState, collect_excluded_ids, select_next_task, spawn_worker};

pub(crate) struct ParallelRunOptions {
    pub max_tasks: u32,
    pub workers: u8,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
    pub merge_when: ParallelMergeWhen,
}

struct ParallelSettings {
    workers: u8,
    merge_when: ParallelMergeWhen,
    merge_method: ParallelMergeMethod,
    auto_pr: bool,
    auto_merge: bool,
    draft_on_failure: bool,
    conflict_policy: ConflictPolicy,
    merge_retries: u8,
    workspace_root: PathBuf,
    branch_prefix: String,
    delete_branch_on_merge: bool,
    merge_runner: MergeRunnerConfig,
}

pub(crate) fn run_loop_parallel(
    resolved: &config::Resolved,
    opts: ParallelRunOptions,
) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let ctrlc = crate::runner::ctrlc_state()
        .map_err(|e| anyhow::anyhow!("Ctrl-C handler initialization failed: {}", e))?;

    if ctrlc.interrupted.load(Ordering::SeqCst) {
        return Err(runutil::RunAbort::new(
            runutil::RunAbortReason::Interrupted,
            "Ctrl+C was pressed before parallel execution started",
        )
        .into());
    }
    ctrlc.interrupted.store(false, Ordering::SeqCst);

    signal::clear_stop_signal_at_loop_start(&cache_dir);

    let mut settings = resolve_parallel_settings(resolved, &opts)?;
    if settings.workers < 2 {
        bail!(
            "Parallel run requires workers >= 2 (got {})",
            settings.workers
        );
    }

    let current_branch = git::current_branch(&resolved.repo_root)?;
    let state_path = state::state_file_path(&resolved.repo_root);
    let started_at = timeutil::now_utc_rfc3339_or_fallback();
    let mut state_file = if let Some(existing) = state::load_state(&state_path)? {
        if existing.base_branch != current_branch {
            bail!(
                "Parallel state base branch '{}' does not match current branch '{}'.",
                existing.base_branch,
                current_branch
            );
        }
        if existing.merge_method != settings.merge_method {
            log::warn!(
                "Parallel state merge_method {:?} overrides current settings {:?}.",
                existing.merge_method,
                settings.merge_method
            );
            settings.merge_method = existing.merge_method;
        }
        if existing.merge_when != settings.merge_when {
            log::warn!(
                "Parallel state merge_when {:?} overrides current settings {:?}.",
                existing.merge_when,
                settings.merge_when
            );
            settings.merge_when = existing.merge_when;
        }
        existing
    } else {
        let state = state::ParallelStateFile::new(
            started_at,
            current_branch.clone(),
            settings.merge_method,
            settings.merge_when,
        );
        state::save_state(&state_path, &state)?;
        state
    };

    let base_branch = state_file.base_branch.clone();

    let merge_stop = Arc::new(AtomicBool::new(false));
    let dropped_tasks = prune_stale_tasks_in_flight(&mut state_file);
    if !dropped_tasks.is_empty() {
        log::warn!(
            "Dropping stale in-flight tasks: {}",
            dropped_tasks.join(", ")
        );
        state::save_state(&state_path, &state_file)?;
    }

    let (pr_tx, pr_rx) = mpsc::channel::<git::PrInfo>();
    let (merge_result_tx, merge_result_rx) = mpsc::channel::<MergeResult>();
    let mut merge_handle = None;
    let existing_prs: Vec<git::PrInfo> = state_file
        .prs
        .iter()
        .filter(|record| !record.merged)
        .map(|record| {
            let fallback_head = format!("{}{}", settings.branch_prefix, record.task_id);
            record.pr_info(&fallback_head, &base_branch)
        })
        .collect();

    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AsCreated {
        let resolved = resolved.clone();
        let merge_method = settings.merge_method;
        let conflict_policy = settings.conflict_policy;
        let merge_runner_cfg = settings.merge_runner.clone();
        let retries = settings.merge_retries;
        let workspace_root = settings.workspace_root.clone();
        let delete_branch = settings.delete_branch_on_merge;
        let merge_result_tx_for_thread = merge_result_tx.clone();
        let merge_stop_for_thread = Arc::clone(&merge_stop);

        merge_handle = Some(thread::spawn(move || {
            merge_runner::run_merge_runner(
                &resolved,
                merge_method,
                conflict_policy,
                merge_runner_cfg,
                retries,
                MergeQueueSource::AsCreated(pr_rx),
                &workspace_root,
                delete_branch,
                merge_result_tx_for_thread,
                merge_stop_for_thread,
            )
        }));
    }

    // Track completed workspaces separately (not owned by guard, cleaned up after merge)
    let mut completed_workspaces: HashMap<String, git::WorkspaceSpec> = HashMap::new();
    let mut created_prs: Vec<git::PrInfo> = existing_prs.clone();

    for record in state_file.prs.iter().filter(|record| !record.merged) {
        let path = record
            .workspace_path()
            .unwrap_or_else(|| settings.workspace_root.join(&record.task_id));
        if path.exists() {
            completed_workspaces.insert(
                record.task_id.clone(),
                git::WorkspaceSpec {
                    task_id: record.task_id.clone(),
                    path,
                    branch: format!("{}{}", settings.branch_prefix, record.task_id),
                },
            );
        }
    }

    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AsCreated {
        for pr in &existing_prs {
            let _ = pr_tx.send(pr.clone());
        }
    }

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let worker_overrides = overrides_for_parallel_workers(resolved, &opts.agent_overrides);
    let mut tasks_started: u32 = 0;
    let mut tasks_attempted: usize = 0;
    let mut tasks_succeeded: usize = 0;
    let mut tasks_failed: usize = 0;
    let mut interrupted = false;

    // Create cleanup guard to ensure resources are cleaned up on any exit path
    let mut guard = ParallelCleanupGuard::new(
        Arc::clone(&merge_stop),
        pr_tx,
        merge_handle,
        state_path.clone(),
        state_file,
        settings.workspace_root.clone(),
    );

    // Run the main loop inside a closure so we can handle cleanup on any error
    let loop_result: Result<()> = (|| {
        loop {
            if ctrlc.interrupted.load(Ordering::SeqCst) {
                interrupted = true;
                log::info!("Ctrl+C detected; stopping parallel run and cleaning up.");
                break;
            }

            if signal::stop_signal_exists(&cache_dir) {
                log::info!("Stop signal detected; no new tasks will be started.");
            }

            // Spawn new workers until capacity or max-tasks reached.
            while guard.in_flight().len() < settings.workers as usize
                && (opts.max_tasks == 0 || tasks_started < opts.max_tasks)
                && !signal::stop_signal_exists(&cache_dir)
            {
                let excluded = collect_excluded_ids(guard.state_file(), guard.in_flight());
                let (task_id, task_title) =
                    match select_next_task(resolved, include_draft, &excluded, opts.force)? {
                        Some(task) => task,
                        None => break,
                    };

                let workspace = git::create_workspace_at(
                    &resolved.repo_root,
                    &settings.workspace_root,
                    &task_id,
                    &base_branch,
                    &settings.branch_prefix,
                )?;
                sync_ralph_state(resolved, &workspace.path)?;

                let child = spawn_worker(
                    resolved,
                    &workspace.path,
                    &task_id,
                    &worker_overrides,
                    opts.force,
                )?;

                let record = state::ParallelTaskRecord::new(&task_id, &workspace, child.id());
                guard.state_file_mut().upsert_task(record);
                state::save_state(&state_path, guard.state_file())?;

                // Register worker with the guard for cleanup tracking
                guard.register_worker(
                    task_id.clone(),
                    WorkerState {
                        task_id: task_id.clone(),
                        task_title: task_title.clone(),
                        workspace: workspace.clone(),
                        child,
                    },
                );
                // Also register the workspace for cleanup
                guard.register_workspace(task_id.clone(), workspace);

                tasks_started += 1;
            }

            // Drain merge results for cleanup.
            while let Ok(result) = merge_result_rx.try_recv() {
                if result.merged {
                    if let Some(workspace) = completed_workspaces.remove(&result.task_id)
                        && let Err(err) =
                            git::remove_workspace(&settings.workspace_root, &workspace, true)
                    {
                        log::warn!(
                            "Failed to remove workspace for {}: {:#}",
                            result.task_id,
                            err
                        );
                    }
                    guard.state_file_mut().mark_pr_merged(&result.task_id);
                    state::save_state(&state_path, guard.state_file())?;
                }
            }

            // Poll workers using the guard's poll_workers method
            let finished = guard.poll_workers();

            for (task_id, task_title, workspace, status) in finished {
                tasks_attempted += 1;
                if status.success() {
                    tasks_succeeded += 1;
                    // Handle success
                    if settings.auto_pr
                        && let Err(e) = (|| -> Result<()> {
                            ensure_branch_pushed(&workspace.path)?;
                            let body = promptflow::read_phase2_final_response_cache(
                                &workspace.path,
                                &task_id,
                            )
                            .unwrap_or_default();
                            let title = format!("{}: {}", task_id, task_title);
                            let pr = git::create_pr(
                                &resolved.repo_root,
                                &title,
                                &body,
                                &workspace.branch,
                                &base_branch,
                                false,
                            )?;
                            guard
                                .state_file_mut()
                                .upsert_pr(state::ParallelPrRecord::new(
                                    &task_id,
                                    &pr,
                                    Some(&workspace.path),
                                ));
                            state::save_state(&state_path, guard.state_file())?;
                            created_prs.push(pr.clone());
                            if settings.auto_merge
                                && settings.merge_when == ParallelMergeWhen::AsCreated
                                && let Some(tx) = guard.pr_tx()
                            {
                                let _ = tx.send(pr);
                            }
                            Ok(())
                        })()
                    {
                        log::warn!("Failed to create PR for {}: {}", task_id, e);
                    }
                } else {
                    tasks_failed += 1;
                    // Handle failure
                    if settings.auto_pr
                        && settings.draft_on_failure
                        && let Err(e) = (|| -> Result<()> {
                            if !commit_failure_changes(&workspace.path, &task_id)? {
                                log::warn!(
                                    "Worker {} failed with no changes; skipping draft PR.",
                                    task_id
                                );
                                return Ok(());
                            }
                            ensure_branch_pushed(&workspace.path)?;
                            let body =
                                format!("Failed run for {}. Draft PR generated by Ralph.", task_id);
                            let title = format!("{}: {}", task_id, task_title);
                            let pr = git::create_pr(
                                &resolved.repo_root,
                                &title,
                                &body,
                                &workspace.branch,
                                &base_branch,
                                true,
                            )?;
                            guard
                                .state_file_mut()
                                .upsert_pr(state::ParallelPrRecord::new(
                                    &task_id,
                                    &pr,
                                    Some(&workspace.path),
                                ));
                            state::save_state(&state_path, guard.state_file())?;
                            log::info!(
                                "Draft PR {} created for {}; skipping auto-merge.",
                                pr.number,
                                task_id
                            );
                            Ok(())
                        })()
                    {
                        log::warn!("Failed to create draft PR for {}: {}", task_id, e);
                    }
                }

                // Move workspace to completed_workspaces for potential merge cleanup
                completed_workspaces.insert(task_id.clone(), workspace.clone());
                guard.state_file_mut().remove_task(&task_id);
                state::save_state(&state_path, guard.state_file())?;
                guard.remove_worker(&task_id);
            }

            if guard.in_flight().is_empty() {
                let no_more_tasks = opts.max_tasks != 0 && tasks_started >= opts.max_tasks;
                let excluded = collect_excluded_ids(guard.state_file(), guard.in_flight());
                let next_available =
                    select_next_task(resolved, include_draft, &excluded, opts.force)?.is_some();
                if no_more_tasks || !next_available {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(500));
        }

        Ok(())
    })();

    // Handle cleanup on any exit path (success, error, or interrupt)
    if interrupted || loop_result.is_err() {
        // Cleanup will be performed by the guard's Drop implementation
        // The guard owns the merge_stop signal, pr_tx, merge_handle, and state
        // When it drops, it will:
        // 1. Signal merge runner to stop
        // 2. Drop pr_tx to unblock receiver
        // 3. Join merge runner thread
        // 4. Terminate in-flight workers
        // 5. Clear and persist state

        if interrupted {
            return Err(runutil::RunAbort::new(
                runutil::RunAbortReason::Interrupted,
                "Parallel run interrupted by Ctrl+C",
            )
            .into());
        }
        return loop_result;
    }

    // Success path - perform normal cleanup, then disarm guard
    // Drop pr_tx to signal merge runner to stop
    drop(guard.take_pr_tx());

    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AfterAll {
        let merge_result_tx = merge_result_tx.clone();
        merge_runner::run_merge_runner(
            resolved,
            settings.merge_method,
            settings.conflict_policy,
            settings.merge_runner.clone(),
            settings.merge_retries,
            MergeQueueSource::AfterAll(created_prs.clone()),
            &settings.workspace_root,
            settings.delete_branch_on_merge,
            merge_result_tx,
            Arc::clone(&merge_stop),
        )?;
    }

    if let Some(handle) = guard.take_merge_handle() {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => bail!("Merge runner thread panicked"),
        }
    }

    // Drain any remaining merge results for cleanup.
    while let Ok(result) = merge_result_rx.try_recv() {
        if result.merged {
            if let Some(workspace) = completed_workspaces.remove(&result.task_id)
                && let Err(err) = git::remove_workspace(&settings.workspace_root, &workspace, true)
            {
                log::warn!(
                    "Failed to remove workspace for {}: {:#}",
                    result.task_id,
                    err
                );
            }
            guard.state_file_mut().mark_pr_merged(&result.task_id);
            state::save_state(&state_path, guard.state_file())?;
        }
    }

    // All cleanup successful - disarm the guard
    guard.mark_completed();

    if tasks_attempted > 0 {
        let notify_on_complete = opts
            .agent_overrides
            .notify_on_complete
            .or(resolved.config.agent.notification.notify_on_complete)
            .unwrap_or(true);
        let notify_on_fail = opts
            .agent_overrides
            .notify_on_fail
            .or(resolved.config.agent.notification.notify_on_fail)
            .unwrap_or(true);
        let notify_on_loop_complete = resolved
            .config
            .agent
            .notification
            .notify_on_loop_complete
            .unwrap_or(true);
        let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

        let notify_config = crate::notification::NotificationConfig {
            enabled,
            notify_on_complete,
            notify_on_fail,
            notify_on_loop_complete,
            suppress_when_active: resolved
                .config
                .agent
                .notification
                .suppress_when_active
                .unwrap_or(true),
            sound_enabled: opts
                .agent_overrides
                .notify_sound
                .or(resolved.config.agent.notification.sound_enabled)
                .unwrap_or(false),
            sound_path: resolved.config.agent.notification.sound_path.clone(),
            timeout_ms: resolved
                .config
                .agent
                .notification
                .timeout_ms
                .unwrap_or(8000),
        };
        crate::notification::notify_loop_complete(
            tasks_attempted,
            tasks_succeeded,
            tasks_failed,
            &notify_config,
        );
    }

    Ok(())
}

/// Prune stale in-flight tasks from the parallel state file.
///
/// Drops records when:
/// - The workspace path no longer exists, OR
/// - The recorded PID exists and is no longer running (pid_is_running returns Some(false))
///
/// Retains records when:
/// - PID is missing (None), OR
/// - pid_is_running returns None (indeterminate status)
///
/// Returns the list of dropped task IDs for logging.
fn prune_stale_tasks_in_flight(state_file: &mut state::ParallelStateFile) -> Vec<String> {
    let mut dropped = Vec::new();
    state_file.tasks_in_flight.retain(|record| {
        let path = Path::new(&record.workspace_path);
        if !path.exists() {
            dropped.push(record.task_id.clone());
            return false;
        }
        if let Some(pid) = record.pid
            && crate::lock::pid_is_running(pid) == Some(false)
        {
            dropped.push(record.task_id.clone());
            return false;
        }
        true
    });
    dropped
}

fn overrides_for_parallel_workers(
    resolved: &config::Resolved,
    overrides: &AgentOverrides,
) -> AgentOverrides {
    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(overrides, resolved);
    if repoprompt_flags.plan_required || repoprompt_flags.tool_injection {
        log::warn!(
            "Parallel workers disable RepoPrompt plan/tooling instructions to keep edits in workspace clones."
        );
    }

    let mut worker_overrides = overrides.clone();
    worker_overrides.repoprompt_plan_required = Some(false);
    worker_overrides.repoprompt_tool_injection = Some(false);
    worker_overrides
}

fn resolve_parallel_settings(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
) -> Result<ParallelSettings> {
    let cfg = &resolved.config.parallel;
    Ok(ParallelSettings {
        workers: opts.workers,
        merge_when: opts.merge_when,
        merge_method: cfg.merge_method.unwrap_or(ParallelMergeMethod::Squash),
        auto_pr: cfg.auto_pr.unwrap_or(true),
        auto_merge: cfg.auto_merge.unwrap_or(true),
        draft_on_failure: cfg.draft_on_failure.unwrap_or(true),
        conflict_policy: cfg.conflict_policy.unwrap_or(ConflictPolicy::AutoResolve),
        merge_retries: cfg.merge_retries.unwrap_or(5),
        workspace_root: git::workspace_root(&resolved.repo_root, &resolved.config),
        branch_prefix: cfg
            .branch_prefix
            .clone()
            .unwrap_or_else(|| "ralph/".to_string()),
        delete_branch_on_merge: cfg.delete_branch_on_merge.unwrap_or(true),
        merge_runner: cfg.merge_runner.clone().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Config;

    use tempfile::TempDir;

    #[test]
    fn overrides_for_parallel_workers_forces_repoprompt_off() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().to_path_buf();
        let mut cfg = Config::default();
        cfg.agent.repoprompt_plan_required = Some(true);
        cfg.agent.repoprompt_tool_injection = Some(true);

        let resolved = config::Resolved {
            config: cfg,
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
        };

        let overrides = AgentOverrides {
            include_draft: Some(true),
            repoprompt_plan_required: Some(true),
            repoprompt_tool_injection: Some(true),
            ..AgentOverrides::default()
        };

        let worker_overrides = overrides_for_parallel_workers(&resolved, &overrides);

        assert_eq!(worker_overrides.include_draft, Some(true));
        assert_eq!(worker_overrides.repoprompt_plan_required, Some(false));
        assert_eq!(worker_overrides.repoprompt_tool_injection, Some(false));
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_missing_workspace() -> Result<()> {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: "/nonexistent/path/RQ-0001".to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-0001"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_dead_pid_with_existing_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0002");
        std::fs::create_dir_all(&workspace_path)?;

        // Spawn a short-lived process and wait for it to exit
        let mut child = std::process::Command::new("true").spawn()?;
        let pid = child.id();
        child.wait()?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(pid),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-0002"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_retains_missing_pid_with_existing_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0003");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0003".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0003".to_string(),
            pid: None,
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert!(dropped.is_empty());
        assert_eq!(state_file.tasks_in_flight.len(), 1);
        assert_eq!(state_file.tasks_in_flight[0].task_id, "RQ-0003");
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_retains_running_pid_with_existing_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0004");
        std::fs::create_dir_all(&workspace_path)?;

        // Spawn a long-running process (sleep) that will still be running
        let child = std::process::Command::new("sleep").arg("10").spawn()?;
        let pid = child.id();

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0004".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0004".to_string(),
            pid: Some(pid),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        // Clean up the child process
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();

        assert!(dropped.is_empty());
        assert_eq!(state_file.tasks_in_flight.len(), 1);
        assert_eq!(state_file.tasks_in_flight[0].task_id, "RQ-0004");
        Ok(())
    }
}
