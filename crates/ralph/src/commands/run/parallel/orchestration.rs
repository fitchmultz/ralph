//! Main parallel orchestration loop for direct-push mode.
//!
//! Responsibilities:
//! - Execute the main parallel run loop with worker spawning and monitoring.
//! - Track worker lifecycle and outcomes.
//! - Handle graceful shutdown on signals.
//!
//! Not handled here:
//! - State initialization (see `super::state_init`).
//! - State persistence format (see `super::state`).
//! - Worker lifecycle details (see `super::worker`).
//! - Integration loop logic (see `integration.rs`).
//!
//! Invariants/assumptions:
//! - Called after all preflight checks pass.
//! - Queue lock is held by caller for task selection safety.
//! - Workers push directly to target branch (no PRs).

use crate::config;
use crate::queue;
use crate::{git, runutil, signal, timeutil};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use super::cleanup_guard::ParallelCleanupGuard;
use super::state::{self, WorkerLifecycle, WorkerRecord};
use super::sync::sync_ralph_state;
use super::worker::{WorkerState, collect_excluded_ids, select_next_task_locked, spawn_worker};
use super::workspace_cleanup::remove_workspace_best_effort;
use super::{
    ParallelRunOptions, can_start_more_tasks, effective_active_worker_count, initial_tasks_started,
    load_or_init_parallel_state, overrides_for_parallel_workers,
    preflight_parallel_workspace_root_is_gitignored, prune_stale_workers,
    resolve_parallel_settings, spawn_worker_with_registered_workspace,
};

fn should_exit_when_idle(
    tasks_started: u32,
    max_tasks: u32,
    next_available: bool,
    stop_requested: bool,
) -> bool {
    let no_more_tasks = max_tasks != 0 && tasks_started >= max_tasks;
    no_more_tasks || !next_available || stop_requested
}

fn summarize_block_reason(reason: &str) -> String {
    let first_line = reason.lines().next().unwrap_or(reason).trim();
    const MAX_REASON_LEN: usize = 180;
    if first_line.len() <= MAX_REASON_LEN {
        return first_line.to_string();
    }
    let mut truncated = first_line
        .chars()
        .take(MAX_REASON_LEN - 3)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn announce_blocked_tasks_at_loop_start(
    queue_file: &crate::contracts::QueueFile,
    state_file: &state::ParallelStateFile,
) {
    let queued_ids: HashSet<&str> = queue_file
        .tasks
        .iter()
        .map(|task| task.id.trim())
        .filter(|task_id| !task_id.is_empty())
        .collect();

    let blocked_workers: Vec<&WorkerRecord> = state_file
        .workers
        .iter()
        .filter(|worker| worker.lifecycle == WorkerLifecycle::BlockedPush)
        .filter(|worker| queued_ids.contains(worker.task_id.trim()))
        .collect();

    if blocked_workers.is_empty() {
        return;
    }

    log::warn!(
        "Parallel loop start: {} queued task(s) are in blocked_push and will be skipped until retried.",
        blocked_workers.len()
    );
    for worker in blocked_workers {
        let reason = worker
            .last_error
            .as_deref()
            .map(summarize_block_reason)
            .unwrap_or_else(|| "No failure reason recorded".to_string());
        log::warn!(
            "Blocked task {} (attempts: {}) reason: {}",
            worker.task_id,
            worker.push_attempts,
            reason
        );
    }
    log::warn!("Use `ralph run parallel retry --task <TASK_ID>` to retry a blocked task.");
}

/// Main entry point for parallel run loop.
pub(crate) fn run_loop_parallel(
    resolved: &config::Resolved,
    opts: ParallelRunOptions,
) -> Result<()> {
    // Acquire the queue lock for the entire parallel run loop
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "run loop", opts.force)?;

    // Preflight: require a clean repo
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    let cache_dir = resolved.repo_root.join(".ralph/cache");

    // Ctrl-C handler setup
    let ctrlc_result = crate::runner::ctrlc_state();
    if let Ok(ctrlc) = ctrlc_result {
        if ctrlc.interrupted.load(Ordering::SeqCst) {
            return Err(runutil::RunAbort::new(
                runutil::RunAbortReason::Interrupted,
                "Ctrl+C was pressed before parallel execution started",
            )
            .into());
        }
        ctrlc.interrupted.store(false, Ordering::SeqCst);
    }

    signal::clear_stop_signal_at_loop_start(&cache_dir);

    // Preflight: explicitly repair timestamp maintenance, persist it, and validate queue/done
    let (queue_file, _done_file) = queue::repair_and_validate_queues(resolved, true)
        .context("Parallel preflight: validate queue/done set")?;

    // Preflight: validate workspace mapping
    super::path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        &resolved.repo_root,
        &resolved.queue_path,
        "queue",
    )
    .with_context(|| {
        "Parallel preflight: queue.file must be under repo root (try a repo-relative path like '.ralph/queue.jsonc')".to_string()
    })?;

    super::path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        &resolved.repo_root,
        &resolved.done_path,
        "done",
    )
    .with_context(|| {
        "Parallel preflight: queue.done_file must be under repo root (try a repo-relative path like '.ralph/done.jsonc')".to_string()
    })?;

    let settings = resolve_parallel_settings(resolved, &opts)?;

    // Preflight: workspace_root gitignore check
    preflight_parallel_workspace_root_is_gitignored(&resolved.repo_root, &settings.workspace_root)?;

    // Preflight: parallel workspaces require a pushable origin remote
    git::origin_urls(&resolved.repo_root).context(
        "Parallel preflight: origin remote check failed (parallel mode requires `origin`)",
    )?;

    if settings.workers < 2 {
        bail!(
            "Parallel run requires workers >= 2 (got {})",
            settings.workers
        );
    }

    let current_branch = git::current_branch(&resolved.repo_root)?;
    let state_path = state::state_file_path(&resolved.repo_root);
    let started_at = timeutil::now_utc_rfc3339_or_fallback();
    let state_file = load_or_init_parallel_state(
        &resolved.repo_root,
        &state_path,
        &current_branch,
        &started_at,
        &settings,
    )?;
    announce_blocked_tasks_at_loop_start(&queue_file, &state_file);

    let target_branch = state_file.target_branch.clone();

    // Initialize webhook worker
    crate::webhook::init_worker_for_parallel(&resolved.config.agent.webhook, settings.workers);

    // Emit loop_started webhook
    let loop_start_time = std::time::Instant::now();
    let loop_webhook_ctx = crate::webhook::WebhookContext {
        repo_root: Some(resolved.repo_root.display().to_string()),
        branch: Some(current_branch.clone()),
        commit: crate::session::get_git_head_commit(&resolved.repo_root),
        ..Default::default()
    };
    crate::webhook::notify_loop_started(
        &resolved.config.agent.webhook,
        &started_at,
        loop_webhook_ctx.clone(),
    );

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let worker_overrides = overrides_for_parallel_workers(resolved, &opts.agent_overrides);

    let mut tasks_started: u32 = initial_tasks_started(&state_file);
    let mut tasks_attempted: usize = 0;
    let mut tasks_succeeded: usize = 0;
    let mut tasks_failed: usize = 0;
    let mut interrupted = false;
    let mut attempted_task_ids: HashSet<String> = HashSet::new();

    // Create cleanup guard
    let mut guard = ParallelCleanupGuard::new_simple(
        state_path.clone(),
        state_file,
        settings.workspace_root.clone(),
    );

    let mut stop_requested: bool = false;

    // Run the main loop
    let loop_result: Result<()> = (|| {
        loop {
            // Check for Ctrl-C interrupt
            if ctrlc_result
                .as_ref()
                .is_ok_and(|ctrlc| ctrlc.interrupted.load(Ordering::SeqCst))
            {
                interrupted = true;
                log::info!("Ctrl+C detected; stopping parallel run and cleaning up.");
                break;
            }

            // Check for stop signal
            if !stop_requested && signal::stop_signal_exists(&cache_dir) {
                stop_requested = true;
                log::info!("Stop signal detected; no new tasks will be started.");
            }

            // Prune stale workers
            let pruned_workers = prune_stale_workers(guard.state_file_mut());
            if !pruned_workers.is_empty() {
                log::warn!("Pruned stale workers: {}", pruned_workers.join(", "));
                state::save_state(&state_path, guard.state_file())?;
            }

            // Spawn new workers until capacity
            while effective_active_worker_count(guard.state_file(), guard.in_flight().len())
                < settings.workers as usize
                && can_start_more_tasks(tasks_started, opts.max_tasks)
                && !stop_requested
            {
                let excluded = collect_excluded_ids(
                    guard.state_file(),
                    guard.in_flight(),
                    &attempted_task_ids,
                );
                let (task_id, task_title) = match select_next_task_locked(
                    resolved,
                    include_draft,
                    &excluded,
                    &_queue_lock,
                )? {
                    Some(task) => task,
                    None => break,
                };

                let (workspace, child) = spawn_worker_with_registered_workspace(
                    &mut guard,
                    &task_id,
                    || {
                        let workspace = git::create_workspace_at(
                            &resolved.repo_root,
                            &settings.workspace_root,
                            &task_id,
                            &target_branch,
                        )?;
                        Ok(workspace)
                    },
                    |path| sync_ralph_state(resolved, path),
                    |workspace| {
                        spawn_worker(
                            resolved,
                            &workspace.path,
                            &task_id,
                            &target_branch,
                            &worker_overrides,
                            opts.force,
                        )
                    },
                )?;

                let task_started_at = timeutil::now_utc_rfc3339_or_fallback();
                let record = WorkerRecord::new(&task_id, workspace.path.clone(), task_started_at);
                guard.state_file_mut().upsert_worker(record);
                state::save_state(&state_path, guard.state_file())?;

                guard.register_worker(
                    task_id.clone(),
                    WorkerState {
                        task_id: task_id.clone(),
                        task_title: task_title.clone(),
                        workspace: workspace.clone(),
                        child,
                    },
                );
                attempted_task_ids.insert(task_id);

                tasks_started += 1;
            }

            // Poll workers
            let finished = guard.poll_workers();

            for (task_id, _task_title, workspace, status) in finished {
                tasks_attempted += 1;

                if status.success() {
                    tasks_succeeded += 1;

                    // Worker completed phases successfully
                    // Note: The integration loop (rebase, conflict resolution, push)
                    // is handled by the worker process itself in direct-push mode.
                    // We just track the outcome here.

                    if let Some(worker) = guard.state_file_mut().get_worker_mut(&task_id) {
                        worker.mark_completed(timeutil::now_utc_rfc3339_or_fallback());
                    }

                    log::info!("Worker {} completed successfully", task_id);
                } else {
                    tasks_failed += 1;

                    let blocked_marker =
                        match super::integration::read_blocked_push_marker(&workspace.path) {
                            Ok(marker) => marker,
                            Err(err) => {
                                log::warn!(
                                    "Failed reading blocked marker for {} ({}): {}",
                                    task_id,
                                    workspace.path.display(),
                                    err
                                );
                                None
                            }
                        };

                    if let Some(marker) = blocked_marker {
                        if let Some(worker) = guard.state_file_mut().get_worker_mut(&task_id) {
                            worker.push_attempts = marker.attempt;
                            worker.mark_blocked(
                                timeutil::now_utc_rfc3339_or_fallback(),
                                marker.reason.clone(),
                            );
                        }

                        log::warn!(
                            "Worker {} blocked after {}/{} integration attempts: {}",
                            task_id,
                            marker.attempt,
                            marker.max_attempts,
                            marker.reason
                        );
                        log::warn!(
                            "Retaining blocked workspace for retry: {}",
                            workspace.path.display()
                        );
                    } else {
                        if let Some(worker) = guard.state_file_mut().get_worker_mut(&task_id) {
                            worker.mark_failed(
                                timeutil::now_utc_rfc3339_or_fallback(),
                                format!("Worker exited with status: {:?}", status.code()),
                            );
                        }

                        log::warn!(
                            "Worker {} failed with exit status: {:?}",
                            task_id,
                            status.code()
                        );

                        // Clean up failed worker workspace
                        remove_workspace_best_effort(
                            &settings.workspace_root,
                            &workspace,
                            "worker failure",
                        );
                    }
                }

                state::save_state(&state_path, guard.state_file())?;
                guard.remove_worker(&task_id);
            }

            // Check if we should exit
            if guard.in_flight().is_empty() {
                let excluded = collect_excluded_ids(
                    guard.state_file(),
                    guard.in_flight(),
                    &attempted_task_ids,
                );
                let next_available =
                    select_next_task_locked(resolved, include_draft, &excluded, &_queue_lock)?
                        .is_some();

                if should_exit_when_idle(
                    tasks_started,
                    opts.max_tasks,
                    next_available,
                    stop_requested,
                ) {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(500));
        }

        Ok(())
    })();

    // Handle cleanup on exit
    if interrupted || loop_result.is_err() {
        let loop_stopped_at = crate::timeutil::now_utc_rfc3339_or_fallback();
        let loop_duration_ms = loop_start_time.elapsed().as_millis() as u64;
        let loop_note = if interrupted {
            Some("Parallel run interrupted by Ctrl+C".to_string())
        } else {
            loop_result.as_ref().err().map(|e| e.to_string())
        };
        crate::webhook::notify_loop_stopped(
            &resolved.config.agent.webhook,
            &loop_stopped_at,
            crate::webhook::WebhookContext {
                duration_ms: Some(loop_duration_ms),
                ..loop_webhook_ctx
            },
            loop_note.as_deref(),
        );

        if interrupted {
            return Err(runutil::RunAbort::new(
                runutil::RunAbortReason::Interrupted,
                "Parallel run interrupted by Ctrl+C",
            )
            .into());
        }
        return loop_result;
    }

    // Success path
    guard.mark_completed();

    // Clear stop signal
    if (stop_requested || signal::stop_signal_exists(&cache_dir))
        && let Err(e) = signal::clear_stop_signal(&cache_dir)
    {
        log::warn!("Failed to clear stop signal: {}", e);
    }

    if tasks_attempted > 0 {
        let notify_config = crate::notification::build_notification_config(
            &resolved.config.agent.notification,
            &crate::notification::NotificationOverrides {
                notify_on_complete: opts.agent_overrides.notify_on_complete,
                notify_on_fail: opts.agent_overrides.notify_on_fail,
                notify_sound: opts.agent_overrides.notify_sound,
            },
        );
        crate::notification::notify_loop_complete(
            tasks_attempted,
            tasks_succeeded,
            tasks_failed,
            &notify_config,
        );
    }

    // Emit loop_stopped webhook
    let loop_stopped_at = crate::timeutil::now_utc_rfc3339_or_fallback();
    let loop_duration_ms = loop_start_time.elapsed().as_millis() as u64;
    let loop_note = Some(format!(
        "Parallel run completed: {}/{} succeeded, {} failed",
        tasks_succeeded, tasks_attempted, tasks_failed
    ));
    crate::webhook::notify_loop_stopped(
        &resolved.config.agent.webhook,
        &loop_stopped_at,
        crate::webhook::WebhookContext {
            duration_ms: Some(loop_duration_ms),
            ..loop_webhook_ctx
        },
        loop_note.as_deref(),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentOverrides;
    use crate::config;
    use crate::contracts::Config;

    #[test]
    fn overrides_for_parallel_workers_forces_repoprompt_off() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
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
    fn should_exit_when_idle_unbounded_continues_if_task_available() {
        assert!(
            !should_exit_when_idle(42, 0, true, false),
            "unbounded max_tasks should continue when next task is available"
        );
    }

    #[test]
    fn should_exit_when_idle_unbounded_stops_if_no_task_available() {
        assert!(
            should_exit_when_idle(42, 0, false, false),
            "unbounded max_tasks should stop only when queue selection has no runnable task"
        );
    }

    #[test]
    fn should_exit_when_idle_bounded_stops_at_limit_even_if_task_available() {
        assert!(
            should_exit_when_idle(5, 5, true, false),
            "bounded max_tasks should stop once task cap is reached"
        );
    }
}
