//! Main parallel orchestration loop.
//!
//! Responsibilities:
//! - Execute the main parallel run loop with worker spawning and monitoring.
//! - Coordinate PR creation on worker success/failure.
//! - Queue and process merge-agent subprocess jobs.
//! - Handle graceful shutdown on signals.
//!
//! Not handled here:
//! - State initialization (see `super::state_init`).
//! - State persistence format (see `super::state`).
//! - Worker lifecycle details (see `super::worker`).
//! - Merge-agent execution logic (see `super::merge_agent`).
//!
//! Invariants/assumptions:
//! - Called after all preflight checks pass.
//! - Queue lock is held by caller for task selection safety.
//! - Merge-agent subprocess runs synchronously in the main loop.
//!
//! # Runtime Policies (Spec Section 20)
//!
//! ## Policy 2: Unresolved Conflict Handling
//!
//! When `MergeExitClassification::ConflictRetryable` is returned:
//! - PR is left open (not closed, not deleted)
//! - Failure is persisted as retryable (`update_merge_result` with `retryable=true`)
//! - Task is requeued via `requeue_merge` for later retry
//! - Main loop continues (does NOT abort)
//!
//! ## Policy 3: Workspace Retention
//!
//! On successful merge (`Success` or `AlreadyFinalized` classification):
//! - Workspace is deleted immediately via `std::fs::remove_dir_all`
//! - Deletion failure is non-fatal (logged as warning, not error)
//! - Cleanup happens in both `AsCreated` and `AfterAll` modes
//!
//! These policies are fixed per `docs/features/parallel-mode-rewrite.md` section 20.

use crate::config;
use crate::contracts::ParallelMergeWhen;
use crate::queue;
use crate::{git, promptflow, runutil, signal, timeutil};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use super::cleanup_guard::ParallelCleanupGuard;
use super::merge_runner::MergeWorkItem;
use super::state::{self, PendingMergeJob, PendingMergeLifecycle};
use super::sync::{commit_failure_changes, ensure_branch_pushed, sync_ralph_state};
use super::worker::{WorkerState, collect_excluded_ids, select_next_task_locked, spawn_worker};
use super::{
    MergeExitClassification, ParallelRunOptions, apply_git_commit_push_policy_to_parallel_settings,
    can_start_more_tasks, classify_merge_exit_code, effective_in_flight_count,
    initial_tasks_started, load_or_init_parallel_state, overrides_for_parallel_workers,
    preflight_parallel_workspace_root_is_gitignored, prune_stale_tasks_in_flight,
    resolve_parallel_settings, spawn_merge_agent, spawn_worker_with_registered_workspace,
};

/// Main entry point for parallel run loop.
pub(crate) fn run_loop_parallel(
    resolved: &config::Resolved,
    opts: ParallelRunOptions,
) -> Result<()> {
    // Acquire the queue lock for the entire parallel run loop to prevent
    // other run loops from selecting the same tasks during the selection→spawn window.
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "run loop", opts.force)?;

    // Preflight: require a clean repo before creating workspaces/spawning workers.
    // Honor --force to bypass (matches run_one_impl behavior).
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    let cache_dir = resolved.repo_root.join(".ralph/cache");
    // Ctrl-C handler: initialize if not already done. In tests, a previous test may have
    // registered the handler, so we treat "already registered" as a non-fatal condition
    // and skip the pre-run interrupt check in that case.
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

    // Preflight: validate queue/done before doing any parallel orchestration.
    // This fails fast on invalid queue state rather than spawning workers that will fail individually.
    let queue_file =
        queue::load_queue(&resolved.queue_path).context("Parallel preflight: load queue.json")?;
    let done = queue::load_queue_or_default(&resolved.done_path)
        .context("Parallel preflight: load done.json")?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("Parallel preflight: validate queue/done set")?;
    queue::log_warnings(&warnings);

    // Preflight: parallel workspace mapping requires queue/done to be repo-contained.
    // Fail fast here (before state file creation / worker spawn) with an actionable error.
    super::path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        &resolved.repo_root, // validation-only: map into the same root
        &resolved.queue_path,
        "queue",
    )
    .with_context(|| {
        format!(
            "Parallel preflight: queue.file must be under repo root for workspace mapping (try a repo-relative path like '.ralph/queue.json'). repo_root={}, queue_path={}",
            resolved.repo_root.display(),
            resolved.queue_path.display()
        )
    })?;

    super::path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        &resolved.repo_root, // validation-only: map into the same root
        &resolved.done_path,
        "done",
    )
    .with_context(|| {
        format!(
            "Parallel preflight: queue.done_file must be under repo root for workspace mapping (try a repo-relative path like '.ralph/done.json'). repo_root={}, done_path={}",
            resolved.repo_root.display(),
            resolved.done_path.display()
        )
    })?;

    let mut settings = resolve_parallel_settings(resolved, &opts)?;

    // Preflight: fail fast if workspace_root is inside repo but not gitignored
    preflight_parallel_workspace_root_is_gitignored(&resolved.repo_root, &settings.workspace_root)?;

    // Compute effective git_commit_push_enabled with same precedence as run_one_impl
    let effective_git_commit_push = opts
        .agent_overrides
        .git_commit_push_enabled
        .or(resolved.config.agent.git_commit_push_enabled)
        .unwrap_or(true);

    // Disable PR automation if commit/push is disabled (PRs require pushed commits)
    if !effective_git_commit_push {
        log::warn!(
            "Parallel mode: git commit/push is disabled. Disabling PR automation (auto_pr, auto_merge, draft_on_failure) for this invocation."
        );
        apply_git_commit_push_policy_to_parallel_settings(&mut settings, false);
    }

    // Preflight: if PR automation is enabled, verify gh CLI is available and authenticated.
    // This fails fast with a clear error rather than running tasks and failing at PR creation.
    if settings.auto_pr || settings.auto_merge {
        git::check_gh_available().context(
            "Parallel preflight: gh CLI check failed (auto_pr or auto_merge is enabled)",
        )?;
    }

    // Preflight: parallel workspaces require a pushable origin remote.
    // Fail fast before state file creation / worker spawn.
    let _ = git::origin_urls(&resolved.repo_root).context(
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
    let mut state_file = load_or_init_parallel_state(
        &resolved.repo_root,
        &state_path,
        &current_branch,
        &started_at,
        &mut settings,
    )?;

    let base_branch = state_file.base_branch.clone();

    // Emit loop_started webhook after preflights pass
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

    // Reconcile existing open/unmerged PRs into pending_merges queue for AsCreated mode.
    // This handles resumed runs where PRs were created but not yet merged.
    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AsCreated {
        // Collect records to reconcile first to avoid borrow issues
        let records_to_enqueue: Vec<(String, u32)> = state_file
            .prs
            .iter()
            .filter(|record| record.is_open_unmerged())
            .filter(|record| state_file.get_pending_merge(&record.task_id).is_none())
            .map(|record| (record.task_id.clone(), record.pr_number))
            .collect();

        for (task_id, pr_number) in records_to_enqueue {
            let merge_job = PendingMergeJob {
                task_id: task_id.clone(),
                pr_number,
                workspace_path: None,
                lifecycle: PendingMergeLifecycle::Queued,
                attempts: 0,
                queued_at: timeutil::now_utc_rfc3339_or_fallback(),
                last_error: None,
            };
            state_file.enqueue_merge(merge_job);
            log::info!(
                "Reconciled existing PR {} for task {} into pending_merges queue",
                pr_number,
                task_id
            );
        }
    }

    // Track completed workspaces separately (not owned by guard, cleaned up after merge)
    let mut completed_workspaces: HashMap<String, git::WorkspaceSpec> = HashMap::new();
    // Track work items for AfterAll mode
    let mut after_all_work_items: Vec<MergeWorkItem> = Vec::new();

    // Only track workspaces for open/unmerged PRs (closed/merged should not drive merge behavior)
    for record in state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
    {
        let path = settings.workspace_root.join(&record.task_id);
        if path.exists() {
            completed_workspaces.insert(
                record.task_id.clone(),
                git::WorkspaceSpec {
                    path,
                    branch: format!("{}{}", settings.branch_prefix, record.task_id),
                },
            );
        }
    }

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let worker_overrides = overrides_for_parallel_workers(resolved, &opts.agent_overrides);
    // Count resumed in-flight tasks toward max_tasks to prevent over-starting on resume.
    let now = time::OffsetDateTime::now_utc();
    let mut tasks_started: u32 = initial_tasks_started(
        &state_file,
        now,
        settings.auto_pr,
        settings.draft_on_failure,
    );
    let mut tasks_attempted: usize = 0;
    let mut tasks_succeeded: usize = 0;
    let mut tasks_failed: usize = 0;
    let mut interrupted = false;

    // Create cleanup guard to ensure resources are cleaned up on any exit path
    // Note: merge-agent subprocess architecture no longer needs merge_stop/pr_tx/merge_handle
    let mut guard = ParallelCleanupGuard::new_simple(
        state_path.clone(),
        state_file,
        settings.workspace_root.clone(),
    );

    // Track whether stop signal has been observed to avoid repeated logging
    let mut stop_requested: bool = false;

    // Run the main loop inside a closure so we can handle cleanup on any error
    let loop_result: Result<()> = (|| {
        loop {
            // Check for Ctrl-C interrupt (if handler was successfully registered)
            if ctrlc_result
                .as_ref()
                .is_ok_and(|ctrlc| ctrlc.interrupted.load(Ordering::SeqCst))
            {
                interrupted = true;
                log::info!("Ctrl+C detected; stopping parallel run and cleaning up.");
                break;
            }

            // Check for stop signal once per loop iteration
            if !stop_requested && signal::stop_signal_exists(&cache_dir) {
                stop_requested = true;
                log::info!("Stop signal detected; no new tasks will be started.");
            }

            // Periodically prune stale records to free capacity on resumed work.
            let pruned_in_flight = prune_stale_tasks_in_flight(guard.state_file_mut());

            if !pruned_in_flight.is_empty() {
                log::warn!(
                    "Dropping stale in-flight tasks during loop: {}",
                    pruned_in_flight.join(", ")
                );
                state::save_state(&state_path, guard.state_file())?;
            }

            // Spawn new workers until capacity or max-tasks reached.
            while effective_in_flight_count(guard.state_file(), guard.in_flight().len())
                < settings.workers as usize
                && can_start_more_tasks(tasks_started, opts.max_tasks)
                && !stop_requested
            {
                let excluded = collect_excluded_ids(guard.state_file(), guard.in_flight());
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
                        git::create_workspace_at(
                            &resolved.repo_root,
                            &settings.workspace_root,
                            &task_id,
                            &base_branch,
                            &settings.branch_prefix,
                        )
                    },
                    |path| sync_ralph_state(resolved, path),
                    |workspace| {
                        spawn_worker(
                            resolved,
                            &workspace.path,
                            &task_id,
                            &worker_overrides,
                            opts.force,
                        )
                    },
                )?;

                let task_started_at = timeutil::now_utc_rfc3339_or_fallback();
                let record = state::ParallelTaskRecord::new(
                    &task_id,
                    &workspace,
                    child.id(),
                    Some(task_started_at),
                );
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

                tasks_started += 1;
            }

            // Process one pending merge job per iteration (if auto_merge and AsCreated mode).
            // This replaces the background merge-runner thread with inline subprocess dispatch.
            if settings.auto_merge
                && settings.merge_when == ParallelMergeWhen::AsCreated
                && !stop_requested
                && let Some(merge_job) = guard.state_file().next_queued_merge().cloned()
            {
                let task_id = merge_job.task_id.clone();
                let pr_number = merge_job.pr_number;

                log::info!("Invoking merge-agent for task {} PR {}", task_id, pr_number);

                // Mark as in-progress before spawning
                guard.state_file_mut().mark_merge_in_progress(&task_id);
                state::save_state(&state_path, guard.state_file())?;

                match spawn_merge_agent(&resolved.repo_root, &task_id, pr_number) {
                    Ok(outcome) => {
                        let classification = classify_merge_exit_code(outcome.exit_code);

                        match classification {
                            MergeExitClassification::Success
                            | MergeExitClassification::AlreadyFinalized => {
                                log::info!(
                                    "Merge-agent succeeded for task {} PR {} (exit={})",
                                    task_id,
                                    pr_number,
                                    outcome.exit_code
                                );

                                // Update PR lifecycle
                                guard.state_file_mut().mark_pr_merged(&task_id);

                                // Delete workspace immediately per spec
                                if let Some(ws_path) = &merge_job.workspace_path {
                                    if let Err(e) = std::fs::remove_dir_all(ws_path) {
                                        log::warn!(
                                            "Failed to delete workspace {} for {}: {}",
                                            ws_path.display(),
                                            task_id,
                                            e
                                        );
                                    } else {
                                        log::info!(
                                            "Deleted workspace {} for {}",
                                            ws_path.display(),
                                            task_id
                                        );
                                    }
                                }

                                // Remove pending merge job
                                guard.state_file_mut().remove_pending_merge(&task_id);

                                // Update completed_workspaces tracking
                                completed_workspaces.remove(&task_id);

                                state::save_state(&state_path, guard.state_file())?;
                            }

                            MergeExitClassification::ConflictRetryable => {
                                log::warn!(
                                    "Merge-agent conflict for task {} PR {}: {}",
                                    task_id,
                                    pr_number,
                                    outcome.stderr_output.lines().next().unwrap_or("unknown")
                                );

                                // Per spec: leave PR open, persist retryable, continue loop
                                guard.state_file_mut().update_merge_result(
                                    &task_id,
                                    false,
                                    Some(format!("Merge conflict: {}", outcome.stderr_output)),
                                    true, // retryable
                                );

                                // Reset to Queued so it can be retried later
                                guard.state_file_mut().requeue_merge(&task_id);

                                state::save_state(&state_path, guard.state_file())?;
                            }

                            MergeExitClassification::RuntimeRetryable => {
                                log::warn!(
                                    "Merge-agent runtime error for task {} PR {}: {}",
                                    task_id,
                                    pr_number,
                                    outcome.stderr_output
                                );

                                guard.state_file_mut().update_merge_result(
                                    &task_id,
                                    false,
                                    Some(outcome.stderr_output.clone()),
                                    true, // retryable
                                );

                                // Check retry limit
                                let max_retries = settings.merge_retries;
                                if let Some(job) =
                                    guard.state_file_mut().get_pending_merge_mut(&task_id)
                                {
                                    if job.attempts >= max_retries {
                                        log::error!(
                                            "Merge-agent exhausted retries for task {} after {} attempts",
                                            task_id,
                                            job.attempts
                                        );
                                        job.lifecycle = PendingMergeLifecycle::TerminalFailed;
                                    } else {
                                        // Reset to Queued so it can be retried
                                        job.lifecycle = PendingMergeLifecycle::Queued;
                                    }
                                }

                                state::save_state(&state_path, guard.state_file())?;
                            }

                            MergeExitClassification::TerminalFailure => {
                                log::error!(
                                    "Merge-agent terminal failure for task {} PR {}: exit {} - {}",
                                    task_id,
                                    pr_number,
                                    outcome.exit_code,
                                    outcome.stderr_output
                                );

                                guard.state_file_mut().mark_merge_terminal_failed(
                                    &task_id,
                                    outcome.stderr_output.clone(),
                                );

                                state::save_state(&state_path, guard.state_file())?;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to spawn merge-agent for task {} PR {}: {}",
                            task_id,
                            pr_number,
                            e
                        );

                        guard.state_file_mut().update_merge_result(
                            &task_id,
                            false,
                            Some(e.to_string()),
                            true, // retryable - subprocess spawn failure
                        );
                        guard.state_file_mut().requeue_merge(&task_id);
                        state::save_state(&state_path, guard.state_file())?;
                    }
                }
            }

            // Poll workers using the guard's poll_workers method
            let finished = guard.poll_workers();

            for (task_id, task_title, workspace, status) in finished {
                tasks_attempted += 1;
                if status.success() {
                    tasks_succeeded += 1;
                    // Handle success
                    if settings.auto_pr {
                        match (|| -> Result<git::PrInfo> {
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
                            Ok(pr)
                        })() {
                            Ok(pr) => {
                                // Validate PR head matches expected naming before enqueueing
                                let expected_head =
                                    format!("{}{}", settings.branch_prefix, task_id);
                                if pr.head.trim() != expected_head {
                                    log::warn!(
                                        "PR {} for task {} has mismatched head '{}', expected '{}'. \
                                         Skipping auto-merge; user can merge manually.",
                                        pr.number,
                                        task_id,
                                        pr.head.trim(),
                                        expected_head
                                    );
                                } else {
                                    // Track for AfterAll mode
                                    let work_item = MergeWorkItem {
                                        task_id: task_id.clone(),
                                        pr: pr.clone(),
                                        workspace_path: Some(workspace.path.clone()),
                                    };
                                    after_all_work_items.push(work_item);

                                    // Queue merge job for AsCreated mode (new architecture)
                                    if settings.auto_merge
                                        && settings.merge_when == ParallelMergeWhen::AsCreated
                                    {
                                        let merge_job = PendingMergeJob {
                                            task_id: task_id.clone(),
                                            pr_number: pr.number,
                                            workspace_path: Some(workspace.path.clone()),
                                            lifecycle: PendingMergeLifecycle::Queued,
                                            attempts: 0,
                                            queued_at: timeutil::now_utc_rfc3339_or_fallback(),
                                            last_error: None,
                                        };
                                        guard.state_file_mut().enqueue_merge(merge_job);
                                        state::save_state(&state_path, guard.state_file())?;
                                        log::info!(
                                            "Queued merge job for task {} PR {}",
                                            task_id,
                                            pr.number
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to create PR for {}: {}", task_id, e);
                            }
                        }
                    }
                } else {
                    tasks_failed += 1;
                    // Handle failure
                    if settings.auto_pr && settings.draft_on_failure {
                        match (|| -> Result<Option<git::PrInfo>> {
                            if !commit_failure_changes(&workspace.path, &task_id)? {
                                return Ok(None);
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
                            Ok(Some(pr))
                        })() {
                            Ok(Some(pr)) => {
                                log::info!(
                                    "Draft PR {} created for {}; skipping auto-merge.",
                                    pr.number,
                                    task_id
                                );
                            }
                            Ok(None) => {
                                log::info!(
                                    "Worker for {} failed with no changes; skipping draft PR",
                                    task_id
                                );
                            }
                            Err(e) => {
                                log::warn!("Failed to create draft PR for {}: {}", task_id, e);
                            }
                        }
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
                    select_next_task_locked(resolved, include_draft, &excluded, &_queue_lock)?
                        .is_some();
                // Exit if: max tasks reached, no more tasks available, or stop requested
                if no_more_tasks || !next_available || stop_requested {
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
        // The guard will:
        // 1. Terminate in-flight workers
        // 2. Clear and persist state

        // Emit loop_stopped webhook on error/interrupt path
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

    // Success path - handle AfterAll merge mode with merge-agent subprocess
    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AfterAll {
        // Enqueue all work items as pending merges
        for work_item in &after_all_work_items {
            let merge_job = PendingMergeJob {
                task_id: work_item.task_id.clone(),
                pr_number: work_item.pr.number,
                workspace_path: work_item.workspace_path.clone(),
                lifecycle: PendingMergeLifecycle::Queued,
                attempts: 0,
                queued_at: timeutil::now_utc_rfc3339_or_fallback(),
                last_error: None,
            };
            guard.state_file_mut().enqueue_merge(merge_job);
        }
        state::save_state(&state_path, guard.state_file())?;

        log::info!(
            "Processing {} queued merges in AfterAll mode",
            guard.state_file().pending_merge_count()
        );

        // Process all pending merges
        while guard.state_file().has_queued_merges() {
            if let Some(merge_job) = guard.state_file().next_queued_merge().cloned() {
                let task_id = merge_job.task_id.clone();
                let pr_number = merge_job.pr_number;

                log::info!(
                    "AfterAll: Invoking merge-agent for task {} PR {}",
                    task_id,
                    pr_number
                );

                guard.state_file_mut().mark_merge_in_progress(&task_id);
                state::save_state(&state_path, guard.state_file())?;

                match spawn_merge_agent(&resolved.repo_root, &task_id, pr_number) {
                    Ok(outcome) => {
                        let classification = classify_merge_exit_code(outcome.exit_code);

                        match classification {
                            MergeExitClassification::Success
                            | MergeExitClassification::AlreadyFinalized => {
                                log::info!(
                                    "AfterAll: Merge-agent succeeded for task {} PR {} (exit={})",
                                    task_id,
                                    pr_number,
                                    outcome.exit_code
                                );

                                guard.state_file_mut().mark_pr_merged(&task_id);

                                if let Some(ws_path) = &merge_job.workspace_path
                                    && let Err(e) = std::fs::remove_dir_all(ws_path)
                                {
                                    log::warn!(
                                        "Failed to delete workspace {} for {}: {}",
                                        ws_path.display(),
                                        task_id,
                                        e
                                    );
                                }

                                guard.state_file_mut().remove_pending_merge(&task_id);
                                completed_workspaces.remove(&task_id);
                                state::save_state(&state_path, guard.state_file())?;
                            }
                            MergeExitClassification::ConflictRetryable => {
                                log::warn!(
                                    "AfterAll: Merge-agent conflict for task {} PR {}: {}",
                                    task_id,
                                    pr_number,
                                    outcome.stderr_output.lines().next().unwrap_or("unknown")
                                );
                                guard.state_file_mut().update_merge_result(
                                    &task_id,
                                    false,
                                    Some(format!("Merge conflict: {}", outcome.stderr_output)),
                                    true,
                                );
                                guard.state_file_mut().requeue_merge(&task_id);
                                state::save_state(&state_path, guard.state_file())?;
                                // Continue to next merge job
                            }
                            MergeExitClassification::RuntimeRetryable => {
                                log::warn!(
                                    "AfterAll: Merge-agent runtime error for task {} PR {}: {}",
                                    task_id,
                                    pr_number,
                                    outcome.stderr_output
                                );
                                guard.state_file_mut().update_merge_result(
                                    &task_id,
                                    false,
                                    Some(outcome.stderr_output.clone()),
                                    true,
                                );

                                let max_retries = settings.merge_retries;
                                if let Some(job) =
                                    guard.state_file_mut().get_pending_merge_mut(&task_id)
                                {
                                    if job.attempts >= max_retries {
                                        log::error!(
                                            "AfterAll: Merge-agent exhausted retries for task {} after {} attempts",
                                            task_id,
                                            job.attempts
                                        );
                                        job.lifecycle = PendingMergeLifecycle::TerminalFailed;
                                    } else {
                                        job.lifecycle = PendingMergeLifecycle::Queued;
                                    }
                                }
                                state::save_state(&state_path, guard.state_file())?;
                            }
                            MergeExitClassification::TerminalFailure => {
                                log::error!(
                                    "AfterAll: Merge-agent terminal failure for task {} PR {}: exit {} - {}",
                                    task_id,
                                    pr_number,
                                    outcome.exit_code,
                                    outcome.stderr_output
                                );
                                guard.state_file_mut().mark_merge_terminal_failed(
                                    &task_id,
                                    outcome.stderr_output.clone(),
                                );
                                state::save_state(&state_path, guard.state_file())?;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "AfterAll: Failed to spawn merge-agent for task {} PR {}: {}",
                            task_id,
                            pr_number,
                            e
                        );
                        guard.state_file_mut().update_merge_result(
                            &task_id,
                            false,
                            Some(e.to_string()),
                            true,
                        );
                        guard.state_file_mut().requeue_merge(&task_id);
                        state::save_state(&state_path, guard.state_file())?;
                    }
                }
            }
        }
    }

    // All cleanup successful - disarm the guard
    guard.mark_completed();

    // Clear stop signal on successful exit if it was observed (or still exists)
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

    // Emit loop_stopped webhook on success path
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
    use crate::commands::run::merge_agent::exit_codes;
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

    // =========================================================================
    // Policy Regression Tests (Spec Section 20)
    // =========================================================================

    /// Regression test: ConflictRetryable classification maps to exit code 3.
    ///
    /// Per spec section 20, decision 2: "Unresolved conflict policy: leave PR open,
    /// persist retryable failure, and continue loop execution."
    ///
    /// The classification function must map exit code 3 to ConflictRetryable.
    #[test]
    fn conflict_exit_code_classifies_as_retryable() {
        assert_eq!(
            classify_merge_exit_code(exit_codes::MERGE_CONFLICT),
            MergeExitClassification::ConflictRetryable
        );
    }

    /// Regression test: Success classification triggers workspace deletion logic.
    ///
    /// Per spec section 20, decision 3: "Workspace retention policy: delete workspace
    /// immediately after successful merge finalization."
    ///
    /// This test verifies the classification path that triggers deletion.
    #[test]
    fn success_classification_triggers_deletion_path() {
        // Success (exit 0) and AlreadyFinalized (exit 6) both trigger the
        // deletion path in the orchestration match block.
        assert_eq!(
            classify_merge_exit_code(exit_codes::SUCCESS),
            MergeExitClassification::Success
        );
        assert_eq!(
            classify_merge_exit_code(exit_codes::ALREADY_FINALIZED),
            MergeExitClassification::AlreadyFinalized
        );

        // Both Success and AlreadyFinalized are NOT retryable (they're done)
        assert!(!matches!(
            MergeExitClassification::Success,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));
        assert!(!matches!(
            MergeExitClassification::AlreadyFinalized,
            MergeExitClassification::ConflictRetryable | MergeExitClassification::RuntimeRetryable
        ));
    }

    /// Regression test: ConflictRetryable path does NOT call remove_pending_merge.
    ///
    /// When a conflict occurs, the merge job should be requeued, not removed.
    /// The `requeue_merge` function resets lifecycle to Queued.
    #[test]
    fn conflict_path_requeues_not_removes() {
        use super::state::{ParallelStateFile, PendingMergeJob, PendingMergeLifecycle};
        use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};

        let mut state_file = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // Add a pending merge job
        let merge_job = PendingMergeJob {
            task_id: "RQ-0001".to_string(),
            pr_number: 42,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::InProgress,
            attempts: 1,
            queued_at: "2026-02-01T00:00:00Z".to_string(),
            last_error: Some("Merge conflict".to_string()),
        };
        state_file.enqueue_merge(merge_job);

        // Simulate conflict path: mark in progress, then requeue
        state_file.mark_merge_in_progress("RQ-0001");
        state_file.update_merge_result("RQ-0001", false, Some("Merge conflict".to_string()), true);
        state_file.requeue_merge("RQ-0001");

        // Verify the job still exists (was NOT removed)
        let job = state_file.get_pending_merge("RQ-0001");
        assert!(
            job.is_some(),
            "Conflict path should retain pending merge job"
        );
        let job = job.unwrap();
        assert_eq!(job.lifecycle, PendingMergeLifecycle::Queued);
    }

    /// Regression test: Success path removes pending merge job.
    ///
    /// When merge succeeds, the pending merge job should be removed from the queue.
    #[test]
    fn success_path_removes_pending_merge() {
        use super::state::{ParallelStateFile, PendingMergeJob, PendingMergeLifecycle};
        use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};

        let mut state_file = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // Add a pending merge job
        let merge_job = PendingMergeJob {
            task_id: "RQ-0002".to_string(),
            pr_number: 43,
            workspace_path: None,
            lifecycle: PendingMergeLifecycle::Queued,
            attempts: 0,
            queued_at: "2026-02-01T00:00:00Z".to_string(),
            last_error: None,
        };
        state_file.enqueue_merge(merge_job);

        // Simulate success path: mark PR merged, remove pending merge
        state_file.mark_pr_merged("RQ-0002");
        state_file.remove_pending_merge("RQ-0002");

        // Verify the job was removed
        let job = state_file.get_pending_merge("RQ-0002");
        assert!(
            job.is_none(),
            "Success path should remove pending merge job"
        );
    }

    /// Regression test: Conflict path does NOT mark PR as merged.
    ///
    /// Per spec section 20, decision 2: PR should remain open on conflict.
    #[test]
    fn conflict_path_leaves_pr_open() {
        use super::state::{ParallelPrLifecycle, ParallelPrRecord, ParallelStateFile};
        use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};

        let mut state_file = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // Add a PR record
        state_file.prs.push(ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 44,
            lifecycle: ParallelPrLifecycle::Open,
        });

        // Simulate conflict path: update_merge_result and requeue (no mark_pr_merged)
        state_file.update_merge_result("RQ-0003", false, Some("Conflict".to_string()), true);
        state_file.requeue_merge("RQ-0003");

        // Verify PR is still Open, not Merged
        let pr_record = state_file.prs.iter().find(|r| r.task_id == "RQ-0003");
        assert!(pr_record.is_some());
        assert_eq!(
            pr_record.unwrap().lifecycle,
            ParallelPrLifecycle::Open,
            "Conflict path should leave PR open"
        );
    }
}
