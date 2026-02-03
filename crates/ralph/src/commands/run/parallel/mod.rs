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
use merge_runner::{MergeQueueSource, MergeResult};
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

    let (pr_tx, pr_rx) = mpsc::channel::<git::PrInfo>();
    let (merge_result_tx, merge_result_rx) = mpsc::channel::<MergeResult>();
    let mut merge_handle = None;
    // Only include PRs that are still open and not merged
    let existing_prs: Vec<git::PrInfo> = state_file
        .prs
        .iter()
        .filter(|record| {
            matches!(record.lifecycle, state::ParallelPrLifecycle::Open) && !record.merged
        })
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

    // Only track workspaces for open/unmerged PRs (closed/merged should not drive merge behavior)
    for record in state_file.prs.iter().filter(|record| {
        matches!(record.lifecycle, state::ParallelPrLifecycle::Open) && !record.merged
    }) {
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
        for pr in &existing_prs {
            let _ = pr_tx.send(pr.clone());
        }
    }

    let include_draft = opts.agent_overrides.include_draft.unwrap_or(false);
    let worker_overrides = overrides_for_parallel_workers(resolved, &opts.agent_overrides);
    // Count resumed in-flight tasks toward max_tasks to prevent over-starting on resume.
    let mut tasks_started: u32 = initial_tasks_started(&state_file);
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
            if ctrlc.interrupted.load(Ordering::SeqCst) {
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
            let pruned = prune_stale_tasks_in_flight(guard.state_file_mut());
            if !pruned.is_empty() {
                log::warn!(
                    "Dropping stale in-flight tasks during loop: {}",
                    pruned.join(", ")
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

        if existing.base_branch.is_empty() {
            if in_flight.is_empty() && blocking_prs.is_empty() {
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
                    &blocking_prs
                ));
            }
        } else if existing.base_branch != current_branch {
            if in_flight.is_empty() && blocking_prs.is_empty() {
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
                    &blocking_prs
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
        .filter(|record| {
            matches!(record.lifecycle, state::ParallelPrLifecycle::Open) && !record.merged
        })
        .map(|record| record.task_id.clone())
        .collect()
}

fn format_base_branch_mismatch_error(
    state_path: &Path,
    recorded_branch: &str,
    current_branch: &str,
    in_flight: &[String],
    blocking_prs: &[String],
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

fn apply_merge_queue_sync(resolved: &config::Resolved, result: &MergeResult) -> Result<()> {
    let Some(queue_bytes) = result.queue_bytes.as_ref() else {
        bail!(
            "Merged PR for {} did not return queue bytes; refusing to update local queue.",
            result.task_id
        );
    };
    fsutil::write_atomic(&resolved.queue_path, queue_bytes)
        .with_context(|| format!("write queue bytes for {}", result.task_id))?;

    match result.done_bytes.as_ref() {
        Some(bytes) => {
            fsutil::write_atomic(&resolved.done_path, bytes)
                .with_context(|| format!("write done bytes for {}", result.task_id))?;
        }
        None => {
            if let Err(err) = std::fs::remove_file(&resolved.done_path)
                && err.kind() != std::io::ErrorKind::NotFound
            {
                return Err(err.into());
            }
        }
    }

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
/// Returns the number of tasks_in_flight records as u32, capping at u32::MAX.
fn initial_tasks_started(state_file: &state::ParallelStateFile) -> u32 {
    u32::try_from(state_file.tasks_in_flight.len()).unwrap_or(u32::MAX)
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
    fn resume_in_flight_counts_toward_max_tasks() {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // Simulate 2 tasks in flight from resumed state
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0001".to_string(),
            workspace_path: "/tmp/ws/RQ-0001".to_string(),
            branch: "ralph/RQ-0001".to_string(),
            pid: Some(12345),
        });
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: "/tmp/ws/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12346),
        });

        // Verify initial_tasks_started returns the count
        assert_eq!(initial_tasks_started(&state_file), 2);

        // With max_tasks = 2, should not be able to start more
        assert!(!can_start_more_tasks(2, 2));

        // With max_tasks = 3, should be able to start more
        assert!(can_start_more_tasks(2, 3));

        // With max_tasks = 0 (unlimited), should be able to start more
        assert!(can_start_more_tasks(2, 0));
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
            });
            s.tasks_in_flight.push(state::ParallelTaskRecord {
                task_id: "RQ-0002".to_string(),
                workspace_path: "/tmp/ws/RQ-0002".to_string(),
                branch: "ralph/RQ-0002".to_string(),
                pid: Some(12346),
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
        });
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            workspace_path: "/tmp/ws/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(12346),
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
}
