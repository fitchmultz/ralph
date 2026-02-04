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
use crate::contracts::{
    ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, ParallelMergeWhen, QueueFile,
};
use crate::{fsutil, git, promptflow, runutil, signal, timeutil};
use anyhow::{Context, Result, bail};
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

use crate::queue;
use cleanup_guard::ParallelCleanupGuard;
use merge_runner::{MergeQueueSource, MergeResult, MergeWorkItem};
use sync::{commit_failure_changes, ensure_branch_pushed, sync_ralph_state};
use worker::{WorkerState, collect_excluded_ids, select_next_task_locked, spawn_worker};

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
    path_map::map_resolved_path_into_workspace(
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

    path_map::map_resolved_path_into_workspace(
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
            let _ = pr_tx.send(work_item.clone());
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
                                        let _ = state::save_state(&state_path, guard.state_file());
                                    }
                                } else {
                                    let work_item = MergeWorkItem {
                                        task_id: task_id.clone(),
                                        pr: pr.clone(),
                                    };
                                    created_work_items.push(work_item.clone());
                                    if settings.auto_merge
                                        && settings.merge_when == ParallelMergeWhen::AsCreated
                                        && let Some(tx) = guard.pr_tx()
                                    {
                                        let _ = tx.send(work_item);
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
                    record_finished_without_pr(
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

fn load_or_init_parallel_state(
    repo_root: &Path,
    state_path: &Path,
    current_branch: &str,
    started_at: &str,
    settings: &mut ParallelSettings,
) -> Result<state::ParallelStateFile> {
    let current_branch = current_branch.trim();
    if let Some(mut existing) = state::load_state(state_path)? {
        let dropped_tasks = prune_stale_tasks_in_flight(&mut existing);
        if !dropped_tasks.is_empty() {
            log::warn!(
                "Dropping stale in-flight tasks: {}",
                dropped_tasks.join(", ")
            );
            state::save_state(state_path, &existing)?;
        }

        // Reconcile PR records against current GitHub state
        let summary = state::reconcile_pr_records(repo_root, &mut existing)?;
        if summary.has_changes() {
            log::info!(
                "Reconciled PR records: {} closed, {} merged, {} errors",
                summary.closed_count,
                summary.merged_count,
                summary.error_count
            );
            state::save_state(state_path, &existing)?;
        }

        // Prune non-blocking finished-without-PR records on load
        let now = time::OffsetDateTime::now_utc();
        let dropped_finished_without_pr =
            existing.prune_finished_without_pr(now, settings.auto_pr, settings.draft_on_failure);
        if !dropped_finished_without_pr.is_empty() {
            log::info!(
                "Dropping non-blocking finished-without-PR records on load: {}",
                dropped_finished_without_pr.join(", ")
            );
            state::save_state(state_path, &existing)?;
        }

        // Validate PR heads match expected naming convention and persist blockers
        validate_and_block_mismatched_prs(&mut existing, &settings.branch_prefix);
        state::save_state(state_path, &existing)?;

        let mut normalized = false;
        let trimmed_base = existing.base_branch.trim().to_string();
        if trimmed_base != existing.base_branch {
            existing.base_branch = trimmed_base;
            normalized = true;
        }
        if existing.started_at.trim().is_empty() {
            existing.started_at = started_at.to_string();
            normalized = true;
        }

        let in_flight = in_flight_task_ids(&existing);
        let blocking_prs = blocking_pr_task_ids(&existing);
        let finished_without_pr = finished_without_pr_task_ids(&existing);

        if existing.base_branch.is_empty() {
            if in_flight.is_empty() && blocking_prs.is_empty() && finished_without_pr.is_empty() {
                log::warn!(
                    "Parallel state base branch missing; populating from current branch '{}'.",
                    current_branch
                );
                existing.base_branch = current_branch.to_string();
                existing.started_at = started_at.to_string();
                normalized = true;
            } else {
                bail!(format_base_branch_missing_error(
                    state_path,
                    current_branch,
                    &in_flight,
                    &blocking_prs,
                    &finished_without_pr
                ));
            }
        } else if existing.base_branch != current_branch {
            if in_flight.is_empty() && blocking_prs.is_empty() && finished_without_pr.is_empty() {
                log::warn!(
                    "Parallel state base branch '{}' does not match current branch '{}'; retargeting state at {}.",
                    existing.base_branch,
                    current_branch,
                    state_path.display()
                );
                existing.base_branch = current_branch.to_string();
                existing.started_at = started_at.to_string();
                normalized = true;
            } else {
                bail!(format_base_branch_mismatch_error(
                    state_path,
                    &existing.base_branch,
                    current_branch,
                    &in_flight,
                    &blocking_prs,
                    &finished_without_pr
                ));
            }
        }

        if normalized {
            state::save_state(state_path, &existing)?;
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

        Ok(existing)
    } else {
        let state = state::ParallelStateFile::new(
            started_at.to_string(),
            current_branch.to_string(),
            settings.merge_method,
            settings.merge_when,
        );
        state::save_state(state_path, &state)?;
        Ok(state)
    }
}

fn in_flight_task_ids(state_file: &state::ParallelStateFile) -> Vec<String> {
    state_file
        .tasks_in_flight
        .iter()
        .map(|record| record.task_id.clone())
        .collect()
}

fn blocking_pr_task_ids(state_file: &state::ParallelStateFile) -> Vec<String> {
    state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
        .map(|record| record.task_id.clone())
        .collect()
}

fn finished_without_pr_task_ids(state_file: &state::ParallelStateFile) -> Vec<String> {
    state_file
        .finished_without_pr
        .iter()
        .map(|record| record.task_id.clone())
        .collect()
}

/// Validates PR heads match expected naming convention and sets merge_blocker for mismatches.
///
/// For each open/unmerged PR, checks if the stored head matches `{branch_prefix}{task_id}`.
/// If not, sets a merge_blocker on the record so the merge runner will skip it.
/// Also clears stale blockers if the head now matches.
fn validate_and_block_mismatched_prs(
    state_file: &mut state::ParallelStateFile,
    branch_prefix: &str,
) {
    for record in state_file.prs.iter_mut() {
        // Only check open, unmerged PRs
        if !record.is_open_unmerged() {
            continue;
        }

        let expected_head = format!("{}{}", branch_prefix, record.task_id);

        if let Some(ref stored_head) = record.head {
            let trimmed_head = stored_head.trim();
            if trimmed_head != expected_head {
                let blocker_msg = format!(
                    "PR head '{}' does not match expected '{}'. \
                     Branch prefix or task_id may have changed.",
                    trimmed_head, expected_head
                );
                log::warn!(
                    "PR {} for task {} has mismatched head: expected '{}', got '{}'. \
                     Setting merge blocker.",
                    record.pr_number,
                    record.task_id,
                    expected_head,
                    trimmed_head
                );
                record.merge_blocker = Some(blocker_msg);
            } else if record.merge_blocker.is_some() {
                // Head matches now, clear any stale blocker
                log::info!(
                    "PR {} for task {} head now matches expected '{}'. \
                     Clearing stale merge blocker.",
                    record.pr_number,
                    record.task_id,
                    expected_head
                );
                record.merge_blocker = None;
            }
        }
    }
}

fn format_base_branch_mismatch_error(
    state_path: &Path,
    recorded_branch: &str,
    current_branch: &str,
    in_flight: &[String],
    blocking_prs: &[String],
    finished_without_pr: &[String],
) -> String {
    let mut blockers = Vec::new();
    if !in_flight.is_empty() {
        blockers.push(format!(
            "- {} in-flight task(s): {}",
            in_flight.len(),
            in_flight.join(", ")
        ));
    }
    if !blocking_prs.is_empty() {
        blockers.push(format!(
            "- {} open PR(s): {}",
            blocking_prs.len(),
            blocking_prs.join(", ")
        ));
    }
    if !finished_without_pr.is_empty() {
        blockers.push(format!(
            "- {} finished-without-PR task(s): {}",
            finished_without_pr.len(),
            finished_without_pr.join(", ")
        ));
    }
    let blocker_text = if blockers.is_empty() {
        "- none".to_string()
    } else {
        blockers.join("\n")
    };

    format!(
        "Parallel state base branch '{}' does not match current branch '{}'.\nState file: {}\nUnsafe to retarget because:\n{}\nRecovery options:\n1) checkout '{}' and resume the parallel run\n2) if you are certain no parallel run is active, delete '{}'",
        recorded_branch,
        current_branch,
        state_path.display(),
        blocker_text,
        recorded_branch,
        state_path.display()
    )
}

fn format_base_branch_missing_error(
    state_path: &Path,
    current_branch: &str,
    in_flight: &[String],
    blocking_prs: &[String],
    finished_without_pr: &[String],
) -> String {
    let mut blockers = Vec::new();
    if !in_flight.is_empty() {
        blockers.push(format!(
            "- {} in-flight task(s): {}",
            in_flight.len(),
            in_flight.join(", ")
        ));
    }
    if !blocking_prs.is_empty() {
        blockers.push(format!(
            "- {} open PR(s): {}",
            blocking_prs.len(),
            blocking_prs.join(", ")
        ));
    }
    if !finished_without_pr.is_empty() {
        blockers.push(format!(
            "- {} finished-without-PR task(s): {}",
            finished_without_pr.len(),
            finished_without_pr.join(", ")
        ));
    }
    let blocker_text = if blockers.is_empty() {
        "- none".to_string()
    } else {
        blockers.join("\n")
    };

    format!(
        "Parallel state base branch is missing.\nState file: {}\nUnsafe to populate from current branch '{}' because:\n{}\nRecovery options:\n1) checkout the original base branch and resume the parallel run\n2) if you are certain no parallel run is active, delete '{}'",
        state_path.display(),
        current_branch,
        blocker_text,
        state_path.display()
    )
}

fn record_finished_without_pr(
    state_path: &Path,
    state_file: &mut state::ParallelStateFile,
    task_id: &str,
    workspace: &git::WorkspaceSpec,
    success: bool,
    reason: state::ParallelNoPrReason,
    message: Option<String>,
) -> Result<()> {
    let record = state::ParallelFinishedWithoutPrRecord::new(
        task_id,
        workspace,
        success,
        timeutil::now_utc_rfc3339_or_fallback(),
        reason.clone(),
        message.clone(),
    );
    state_file.upsert_finished_without_pr(record);
    state::save_state(state_path, state_file)?;
    let reason_label = reason.as_str();
    log::warn!(
        "Task {} finished without PR (reason: {}). Recorded state in {}. \
         This may temporarily block reruns; it automatically clears when PR settings allow reruns or when the TTL expires.",
        task_id,
        reason_label,
        state_path.display()
    );
    if let Some(detail) = message {
        log::info!("Detail for {}: {}", task_id, detail);
    }
    Ok(())
}

/// Parse bytes into a QueueFile using JSONC parsing rules.
///
/// Validates UTF-8 → JSONC parse with descriptive error context.
fn parse_bytes_to_queue_file(bytes: &[u8], task_id: &str, label: &str) -> Result<QueueFile> {
    let raw = std::str::from_utf8(bytes)
        .with_context(|| format!("[{}] {} bytes are not valid UTF-8", task_id, label))?;
    crate::jsonc::parse_jsonc::<QueueFile>(raw, &format!("[{}] parse {} as JSONC", task_id, label))
}

fn apply_merge_queue_sync(resolved: &config::Resolved, result: &MergeResult) -> Result<()> {
    let Some(queue_bytes) = result.queue_bytes.as_ref() else {
        bail!(
            "Merged PR for {} did not return queue bytes; refusing to update local queue.",
            result.task_id
        );
    };

    // Parse queue bytes into QueueFile (validates UTF-8 + JSONC)
    let queue_file = parse_bytes_to_queue_file(queue_bytes, &result.task_id, "queue")?;

    // Parse done bytes if present
    let done_file: Option<QueueFile> = if let Some(done_bytes) = result.done_bytes.as_ref() {
        Some(parse_bytes_to_queue_file(
            done_bytes,
            &result.task_id,
            "done",
        )?)
    } else {
        None
    };

    // Run semantic validation before any disk writes
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    if let Some(ref done) = done_file {
        let warnings = queue::validate_queue_set(
            &queue_file,
            Some(done),
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )
        .with_context(|| {
            format!(
                "[{}] semantic validation failed for queue/done set",
                result.task_id
            )
        })?;
        queue::log_warnings(&warnings);
    } else {
        queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width).with_context(
            || format!("[{}] semantic validation failed for queue", result.task_id),
        )?;
    }

    // Only persist after all validation succeeds
    queue::save_queue(&resolved.queue_path, &queue_file)
        .with_context(|| format!("[{}] persist validated queue", result.task_id))?;

    match done_file {
        Some(done) => {
            queue::save_queue(&resolved.done_path, &done)
                .with_context(|| format!("[{}] persist validated done", result.task_id))?;
        }
        None => {
            if let Err(err) = std::fs::remove_file(&resolved.done_path)
                && err.kind() != std::io::ErrorKind::NotFound
            {
                return Err(err.into());
            }
        }
    }

    // Productivity is written last (only if queue/done validation succeeded)
    if let Some(bytes) = result.productivity_bytes.as_ref() {
        let productivity_path = resolved
            .repo_root
            .join(".ralph")
            .join("cache")
            .join("productivity.json");
        fsutil::write_atomic(&productivity_path, bytes)
            .with_context(|| format!("write productivity bytes for {}", result.task_id))?;
    }

    Ok(())
}

/// Prune stale in-flight tasks from the parallel state file.
///
/// Drops records when:
/// - The workspace path no longer exists, OR
/// - The recorded PID exists and is no longer running (pid_is_running returns Some(false)), OR
/// - PID is missing (None) AND started_at is missing/invalid OR older than the TTL
///
/// Retains records when:
/// - pid_is_running returns None (indeterminate status), OR
/// - PID is missing but started_at is within TTL
///
/// Returns the list of dropped task IDs for logging.
fn prune_stale_tasks_in_flight(state_file: &mut state::ParallelStateFile) -> Vec<String> {
    let now = time::OffsetDateTime::now_utc();
    let ttl_secs: i64 = crate::constants::timeouts::PARALLEL_FINISHED_WITHOUT_PR_BLOCKER_TTL
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);

    let mut dropped = Vec::new();
    state_file.tasks_in_flight.retain(|record| {
        let path = Path::new(&record.workspace_path);
        if !path.exists() {
            dropped.push(record.task_id.clone());
            return false;
        }

        if let Some(pid) = record.pid {
            if crate::lock::pid_is_running(pid) == Some(false) {
                dropped.push(record.task_id.clone());
                return false;
            }
            // Retain when running or indeterminate (pid_is_running == None).
            return true;
        }

        // PID is missing: time-bound it so it can't block capacity forever.
        let Some(started_at) = timeutil::parse_rfc3339_opt(&record.started_at) else {
            log::warn!(
                "Dropping stale in-flight task {} with missing pid: missing/invalid started_at (workspace: {}).",
                record.task_id,
                record.workspace_path
            );
            dropped.push(record.task_id.clone());
            return false;
        };

        let age_secs = (now.unix_timestamp() - started_at.unix_timestamp()).max(0);
        if age_secs >= ttl_secs {
            log::warn!(
                "Dropping stale in-flight task {} with missing pid after TTL (age_secs={}, ttl_secs={}, started_at='{}', workspace: {}).",
                record.task_id,
                age_secs,
                ttl_secs,
                record.started_at,
                record.workspace_path
            );
            dropped.push(record.task_id.clone());
            return false;
        }

        true
    });
    dropped
}

/// Compute the effective number of tasks in flight for capacity checks.
///
/// Uses the maximum of the persisted state file count and the guard's in-flight
/// count to avoid double-counting while ensuring resumed work is accounted for.
fn effective_in_flight_count(
    state_file: &state::ParallelStateFile,
    guard_in_flight_len: usize,
) -> usize {
    state_file.tasks_in_flight.len().max(guard_in_flight_len)
}

/// Initialize the tasks_started counter from resumed state.
///
/// Returns the number of:
/// - tasks_in_flight records, plus
/// - blocking finished-without-PR records (based on current settings/TTL), plus
/// - open/unmerged PR records (these represent prior completed work still in flight via PR),
///   as u32, capping at u32::MAX.
fn initial_tasks_started(
    state_file: &state::ParallelStateFile,
    now: time::OffsetDateTime,
    auto_pr_enabled: bool,
    draft_on_failure: bool,
) -> u32 {
    let open_unmerged_prs = state_file
        .prs
        .iter()
        .filter(|record| record.is_open_unmerged())
        .count();

    let blocking_finished_without_pr = state_file
        .finished_without_pr
        .iter()
        .filter(|r| r.is_blocking(now, auto_pr_enabled, draft_on_failure))
        .count();

    let total = state_file
        .tasks_in_flight
        .len()
        .saturating_add(blocking_finished_without_pr)
        .saturating_add(open_unmerged_prs);

    u32::try_from(total).unwrap_or(u32::MAX)
}

/// Check if more tasks can be started given the max_tasks limit.
///
/// Returns true if max_tasks is 0 (unlimited) or tasks_started < max_tasks.
fn can_start_more_tasks(tasks_started: u32, max_tasks: u32) -> bool {
    max_tasks == 0 || tasks_started < max_tasks
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

/// Apply git commit/push policy to parallel settings.
/// When git_commit_push_enabled is false, disables PR automation since PRs require pushed commits.
fn apply_git_commit_push_policy_to_parallel_settings(
    settings: &mut ParallelSettings,
    git_commit_push_enabled: bool,
) {
    if !git_commit_push_enabled {
        settings.auto_pr = false;
        settings.auto_merge = false;
        settings.draft_on_failure = false;
    }
}

fn spawn_worker_with_registered_workspace<CreateWorkspace, SyncWorkspace, SpawnWorker>(
    guard: &mut ParallelCleanupGuard,
    task_id: &str,
    create_workspace: CreateWorkspace,
    sync_workspace: SyncWorkspace,
    spawn: SpawnWorker,
) -> Result<(git::WorkspaceSpec, std::process::Child)>
where
    CreateWorkspace: FnOnce() -> Result<git::WorkspaceSpec>,
    SyncWorkspace: FnOnce(&Path) -> Result<()>,
    SpawnWorker: FnOnce(&git::WorkspaceSpec) -> Result<std::process::Child>,
{
    let workspace = create_workspace()?;
    guard.register_workspace(task_id.to_string(), workspace.clone());
    sync_workspace(&workspace.path)?;
    let child = spawn(&workspace)?;
    Ok((workspace, child))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Config;

    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, mpsc};
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
            started_at: "2026-02-02T00:00:00Z".to_string(),
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
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-0002"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_retains_missing_pid_within_ttl() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0003");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the record is within TTL
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

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
            started_at: recent_timestamp,
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert!(dropped.is_empty());
        assert_eq!(state_file.tasks_in_flight.len(), 1);
        assert_eq!(state_file.tasks_in_flight[0].task_id, "RQ-0003");
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_missing_pid_beyond_ttl() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-OLD");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Use a very old timestamp (well beyond the 24h TTL)
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-OLD".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-OLD".to_string(),
            pid: None,
            started_at: "2020-01-01T00:00:00Z".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-OLD"]);
        assert!(state_file.tasks_in_flight.is_empty());
        Ok(())
    }

    #[test]
    fn prune_stale_tasks_drops_missing_pid_with_missing_started_at() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-LEGACY");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Simulate a legacy record with missing started_at (empty string)
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-LEGACY".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-LEGACY".to_string(),
            pid: None,
            started_at: "".to_string(),
        });

        let dropped = prune_stale_tasks_in_flight(&mut state_file);

        assert_eq!(dropped, vec!["RQ-LEGACY"]);
        assert!(state_file.tasks_in_flight.is_empty());
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
            started_at: "2026-02-02T00:00:00Z".to_string(),
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

    fn test_parallel_settings(repo_root: &Path) -> ParallelSettings {
        ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 5,
            workspace_root: repo_root.join("workspaces"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        }
    }

    #[test]
    fn base_branch_mismatch_auto_heals_when_state_empty() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let state_path = state::state_file_path(repo_root);
        let state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            &started_at,
            &mut settings,
        )?;

        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.started_at, started_at);

        let reloaded = state::load_state(&state_path)?.expect("state");
        assert_eq!(reloaded.base_branch, "main");
        assert_eq!(reloaded.started_at, started_at);
        Ok(())
    }

    #[test]
    fn base_branch_missing_auto_heals_when_state_empty() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let state_path = state::state_file_path(repo_root);
        let state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            &started_at,
            &mut settings,
        )?;

        assert_eq!(loaded.base_branch, "main");
        assert_eq!(loaded.started_at, started_at);

        let reloaded = state::load_state(&state_path)?.expect("state");
        assert_eq!(reloaded.base_branch, "main");
        Ok(())
    }

    #[test]
    fn base_branch_missing_errors_when_tasks_in_flight_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_path = repo_root.join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the record is not pruned by TTL
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: None,
            started_at: recent_timestamp,
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("base branch is missing"));
        assert!(msg.contains("in-flight"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    #[test]
    fn base_branch_mismatch_errors_when_tasks_in_flight_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let workspace_path = repo_root.join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the record is not pruned by TTL
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: None,
            started_at: recent_timestamp,
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("Parallel state base branch"));
        assert!(msg.contains("in-flight"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    fn create_test_cleanup_guard(temp: &TempDir) -> ParallelCleanupGuard {
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).expect("create workspace root");

        let state_path = temp.path().join("state.json");
        let state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        let (pr_tx, _pr_rx) = mpsc::channel::<MergeWorkItem>();
        let merge_stop = Arc::new(AtomicBool::new(false));

        ParallelCleanupGuard::new(
            merge_stop,
            pr_tx,
            None,
            state_path,
            state_file,
            workspace_root,
        )
    }

    #[test]
    fn spawn_failure_cleans_registered_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_cleanup_guard(&temp);
        let workspace_root = temp.path().join("workspaces");
        let workspace_path = workspace_root.join("RQ-0001");

        let result = spawn_worker_with_registered_workspace(
            &mut guard,
            "RQ-0001",
            || {
                std::fs::create_dir_all(&workspace_path)?;
                Ok(git::WorkspaceSpec {
                    path: workspace_path.clone(),
                    branch: "ralph/RQ-0001".to_string(),
                })
            },
            |_| Ok(()),
            |_| Err(anyhow::anyhow!("spawn failed")),
        );

        assert!(result.is_err());
        guard.cleanup()?;
        assert!(!workspace_path.exists());
        Ok(())
    }

    #[test]
    fn sync_failure_cleans_registered_workspace_without_spawning() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_cleanup_guard(&temp);
        let workspace_root = temp.path().join("workspaces");
        let workspace_path = workspace_root.join("RQ-0002");
        let spawn_called = Cell::new(false);

        let result = spawn_worker_with_registered_workspace(
            &mut guard,
            "RQ-0002",
            || {
                std::fs::create_dir_all(&workspace_path)?;
                Ok(git::WorkspaceSpec {
                    path: workspace_path.clone(),
                    branch: "ralph/RQ-0002".to_string(),
                })
            },
            |_| Err(anyhow::anyhow!("sync failed")),
            |_| {
                spawn_called.set(true);
                Err(anyhow::anyhow!("spawn should not run"))
            },
        );

        assert!(result.is_err());
        assert!(!spawn_called.get());
        guard.cleanup()?;
        assert!(!workspace_path.exists());
        Ok(())
    }

    #[test]
    fn validate_and_block_mismatched_prs_sets_blocker() {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0001".to_string(),
            pr_number: 42,
            pr_url: "https://example.com/pr/42".to_string(),
            head: Some("feature/RQ-0001".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Open,
            merge_blocker: None,
        });

        validate_and_block_mismatched_prs(&mut state_file, "ralph/");

        let blocker = state_file.prs[0]
            .merge_blocker
            .as_ref()
            .expect("expected merge blocker");
        assert!(blocker.contains("does not match expected"));
    }

    #[test]
    fn validate_and_block_mismatched_prs_clears_stale_blocker() {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0002".to_string(),
            pr_number: 43,
            pr_url: "https://example.com/pr/43".to_string(),
            head: Some("ralph/RQ-0002".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Open,
            merge_blocker: Some("stale".to_string()),
        });

        validate_and_block_mismatched_prs(&mut state_file, "ralph/");

        assert!(state_file.prs[0].merge_blocker.is_none());
    }

    #[test]
    fn validate_and_block_mismatched_prs_skips_closed_or_merged() {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 44,
            pr_url: "https://example.com/pr/44".to_string(),
            head: Some("feature/RQ-0003".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Closed,
            merge_blocker: None,
        });
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0004".to_string(),
            pr_number: 45,
            pr_url: "https://example.com/pr/45".to_string(),
            head: Some("feature/RQ-0004".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: true,
            lifecycle: state::ParallelPrLifecycle::Merged,
            merge_blocker: None,
        });

        validate_and_block_mismatched_prs(&mut state_file, "ralph/");

        assert!(state_file.prs[0].merge_blocker.is_none());
        assert!(state_file.prs[1].merge_blocker.is_none());
    }

    #[test]
    fn base_branch_mismatch_prunes_then_auto_heals_when_only_stale_tasks() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: repo_root
                .join("missing/RQ-0002")
                .to_string_lossy()
                .to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let loaded = load_or_init_parallel_state(
            repo_root,
            &state_path,
            "main",
            &started_at,
            &mut settings,
        )?;

        assert!(loaded.tasks_in_flight.is_empty());
        assert_eq!(loaded.base_branch, "main");
        Ok(())
    }

    #[test]
    fn base_branch_mismatch_errors_when_open_prs_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 7,
            pr_url: "https://example.com/pr/7".to_string(),
            head: None,
            base: None,
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Open,
            merge_blocker: None,
        });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("open PR"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    #[test]
    fn base_branch_missing_errors_when_finished_without_pr_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();

        // Create the workspace directory so the record is considered blocking
        let workspace_path = repo_root.join("workspaces").join("RQ-0008");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the TTL check passes (within 24 hours)
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-0008".to_string(),
                workspace_path: workspace_path.to_string_lossy().to_string(),
                branch: "ralph/RQ-0008".to_string(),
                success: true,
                finished_at: recent_timestamp,
                // Use PrCreateFailed so it blocks regardless of auto_pr setting (within TTL)
                reason: state::ParallelNoPrReason::PrCreateFailed,
                message: None,
            });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("base branch is missing"));
        assert!(msg.contains("finished-without-PR"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    #[test]
    fn base_branch_mismatch_errors_when_finished_without_pr_present() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();

        // Create the workspace directory so the record is considered blocking
        let workspace_path = repo_root.join("workspaces").join("RQ-0009");
        std::fs::create_dir_all(&workspace_path)?;

        // Use a recent timestamp so the TTL check passes (within 24 hours)
        let recent_timestamp = timeutil::now_utc_rfc3339_or_fallback();

        let mut state = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "old".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-0009".to_string(),
                workspace_path: workspace_path.to_string_lossy().to_string(),
                branch: "ralph/RQ-0009".to_string(),
                success: true,
                finished_at: recent_timestamp,
                // Use PrCreateFailed so it blocks regardless of auto_pr setting (within TTL)
                reason: state::ParallelNoPrReason::PrCreateFailed,
                message: None,
            });
        let state_path = state::state_file_path(repo_root);
        state::save_state(&state_path, &state)?;

        let started_at = "2026-02-03T00:00:00Z".to_string();
        let mut settings = test_parallel_settings(repo_root);
        let err =
            load_or_init_parallel_state(repo_root, &state_path, "main", &started_at, &mut settings)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("Parallel state base branch"));
        assert!(msg.contains("finished-without-PR"));
        assert!(msg.contains("state.json"));
        Ok(())
    }

    #[test]
    fn resume_in_flight_counts_toward_max_tasks() -> Result<()> {
        use crate::timeutil;

        let temp = TempDir::new()?;
        let ws_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&ws_root)?;

        // Create workspace directories so records are considered blocking
        let ws1 = ws_root.join("RQ-0001");
        let ws2 = ws_root.join("RQ-0002");
        let ws3 = ws_root.join("RQ-0003");
        std::fs::create_dir_all(&ws1)?;
        std::fs::create_dir_all(&ws2)?;
        std::fs::create_dir_all(&ws3)?;

        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z")?;

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Simulate 2 tasks in flight from resumed state
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: ws1.to_string_lossy().to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: ws2.to_string_lossy().to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12346),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        // AutoPrDisabled only counts as started when auto_pr is still disabled
        state_file
            .finished_without_pr
            .push(state::ParallelFinishedWithoutPrRecord {
                task_id: "RQ-0003".to_string(),
                workspace_path: ws3.to_string_lossy().to_string(),
                branch: "ralph/RQ-0003".to_string(),
                success: true,
                finished_at: "2026-02-01T02:00:00Z".to_string(),
                reason: state::ParallelNoPrReason::AutoPrDisabled,
                message: None,
            });

        // With auto_pr disabled, all 3 count as started
        assert_eq!(initial_tasks_started(&state_file, now, false, true), 3);

        // With auto_pr enabled, AutoPrDisabled records don't block, so only 2 count
        assert_eq!(initial_tasks_started(&state_file, now, true, true), 2);

        // With max_tasks = 2, should not be able to start more (when auto_pr is disabled)
        assert!(!can_start_more_tasks(3, 2));

        // With max_tasks = 3, should not be able to start more (when auto_pr is disabled)
        assert!(!can_start_more_tasks(3, 3));

        // With max_tasks = 4, should be able to start more
        assert!(can_start_more_tasks(3, 4));

        // With max_tasks = 0 (unlimited), should be able to start more
        assert!(can_start_more_tasks(2, 0));

        Ok(())
    }

    #[test]
    fn resume_open_prs_count_toward_max_tasks() {
        use crate::timeutil;

        let now = timeutil::parse_rfc3339("2026-02-03T00:00:00Z").unwrap();

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // One open/unmerged PR from a previous run should count as "already started"
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0100".to_string(),
            pr_number: 1,
            pr_url: "https://example.com/pr/1".to_string(),
            head: Some("ralph/RQ-0100".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Open,
            merge_blocker: None,
        });

        // These should NOT count toward started (they are not open+unmerged)
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0101".to_string(),
            pr_number: 2,
            pr_url: "https://example.com/pr/2".to_string(),
            head: Some("ralph/RQ-0101".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: false,
            lifecycle: state::ParallelPrLifecycle::Closed,
            merge_blocker: None,
        });
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0102".to_string(),
            pr_number: 3,
            pr_url: "https://example.com/pr/3".to_string(),
            head: Some("ralph/RQ-0102".to_string()),
            base: Some("main".to_string()),
            workspace_path: None,
            merged: true,
            lifecycle: state::ParallelPrLifecycle::Merged,
            merge_blocker: None,
        });

        let started = initial_tasks_started(&state_file, now, true, true);
        assert_eq!(started, 1);

        // With max_tasks=1, we should NOT be allowed to start any new tasks on resume.
        assert!(!can_start_more_tasks(started, 1));

        // With max_tasks=2, we can start one more.
        assert!(can_start_more_tasks(started, 2));
    }

    #[test]
    fn resume_in_flight_counts_toward_worker_capacity() {
        let state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        // Test with tasks_in_flight.len() == 2 and guard_in_flight_len == 0
        let state_with_tasks = {
            let mut s = state_file.clone();
            s.tasks_in_flight.push(state::ParallelTaskRecord {
                task_id: "RQ-0001".to_string(),
                workspace_path: "/tmp/ws/RQ-0001".to_string(),
                branch: "ralph/RQ-0001".to_string(),
                pid: Some(12345),
                started_at: "2026-02-02T00:00:00Z".to_string(),
            });
            s.tasks_in_flight.push(state::ParallelTaskRecord {
                task_id: "RQ-0002".to_string(),
                workspace_path: "/tmp/ws/RQ-0002".to_string(),
                branch: "ralph/RQ-0002".to_string(),
                pid: Some(12346),
                started_at: "2026-02-02T00:00:00Z".to_string(),
            });
            s
        };

        // effective_in_flight_count should return 2 (from state file)
        assert_eq!(effective_in_flight_count(&state_with_tasks, 0), 2);

        // With workers_limit == 2, has_capacity should be false
        let has_capacity = effective_in_flight_count(&state_with_tasks, 0) < 2;
        assert!(!has_capacity);

        // With workers_limit == 3, has_capacity should be true
        let has_capacity = effective_in_flight_count(&state_with_tasks, 0) < 3;
        assert!(has_capacity);
    }

    #[test]
    fn capacity_does_not_double_count_guard_and_state() {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: "/tmp/ws/RQ-0001".to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: "/tmp/ws/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12346),
            started_at: "2026-02-02T00:00:00Z".to_string(),
        });

        // With tasks_in_flight.len() == 2 and guard_in_flight_len == 1,
        // effective_in_flight_count should return 2 (max, not sum)
        assert_eq!(effective_in_flight_count(&state_file, 1), 2);

        // With tasks_in_flight.len() == 2 and guard_in_flight_len() == 3,
        // effective_in_flight_count should return 3 (max, not sum)
        assert_eq!(effective_in_flight_count(&state_file, 3), 3);
    }

    // ============================================================================
    // Stop signal idle-stop exit tests (RQ-0570)
    // ============================================================================

    /// Test helper: determine if the loop should break based on current state
    /// Mirrors the logic in the main loop for testing purposes
    fn should_exit_loop(
        stop_requested: bool,
        in_flight_is_empty: bool,
        no_more_tasks: bool,
        next_available: bool,
    ) -> bool {
        if in_flight_is_empty {
            // Exit if: max tasks reached, no more tasks available, or stop requested
            no_more_tasks || !next_available || stop_requested
        } else {
            // Don't exit if workers are still in flight
            false
        }
    }

    #[test]
    fn stop_requested_and_idle_should_exit() {
        // stop_requested=true, in_flight_is_empty=true, next_available=true => break
        assert!(should_exit_loop(true, true, false, true));
    }

    #[test]
    fn stop_requested_with_in_flight_should_not_exit() {
        // stop_requested=true, in_flight_is_empty=false => do not break (wait for in-flight)
        assert!(!should_exit_loop(true, false, false, true));
        assert!(!should_exit_loop(true, false, true, false));
        assert!(!should_exit_loop(true, false, true, true));
    }

    #[test]
    fn no_stop_no_next_available_should_exit() {
        // stop_requested=false, in_flight_is_empty=true, next_available=false => break
        assert!(should_exit_loop(false, true, false, false));
    }

    #[test]
    fn no_stop_no_more_tasks_should_exit() {
        // stop_requested=false, in_flight_is_empty=true, no_more_tasks=true => break
        assert!(should_exit_loop(false, true, true, false));
    }

    #[test]
    fn normal_operation_should_not_exit() {
        // stop_requested=false, in_flight_is_empty=true, next_available=true => continue
        assert!(!should_exit_loop(false, true, false, true));
    }

    #[test]
    fn stop_signal_cleared_on_parallel_loop_exit() {
        use crate::signal;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join(".ralph/cache");

        // Create stop signal
        signal::create_stop_signal(&cache_dir).unwrap();
        assert!(signal::stop_signal_exists(&cache_dir));

        // Clear it (simulating what the parallel loop does on exit)
        let cleared = signal::clear_stop_signal(&cache_dir).unwrap();
        assert!(cleared);
        assert!(!signal::stop_signal_exists(&cache_dir));
    }

    #[test]
    fn apply_git_commit_push_policy_leaves_settings_unchanged_when_enabled() {
        let mut settings = ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 5,
            workspace_root: PathBuf::from("/tmp/workspaces"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        };

        // When git_commit_push_enabled is true, settings should remain unchanged
        apply_git_commit_push_policy_to_parallel_settings(&mut settings, true);

        assert!(settings.auto_pr);
        assert!(settings.auto_merge);
        assert!(settings.draft_on_failure);
    }

    #[test]
    fn apply_git_commit_push_policy_disables_pr_automation_when_disabled() {
        let mut settings = ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 5,
            workspace_root: PathBuf::from("/tmp/workspaces"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        };

        // When git_commit_push_enabled is false, PR automation should be disabled
        apply_git_commit_push_policy_to_parallel_settings(&mut settings, false);

        assert!(!settings.auto_pr);
        assert!(!settings.auto_merge);
        assert!(!settings.draft_on_failure);
    }

    // Tests for apply_merge_queue_sync validation (RQ-0578)

    use std::fs;

    fn build_test_resolved_for_merge_tests(
        repo_root: &Path,
        queue_path: PathBuf,
        done_path: PathBuf,
    ) -> config::Resolved {
        config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn apply_merge_queue_sync_rejects_invalid_queue_bytes() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");
        let productivity_path = cache_dir.join("productivity.json");

        // Create sentinel files
        let sentinel_queue = "{\"version\":1,\"tasks\":[]}";
        let sentinel_done = "{\"version\":1,\"tasks\":[]}";
        let sentinel_productivity = "{}";
        fs::write(&queue_path, sentinel_queue).unwrap();
        fs::write(&done_path, sentinel_done).unwrap();
        fs::write(&productivity_path, sentinel_productivity).unwrap();

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(b"not valid json".to_vec()),
            done_bytes: Some(sentinel_done.as_bytes().to_vec()),
            productivity_bytes: Some(sentinel_productivity.as_bytes().to_vec()),
        };

        // Should return error for invalid queue bytes
        let err = apply_merge_queue_sync(&resolved, &result).unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("parse queue as JSONC") || err_msg.contains("queue"),
            "Error should mention queue parsing: {}",
            err_msg
        );

        // Verify sentinel files unchanged
        assert_eq!(fs::read_to_string(&queue_path).unwrap(), sentinel_queue);
        assert_eq!(fs::read_to_string(&done_path).unwrap(), sentinel_done);
        assert_eq!(
            fs::read_to_string(&productivity_path).unwrap(),
            sentinel_productivity
        );
    }

    #[test]
    fn apply_merge_queue_sync_rejects_invalid_done_bytes() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");
        let productivity_path = cache_dir.join("productivity.json");

        // Create sentinel files
        let sentinel_queue = "{\"version\":1,\"tasks\":[]}";
        let sentinel_done = "{\"version\":1,\"tasks\":[]}";
        let sentinel_productivity = "{}";
        fs::write(&queue_path, sentinel_queue).unwrap();
        fs::write(&done_path, sentinel_done).unwrap();
        fs::write(&productivity_path, sentinel_productivity).unwrap();

        let valid_queue = "{\"version\":1,\"tasks\":[{\"id\":\"RQ-0001\",\"status\":\"todo\",\"title\":\"Test\",\"tags\":[\"test\"],\"scope\":[\"file\"],\"evidence\":[\"obs\"],\"plan\":[\"do\"],\"created_at\":\"2026-01-18T00:00:00Z\",\"updated_at\":\"2026-01-18T00:00:00Z\"}]}";

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(valid_queue.as_bytes().to_vec()),
            done_bytes: Some(b"not valid json".to_vec()),
            productivity_bytes: Some(sentinel_productivity.as_bytes().to_vec()),
        };

        // Should return error for invalid done bytes
        let err = apply_merge_queue_sync(&resolved, &result).unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("parse done as JSONC") || err_msg.contains("done"),
            "Error should mention done parsing: {}",
            err_msg
        );

        // Verify sentinel files unchanged
        assert_eq!(fs::read_to_string(&queue_path).unwrap(), sentinel_queue);
        assert_eq!(fs::read_to_string(&done_path).unwrap(), sentinel_done);
        assert_eq!(
            fs::read_to_string(&productivity_path).unwrap(),
            sentinel_productivity
        );
    }

    #[test]
    fn apply_merge_queue_sync_rejects_invalid_utf8_in_queue_bytes() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");
        let productivity_path = cache_dir.join("productivity.json");

        // Create sentinel files
        let sentinel_queue = "{\"version\":1,\"tasks\":[]}";
        let sentinel_done = "{\"version\":1,\"tasks\":[]}";
        let sentinel_productivity = "{}";
        fs::write(&queue_path, sentinel_queue).unwrap();
        fs::write(&done_path, sentinel_done).unwrap();
        fs::write(&productivity_path, sentinel_productivity).unwrap();

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        // Invalid UTF-8 bytes
        let invalid_utf8 = vec![0xff, 0xfe, 0xfd];
        let valid_done = "{\"version\":1,\"tasks\":[]}";

        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(invalid_utf8),
            done_bytes: Some(valid_done.as_bytes().to_vec()),
            productivity_bytes: Some(sentinel_productivity.as_bytes().to_vec()),
        };

        // Should return error for invalid UTF-8
        let err = apply_merge_queue_sync(&resolved, &result).unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("not valid UTF-8") || err_msg.contains("UTF-8"),
            "Error should mention UTF-8: {}",
            err_msg
        );

        // Verify sentinel files unchanged
        assert_eq!(fs::read_to_string(&queue_path).unwrap(), sentinel_queue);
        assert_eq!(fs::read_to_string(&done_path).unwrap(), sentinel_done);
        assert_eq!(
            fs::read_to_string(&productivity_path).unwrap(),
            sentinel_productivity
        );
    }

    #[test]
    fn apply_merge_queue_sync_rejects_semantic_validation_failure() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");
        let productivity_path = cache_dir.join("productivity.json");

        // Create sentinel files
        let sentinel_queue = "{\"version\":1,\"tasks\":[]}";
        let sentinel_done = "{\"version\":1,\"tasks\":[]}";
        let sentinel_productivity = "{}";
        fs::write(&queue_path, sentinel_queue).unwrap();
        fs::write(&done_path, sentinel_done).unwrap();
        fs::write(&productivity_path, sentinel_productivity).unwrap();

        // Queue and done both contain the same task ID (duplicate - should fail validation)
        let valid_queue = "{\"version\":1,\"tasks\":[{\"id\":\"RQ-0001\",\"status\":\"todo\",\"title\":\"Test\",\"tags\":[\"test\"],\"scope\":[\"file\"],\"evidence\":[\"obs\"],\"plan\":[\"do\"],\"created_at\":\"2026-01-18T00:00:00Z\",\"updated_at\":\"2026-01-18T00:00:00Z\"}]}";
        let valid_done = "{\"version\":1,\"tasks\":[{\"id\":\"RQ-0001\",\"status\":\"done\",\"title\":\"Test Done\",\"tags\":[\"test\"],\"scope\":[\"file\"],\"evidence\":[\"obs\"],\"plan\":[\"do\"],\"created_at\":\"2026-01-18T00:00:00Z\",\"updated_at\":\"2026-01-18T00:00:00Z\",\"completed_at\":\"2026-01-18T00:00:00Z\"}]}";

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(valid_queue.as_bytes().to_vec()),
            done_bytes: Some(valid_done.as_bytes().to_vec()),
            productivity_bytes: Some(sentinel_productivity.as_bytes().to_vec()),
        };

        // Should return error for duplicate IDs
        let err = apply_merge_queue_sync(&resolved, &result).unwrap_err();
        let err_chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
        let full_error = err_chain.join(" | ");
        assert!(
            full_error.contains("Duplicate task ID detected across queue and done"),
            "Error should mention duplicate ID: {}",
            full_error
        );

        // Verify sentinel files unchanged
        assert_eq!(fs::read_to_string(&queue_path).unwrap(), sentinel_queue);
        assert_eq!(fs::read_to_string(&done_path).unwrap(), sentinel_done);
        assert_eq!(
            fs::read_to_string(&productivity_path).unwrap(),
            sentinel_productivity
        );
    }

    #[test]
    fn apply_merge_queue_sync_removes_done_file_when_done_bytes_none() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");
        let productivity_path = cache_dir.join("productivity.json");

        // Create existing done file
        let existing_done = "{\"version\":1,\"tasks\":[]}";
        fs::write(&done_path, existing_done).unwrap();

        let valid_queue = "{\"version\":1,\"tasks\":[{\"id\":\"RQ-0001\",\"status\":\"todo\",\"title\":\"Test\",\"tags\":[\"test\"],\"scope\":[\"file\"],\"evidence\":[\"obs\"],\"plan\":[\"do\"],\"created_at\":\"2026-01-18T00:00:00Z\",\"updated_at\":\"2026-01-18T00:00:00Z\"}]}";
        let productivity = "{}";

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(valid_queue.as_bytes().to_vec()),
            done_bytes: None, // No done bytes - should remove done file
            productivity_bytes: Some(productivity.as_bytes().to_vec()),
        };

        // Should succeed
        apply_merge_queue_sync(&resolved, &result).unwrap();

        // Queue should be written
        assert!(queue_path.exists());
        // Done file should be removed
        assert!(!done_path.exists());
        // Productivity should be written
        assert!(productivity_path.exists());
    }

    #[test]
    fn apply_merge_queue_sync_accepts_jsonc_and_normalizes_output() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        // JSONC with comment and trailing comma (should be accepted)
        let jsonc_queue = r#"{
            // This is a comment
            "version": 1,
            "tasks": [{
                "id": "RQ-0001",
                "status": "todo",
                "title": "Test",
                "tags": ["test"],
                "scope": ["file"],
                "evidence": ["obs"],
                "plan": ["do"],
                "created_at": "2026-01-18T00:00:00Z",
                "updated_at": "2026-01-18T00:00:00Z",
            }],
        }"#;

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(jsonc_queue.as_bytes().to_vec()),
            done_bytes: None,
            productivity_bytes: None,
        };

        // Should succeed (JSONC accepted)
        apply_merge_queue_sync(&resolved, &result).unwrap();

        // Read the written queue
        let written_queue = fs::read_to_string(&queue_path).unwrap();

        // Comment should be stripped (not present in normalized output)
        assert!(
            !written_queue.contains("// This is a comment"),
            "Comment should be stripped from normalized output"
        );

        // Should be valid JSON that parses
        let parsed: QueueFile = serde_json::from_str(&written_queue).unwrap();
        assert_eq!(parsed.tasks.len(), 1);
        assert_eq!(parsed.tasks[0].id, "RQ-0001");
    }

    #[test]
    fn apply_merge_queue_sync_preserves_files_on_validation_failure() {
        let temp = TempDir::new().unwrap();
        let ralph_dir = temp.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();
        let cache_dir = ralph_dir.join("cache");
        fs::create_dir_all(&cache_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");
        let productivity_path = cache_dir.join("productivity.json");

        // Create sentinel files with unique content
        let sentinel_queue = "SENTINEL_QUEUE_CONTENT";
        let sentinel_done = "SENTINEL_DONE_CONTENT";
        let sentinel_productivity = "SENTINEL_PRODUCTIVITY";
        fs::write(&queue_path, sentinel_queue).unwrap();
        fs::write(&done_path, sentinel_done).unwrap();
        fs::write(&productivity_path, sentinel_productivity).unwrap();

        // Invalid queue bytes (malformed JSON)
        let result = MergeResult {
            task_id: "RQ-0001".to_string(),
            merged: true,
            queue_bytes: Some(b"{ invalid json".to_vec()),
            done_bytes: Some(b"also invalid".to_vec()),
            productivity_bytes: Some(b"should not be written".to_vec()),
        };

        let resolved =
            build_test_resolved_for_merge_tests(temp.path(), queue_path.clone(), done_path.clone());

        // Should fail
        let _ = apply_merge_queue_sync(&resolved, &result);

        // All sentinel files should be unchanged
        assert_eq!(fs::read_to_string(&queue_path).unwrap(), sentinel_queue);
        assert_eq!(fs::read_to_string(&done_path).unwrap(), sentinel_done);
        assert_eq!(
            fs::read_to_string(&productivity_path).unwrap(),
            sentinel_productivity
        );
    }

    #[test]
    fn run_loop_parallel_rejects_dirty_repo_without_force() -> anyhow::Result<()> {
        use crate::testsupport::git as git_test;
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;

        // Avoid false positives: queue lock may create runtime files under .ralph/lock or .ralph/cache.
        // Ignore those so the dirty signal is *our* disallowed file.
        std::fs::write(
            temp.path().join(".gitignore"),
            ".ralph/lock/\n.ralph/cache/\n",
        )?;
        git_test::git_run(temp.path(), &["add", ".gitignore"])?;
        git_test::git_run(temp.path(), &["commit", "-m", "init"])?;

        // Ensure .ralph exists (lock code may assume it).
        std::fs::create_dir_all(temp.path().join(".ralph"))?;

        // Create a minimal valid queue.json (otherwise we get "no todo tasks" before clean-repo check)
        let queue_path = temp.path().join(".ralph/queue.json");
        let queue_content = r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"todo","title":"Test","scope":["file.rs"],"evidence":["obs"],"plan":["step"],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#;
        std::fs::write(&queue_path, queue_content)?;

        // Introduce a disallowed dirty change.
        std::fs::write(temp.path().join("notes.txt"), "dirty")?;

        let repo_root = temp.path().to_path_buf();
        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path,
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = run_loop_parallel(
            &resolved,
            ParallelRunOptions {
                max_tasks: 0,
                workers: 2,
                agent_overrides: AgentOverrides::default(),
                force: false,
                merge_when: ParallelMergeWhen::AsCreated,
            },
        )
        .unwrap_err();

        // The error should be a DirtyRepo error (wrapped in anyhow)
        let err_str = format!("{}", err);
        assert!(
            err_str.contains("repo is dirty"),
            "expected DirtyRepo error, got: {}",
            err_str
        );
        assert!(
            err_str.contains("notes.txt"),
            "expected error to mention notes.txt, got: {}",
            err_str
        );

        Ok(())
    }

    #[test]
    fn run_loop_parallel_fails_fast_on_invalid_queue_json() -> anyhow::Result<()> {
        use crate::testsupport::git as git_test;
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;

        std::fs::create_dir_all(temp.path().join(".ralph"))?;

        // Ignore runtime dirs so acquiring locks / touching cache doesn't dirty the repo.
        std::fs::write(
            temp.path().join(".gitignore"),
            ".ralph/lock/\n.ralph/cache/\n",
        )?;
        git_test::git_run(temp.path(), &["add", ".gitignore"])?;

        // Invalid queue: missing created_at/updated_at (semantic validation must fail).
        let queue_path = temp.path().join(".ralph/queue.json");
        std::fs::write(
            &queue_path,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"todo","title":"Test","scope":["file.rs"],"evidence":["obs"],"plan":["step"]}]}"#,
        )?;
        git_test::git_run(temp.path(), &["add", ".ralph/queue.json"])?;
        git_test::git_run(temp.path(), &["commit", "-m", "init"])?;

        let repo_root = temp.path().to_path_buf();
        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path,
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = run_loop_parallel(
            &resolved,
            ParallelRunOptions {
                max_tasks: 0,
                workers: 2,
                agent_overrides: AgentOverrides::default(),
                force: false,
                merge_when: ParallelMergeWhen::AsCreated,
            },
        )
        .unwrap_err();

        let err_chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
        let full_error = err_chain.join(" | ");
        assert!(
            full_error.contains("created_at") || full_error.contains("updated_at"),
            "expected missing timestamp validation error, got: {}",
            full_error
        );

        // Fail-fast proof: state file is not created.
        let state_path = state::state_file_path(&repo_root);
        assert!(
            !state_path.exists(),
            "state file should not exist on preflight failure: {}",
            state_path.display()
        );

        Ok(())
    }

    #[test]
    fn run_loop_parallel_fails_fast_on_invalid_done_json() -> anyhow::Result<()> {
        use crate::testsupport::git as git_test;
        use tempfile::TempDir;

        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;

        std::fs::create_dir_all(temp.path().join(".ralph"))?;

        std::fs::write(
            temp.path().join(".gitignore"),
            ".ralph/lock/\n.ralph/cache/\n",
        )?;
        git_test::git_run(temp.path(), &["add", ".gitignore"])?;

        // Valid queue.
        let queue_path = temp.path().join(".ralph/queue.json");
        std::fs::write(
            &queue_path,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"todo","title":"Test","scope":["file.rs"],"evidence":["obs"],"plan":["step"],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#,
        )?;

        // Invalid done: contains non-terminal status (must be done/rejected only).
        let done_path = temp.path().join(".ralph/done.json");
        std::fs::write(
            &done_path,
            r#"{"version":1,"tasks":[{"id":"RQ-0002","status":"todo","title":"Bad Done","scope":["file.rs"],"evidence":["obs"],"plan":["step"],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#,
        )?;

        git_test::git_run(
            temp.path(),
            &["add", ".ralph/queue.json", ".ralph/done.json"],
        )?;
        git_test::git_run(temp.path(), &["commit", "-m", "init"])?;

        let repo_root = temp.path().to_path_buf();
        let resolved = config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.clone(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = run_loop_parallel(
            &resolved,
            ParallelRunOptions {
                max_tasks: 0,
                workers: 2,
                agent_overrides: AgentOverrides::default(),
                force: false,
                merge_when: ParallelMergeWhen::AsCreated,
            },
        )
        .unwrap_err();

        let err_chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
        let full_error = err_chain.join(" | ");
        assert!(
            full_error.contains("done.json")
                && full_error.contains("must contain only done/rejected"),
            "expected done.json terminal-status validation error, got: {}",
            full_error
        );

        let state_path = state::state_file_path(&repo_root);
        assert!(
            !state_path.exists(),
            "state file should not exist on preflight failure: {}",
            state_path.display()
        );

        Ok(())
    }
}
