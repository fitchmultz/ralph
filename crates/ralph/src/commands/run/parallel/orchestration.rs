//! Main parallel orchestration loop.
//!
//! Responsibilities:
//! - Execute the main parallel run loop with worker spawning and monitoring.
//! - Coordinate PR creation on worker success/failure.
//! - Process merge results and trigger workspace cleanup.
//! - Handle graceful shutdown on signals.
//!
//! Not handled here:
//! - State initialization (see `super::state_init`).
//! - State persistence format (see `super::state`).
//! - Worker lifecycle details (see `super::worker`).
//! - Merge runner implementation (see `super::merge_runner`).
//!
//! Invariants/assumptions:
//! - Called after all preflight checks pass.
//! - Queue lock is held by caller for task selection safety.

use crate::config;
use crate::contracts::ParallelMergeWhen;
use crate::queue;
use crate::{git, promptflow, runutil, signal, timeutil};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use super::cleanup_guard::ParallelCleanupGuard;
use super::merge_runner::{MergeQueueSource, MergeResult, MergeWorkItem};
use super::state;
use super::sync::{commit_failure_changes, ensure_branch_pushed, sync_ralph_state};
use super::worker::{WorkerState, collect_excluded_ids, select_next_task_locked, spawn_worker};
use super::{
    ParallelRunOptions, apply_git_commit_push_policy_to_parallel_settings, apply_merge_queue_sync,
    can_start_more_tasks, effective_in_flight_count, initial_tasks_started,
    load_or_init_parallel_state, overrides_for_parallel_workers,
    preflight_parallel_workspace_root_is_gitignored, prune_stale_tasks_in_flight,
    resolve_parallel_settings, spawn_worker_with_registered_workspace,
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
    let state_file = load_or_init_parallel_state(
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

    let merge_stop = Arc::new(AtomicBool::new(false));

    let (pr_tx, pr_rx) = mpsc::channel::<MergeWorkItem>();
    let (merge_result_tx, merge_result_rx) = mpsc::channel::<MergeResult>();
    let mut merge_handle = None;
    // Build merge work items from open/unmerged PRs, filtering out blocked ones
    let existing_work_items: Vec<MergeWorkItem> = state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
        .filter(|record| {
            // Skip PRs with merge blockers
            if let Some(ref blocker) = record.merge_blocker {
                log::warn!(
                    "Skipping PR {} for task {} due to merge blocker: {}",
                    record.pr_number,
                    record.task_id,
                    blocker
                );
                return false;
            }
            true
        })
        .map(|record| {
            let fallback_head = format!("{}{}", settings.branch_prefix, record.task_id);
            let pr = record.pr_info(&fallback_head, &base_branch);
            MergeWorkItem {
                task_id: record.task_id.clone(),
                pr,
                workspace_path: record.workspace_path.as_ref().map(PathBuf::from),
            }
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
            super::merge_runner::run_merge_runner(
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
    let mut created_work_items: Vec<MergeWorkItem> = existing_work_items.clone();

    // Only track workspaces for open/unmerged PRs (closed/merged should not drive merge behavior)
    for record in state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
    {
        let path = record
            .workspace_path()
            .unwrap_or_else(|| settings.workspace_root.join(&record.task_id));
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

    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AsCreated {
        for work_item in &existing_work_items {
            if let Err(e) = pr_tx.send(work_item.clone()) {
                log::debug!(
                    "Failed to send existing work item for task {} to merge runner: {}",
                    work_item.task_id,
                    e
                );
            }
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
    let mut guard = ParallelCleanupGuard::new(
        Arc::clone(&merge_stop),
        pr_tx,
        merge_handle,
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
            let now = time::OffsetDateTime::now_utc();

            let pruned_in_flight = prune_stale_tasks_in_flight(guard.state_file_mut());
            let pruned_finished_without_pr = guard.state_file_mut().prune_finished_without_pr(
                now,
                settings.auto_pr,
                settings.draft_on_failure,
            );

            if !pruned_in_flight.is_empty() || !pruned_finished_without_pr.is_empty() {
                if !pruned_in_flight.is_empty() {
                    log::warn!(
                        "Dropping stale in-flight tasks during loop: {}",
                        pruned_in_flight.join(", ")
                    );
                }
                if !pruned_finished_without_pr.is_empty() {
                    log::info!(
                        "Dropping non-blocking finished-without-PR records during loop: {}",
                        pruned_finished_without_pr.join(", ")
                    );
                }
                state::save_state(&state_path, guard.state_file())?;
            }

            // Spawn new workers until capacity or max-tasks reached.
            while effective_in_flight_count(guard.state_file(), guard.in_flight().len())
                < settings.workers as usize
                && can_start_more_tasks(tasks_started, opts.max_tasks)
                && !stop_requested
            {
                let now = time::OffsetDateTime::now_utc();
                let excluded = collect_excluded_ids(
                    guard.state_file(),
                    guard.in_flight(),
                    now,
                    settings.auto_pr,
                    settings.draft_on_failure,
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

            // Drain merge results for cleanup.
            while let Ok(result) = merge_result_rx.try_recv() {
                if result.merged {
                    apply_merge_queue_sync(resolved, &result)?;
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
                } else {
                    super::persist_merge_blocker_from_result(
                        &state_path,
                        guard.state_file_mut(),
                        &result,
                    )?;
                }
            }

            // Poll workers using the guard's poll_workers method
            let finished = guard.poll_workers();

            for (task_id, task_title, workspace, status) in finished {
                tasks_attempted += 1;
                let mut no_pr_reason: Option<state::ParallelNoPrReason> = None;
                let mut no_pr_message: Option<String> = None;
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
                                    let blocker_msg = format!(
                                        "PR head '{}' does not match expected '{}'. \
                                         Branch prefix may have changed.",
                                        pr.head, expected_head
                                    );
                                    log::warn!(
                                        "PR {} for task {} has mismatched head: {}. \
                                         Setting merge blocker and skipping auto-merge.",
                                        pr.number,
                                        task_id,
                                        blocker_msg
                                    );
                                    // Update the PR record with the blocker
                                    if let Some(record) = guard
                                        .state_file_mut()
                                        .prs
                                        .iter_mut()
                                        .find(|r| r.task_id == task_id)
                                    {
                                        record.merge_blocker = Some(blocker_msg);
                                        if let Err(e) =
                                            state::save_state(&state_path, guard.state_file())
                                        {
                                            log::debug!(
                                                "Failed to save state after setting merge blocker for {}: {}",
                                                task_id,
                                                e
                                            );
                                        }
                                    }
                                } else {
                                    let work_item = MergeWorkItem {
                                        task_id: task_id.clone(),
                                        pr: pr.clone(),
                                        workspace_path: Some(workspace.path.clone()),
                                    };
                                    created_work_items.push(work_item.clone());
                                    if settings.auto_merge
                                        && settings.merge_when == ParallelMergeWhen::AsCreated
                                        && let Some(tx) = guard.pr_tx()
                                        && let Err(e) = tx.send(work_item)
                                    {
                                        log::debug!(
                                            "Failed to send work item for task {} to merge runner: {}",
                                            task_id,
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                no_pr_reason = Some(state::ParallelNoPrReason::PrCreateFailed);
                                no_pr_message = Some(e.to_string());
                                log::warn!("Failed to create PR for {}: {}", task_id, e);
                            }
                        }
                    } else {
                        no_pr_reason = Some(state::ParallelNoPrReason::AutoPrDisabled);
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
                                no_pr_reason =
                                    Some(state::ParallelNoPrReason::DraftPrSkippedNoChanges);
                                no_pr_message = Some(
                                    "worker failed with no changes; skipping draft PR".to_string(),
                                );
                            }
                            Err(e) => {
                                no_pr_reason = Some(state::ParallelNoPrReason::PrCreateFailed);
                                no_pr_message = Some(e.to_string());
                                log::warn!("Failed to create draft PR for {}: {}", task_id, e);
                            }
                        }
                    } else if !settings.auto_pr {
                        no_pr_reason = Some(state::ParallelNoPrReason::AutoPrDisabled);
                    } else {
                        no_pr_reason = Some(state::ParallelNoPrReason::DraftPrDisabled);
                    }
                }

                if guard.state_file().has_pr_record(&task_id) {
                    if guard.state_file_mut().remove_finished_without_pr(&task_id) {
                        state::save_state(&state_path, guard.state_file())?;
                    }
                } else {
                    let reason = no_pr_reason.unwrap_or(state::ParallelNoPrReason::Unknown);
                    super::record_finished_without_pr(
                        &state_path,
                        guard.state_file_mut(),
                        &task_id,
                        &workspace,
                        status.success(),
                        reason,
                        no_pr_message,
                    )?;
                }

                // Move workspace to completed_workspaces for potential merge cleanup
                completed_workspaces.insert(task_id.clone(), workspace.clone());
                guard.state_file_mut().remove_task(&task_id);
                state::save_state(&state_path, guard.state_file())?;
                guard.remove_worker(&task_id);
            }

            if guard.in_flight().is_empty() {
                let no_more_tasks = opts.max_tasks != 0 && tasks_started >= opts.max_tasks;
                let now = time::OffsetDateTime::now_utc();
                let excluded = collect_excluded_ids(
                    guard.state_file(),
                    guard.in_flight(),
                    now,
                    settings.auto_pr,
                    settings.draft_on_failure,
                );
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
        // The guard owns the merge_stop signal, pr_tx, merge_handle, and state
        // When it drops, it will:
        // 1. Signal merge runner to stop
        // 2. Drop pr_tx to unblock receiver
        // 3. Join merge runner thread
        // 4. Terminate in-flight workers
        // 5. Clear and persist state

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

    // Success path - perform normal cleanup, then disarm guard
    // Drop pr_tx to signal merge runner to stop
    drop(guard.take_pr_tx());

    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AfterAll {
        let merge_result_tx = merge_result_tx.clone();
        super::merge_runner::run_merge_runner(
            resolved,
            settings.merge_method,
            settings.conflict_policy,
            settings.merge_runner.clone(),
            settings.merge_retries,
            MergeQueueSource::AfterAll(created_work_items.clone()),
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
            apply_merge_queue_sync(resolved, &result)?;
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
        } else {
            super::persist_merge_blocker_from_result(&state_path, guard.state_file_mut(), &result)?;
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
}
