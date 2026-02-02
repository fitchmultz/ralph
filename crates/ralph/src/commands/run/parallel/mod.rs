//! Parallel run loop supervisor and worker orchestration.
//!
//! Responsibilities:
//! - Select runnable tasks and spawn parallel workers in git worktrees.
//! - Create PRs on success/failure and optionally dispatch merge runner work.
//! - Track in-flight workers and coordinate cleanup after merges.
//!
//! Not handled here:
//! - CLI parsing (see `crate::cli::run`).
//! - Task execution details (delegated to `ralph run one` workers).
//! - Merge conflict resolution logic (see `merge_runner`).
//!
//! Invariants/assumptions:
//! - Queue order is authoritative for task selection.
//! - Workers run in isolated worktrees with dedicated branches.
//! - PR creation relies on authenticated `gh` CLI access.

use crate::agent::AgentOverrides;
use crate::commands::run::selection::select_run_one_task_index_excluding;
use crate::config;
use crate::contracts::{ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, ParallelMergeWhen};
use crate::{git, promptflow, queue, runutil, signal, timeutil};
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

mod merge_runner;
mod state;

use merge_runner::{MergeQueueSource, MergeResult};

pub(crate) struct ParallelRunOptions {
    pub max_tasks: u32,
    pub workers: u8,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
    pub merge_when: ParallelMergeWhen,
}

struct WorkerState {
    task_id: String,
    task_title: String,
    worktree: git::WorktreeSpec,
    child: Child,
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
    worktree_root: PathBuf,
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
    let mut dropped_tasks = Vec::new();
    state_file.tasks_in_flight.retain(|record| {
        let path = Path::new(&record.worktree_path);
        if path.exists() {
            true
        } else {
            dropped_tasks.push(record.task_id.clone());
            false
        }
    });
    if !dropped_tasks.is_empty() {
        log::warn!(
            "Dropping stale in-flight tasks with missing worktrees: {}",
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
        let worktree_root = settings.worktree_root.clone();
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
                &worktree_root,
                delete_branch,
                merge_result_tx_for_thread,
                merge_stop_for_thread,
            )
        }));
    }

    let mut in_flight: HashMap<String, WorkerState> = HashMap::new();
    let mut completed_worktrees: HashMap<String, git::WorktreeSpec> = HashMap::new();
    let mut created_prs: Vec<git::PrInfo> = existing_prs.clone();

    for record in state_file.prs.iter().filter(|record| !record.merged) {
        let path = record
            .worktree_path()
            .unwrap_or_else(|| settings.worktree_root.join(&record.task_id));
        if path.exists() {
            completed_worktrees.insert(
                record.task_id.clone(),
                git::WorktreeSpec {
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
    let mut tasks_started: u32 = 0;
    let mut tasks_attempted: usize = 0;
    let mut tasks_succeeded: usize = 0;
    let mut tasks_failed: usize = 0;
    let mut interrupted = false;

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
        while in_flight.len() < settings.workers as usize
            && (opts.max_tasks == 0 || tasks_started < opts.max_tasks)
            && !signal::stop_signal_exists(&cache_dir)
        {
            let excluded = collect_excluded_ids(&state_file, &in_flight);
            let (task_id, task_title) =
                match select_next_task(resolved, include_draft, &excluded, opts.force)? {
                    Some(task) => task,
                    None => break,
                };

            let worktree = git::create_worktree_at(
                &resolved.repo_root,
                &settings.worktree_root,
                &task_id,
                &base_branch,
                &settings.branch_prefix,
            )?;

            let child = spawn_worker(
                resolved,
                &worktree.path,
                &task_id,
                &opts.agent_overrides,
                opts.force,
            )?;

            let record = state::ParallelTaskRecord::new(&task_id, &worktree, child.id());
            state_file.upsert_task(record);
            state::save_state(&state_path, &state_file)?;

            in_flight.insert(
                task_id.clone(),
                WorkerState {
                    task_id,
                    task_title,
                    worktree,
                    child,
                },
            );
            tasks_started += 1;
        }

        // Drain merge results for cleanup.
        while let Ok(result) = merge_result_rx.try_recv() {
            if result.merged {
                if let Some(worktree) = completed_worktrees.remove(&result.task_id)
                    && let Err(err) = git::remove_worktree(&resolved.repo_root, &worktree, true)
                {
                    log::warn!(
                        "Failed to remove worktree for {}: {:#}",
                        result.task_id,
                        err
                    );
                }
                state_file.mark_pr_merged(&result.task_id);
                state::save_state(&state_path, &state_file)?;
            }
        }

        // Poll workers.
        let mut finished: Vec<String> = Vec::new();
        for (task_id, worker) in in_flight.iter_mut() {
            if let Some(status) = worker.child.try_wait()? {
                tasks_attempted += 1;
                if status.success() {
                    tasks_succeeded += 1;
                    handle_worker_success(
                        resolved,
                        worker,
                        &settings,
                        &base_branch,
                        &mut created_prs,
                        &pr_tx,
                        &mut state_file,
                        &state_path,
                    )?;
                } else {
                    tasks_failed += 1;
                    handle_worker_failure(
                        resolved,
                        worker,
                        &settings,
                        &base_branch,
                        &mut state_file,
                        &state_path,
                    )?;
                }

                completed_worktrees.insert(task_id.clone(), worker.worktree.clone());
                state_file.remove_task(task_id);
                state::save_state(&state_path, &state_file)?;
                finished.push(task_id.clone());
            }
        }
        for task_id in finished {
            in_flight.remove(&task_id);
        }

        if in_flight.is_empty() {
            let no_more_tasks = opts.max_tasks != 0 && tasks_started >= opts.max_tasks;
            let excluded = collect_excluded_ids(&state_file, &in_flight);
            let next_available =
                select_next_task(resolved, include_draft, &excluded, opts.force)?.is_some();
            if no_more_tasks || !next_available {
                break;
            }
        }

        thread::sleep(Duration::from_millis(500));
    }

    if interrupted {
        merge_stop.store(true, Ordering::SeqCst);
        drop(pr_tx);
        if let Some(handle) = merge_handle.take()
            && let Err(err) = handle.join()
        {
            log::warn!("Merge runner thread panicked during shutdown: {:?}", err);
        }

        terminate_workers(&mut in_flight);
        let cleanup_worktrees =
            collect_worktrees_for_cleanup(&settings, &in_flight, &completed_worktrees, &state_file);
        for spec in cleanup_worktrees {
            if spec.path.exists()
                && let Err(err) = git::remove_worktree(&resolved.repo_root, &spec, true)
            {
                log::warn!("Failed to remove worktree for {}: {:#}", spec.task_id, err);
            }
        }
        state_file.tasks_in_flight.clear();
        state::save_state(&state_path, &state_file)?;
        return Err(runutil::RunAbort::new(
            runutil::RunAbortReason::Interrupted,
            "Parallel run interrupted by Ctrl+C",
        )
        .into());
    }

    drop(pr_tx);

    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AfterAll {
        let merge_result_tx = merge_result_tx.clone();
        merge_runner::run_merge_runner(
            resolved,
            settings.merge_method,
            settings.conflict_policy,
            settings.merge_runner.clone(),
            settings.merge_retries,
            MergeQueueSource::AfterAll(created_prs.clone()),
            &settings.worktree_root,
            settings.delete_branch_on_merge,
            merge_result_tx,
            Arc::clone(&merge_stop),
        )?;
    }

    if let Some(handle) = merge_handle {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => bail!("Merge runner thread panicked"),
        }
    }

    // Drain any remaining merge results for cleanup.
    while let Ok(result) = merge_result_rx.try_recv() {
        if result.merged {
            if let Some(worktree) = completed_worktrees.remove(&result.task_id)
                && let Err(err) = git::remove_worktree(&resolved.repo_root, &worktree, true)
            {
                log::warn!(
                    "Failed to remove worktree for {}: {:#}",
                    result.task_id,
                    err
                );
            }
            state_file.mark_pr_merged(&result.task_id);
            state::save_state(&state_path, &state_file)?;
        }
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

fn select_next_task(
    resolved: &config::Resolved,
    include_draft: bool,
    excluded_ids: &HashSet<String>,
    force: bool,
) -> Result<Option<(String, String)>> {
    let _lock = queue::acquire_queue_lock(&resolved.repo_root, "parallel selection", force)?;
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };

    let idx =
        select_run_one_task_index_excluding(&queue_file, done_ref, include_draft, excluded_ids)?;
    let idx = match idx {
        Some(idx) => idx,
        None => return Ok(None),
    };
    let task = &queue_file.tasks[idx];
    Ok(Some((
        task.id.trim().to_string(),
        task.title.trim().to_string(),
    )))
}

fn collect_excluded_ids(
    state_file: &state::ParallelStateFile,
    in_flight: &HashMap<String, WorkerState>,
) -> HashSet<String> {
    let mut excluded = HashSet::new();
    for key in in_flight.keys() {
        excluded.insert(key.trim().to_string());
    }
    for record in &state_file.tasks_in_flight {
        excluded.insert(record.task_id.trim().to_string());
    }
    for record in &state_file.prs {
        if !record.merged {
            excluded.insert(record.task_id.trim().to_string());
        }
    }
    excluded
}

fn terminate_workers(in_flight: &mut HashMap<String, WorkerState>) {
    for worker in in_flight.values_mut() {
        if let Err(err) = worker.child.kill() {
            log::warn!("Failed to terminate worker {}: {}", worker.task_id, err);
        }
    }

    for worker in in_flight.values_mut() {
        let _ = worker.child.wait();
    }
}

fn collect_worktrees_for_cleanup(
    settings: &ParallelSettings,
    in_flight: &HashMap<String, WorkerState>,
    completed_worktrees: &HashMap<String, git::WorktreeSpec>,
    state_file: &state::ParallelStateFile,
) -> Vec<git::WorktreeSpec> {
    let mut seen = HashSet::new();
    let mut collected = Vec::new();

    let mut push_unique = |spec: git::WorktreeSpec| {
        if seen.insert(spec.path.clone()) {
            collected.push(spec);
        }
    };

    for worker in in_flight.values() {
        push_unique(worker.worktree.clone());
    }

    for spec in completed_worktrees.values() {
        push_unique(spec.clone());
    }

    for record in &state_file.tasks_in_flight {
        push_unique(git::WorktreeSpec {
            task_id: record.task_id.clone(),
            path: PathBuf::from(&record.worktree_path),
            branch: record.branch.clone(),
        });
    }

    for record in state_file.prs.iter().filter(|record| !record.merged) {
        let path = record
            .worktree_path()
            .unwrap_or_else(|| settings.worktree_root.join(&record.task_id));
        let branch = format!("{}{}", settings.branch_prefix, record.task_id);
        push_unique(git::WorktreeSpec {
            task_id: record.task_id.clone(),
            path,
            branch,
        });
    }

    collected
}

fn spawn_worker(
    _resolved: &config::Resolved,
    worktree_path: &Path,
    task_id: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<Child> {
    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut cmd = Command::new(exe);
    cmd.current_dir(worktree_path);
    cmd.env("PWD", worktree_path);

    let mut args: Vec<String> = Vec::new();
    if force {
        args.push("--force".to_string());
    }
    args.push("--no-progress".to_string());
    args.push("run".to_string());
    args.push("one".to_string());
    args.push("--id".to_string());
    args.push(task_id.to_string());
    args.push("--parallel-worker".to_string());
    args.push("--non-interactive".to_string());
    args.push("--git-commit-push-on".to_string());

    args.extend(build_override_args(overrides));

    cmd.args(args);
    let child = cmd.spawn().context("spawn parallel worker")?;
    Ok(child)
}

#[allow(clippy::too_many_arguments)]
fn handle_worker_success(
    resolved: &config::Resolved,
    worker: &WorkerState,
    settings: &ParallelSettings,
    base_branch: &str,
    created_prs: &mut Vec<git::PrInfo>,
    pr_tx: &mpsc::Sender<git::PrInfo>,
    state_file: &mut state::ParallelStateFile,
    state_path: &Path,
) -> Result<()> {
    if !settings.auto_pr {
        return Ok(());
    }

    ensure_branch_pushed(&worker.worktree.path)?;

    let body = promptflow::read_phase2_final_response_cache(&worker.worktree.path, &worker.task_id)
        .unwrap_or_default();
    let title = format!("{}: {}", worker.task_id, worker.task_title);
    let pr = git::create_pr(
        &resolved.repo_root,
        &title,
        &body,
        &worker.worktree.branch,
        base_branch,
        false,
    )?;

    state_file.upsert_pr(state::ParallelPrRecord::new(
        &worker.task_id,
        &pr,
        Some(&worker.worktree.path),
    ));
    state::save_state(state_path, state_file)?;

    created_prs.push(pr.clone());
    if settings.auto_merge && settings.merge_when == ParallelMergeWhen::AsCreated {
        let _ = pr_tx.send(pr);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_worker_failure(
    resolved: &config::Resolved,
    worker: &WorkerState,
    settings: &ParallelSettings,
    base_branch: &str,
    state_file: &mut state::ParallelStateFile,
    state_path: &Path,
) -> Result<()> {
    if !settings.auto_pr || !settings.draft_on_failure {
        return Ok(());
    }

    if !commit_failure_changes(&worker.worktree.path, &worker.task_id)? {
        log::warn!(
            "Worker {} failed with no changes; skipping draft PR.",
            worker.task_id
        );
        return Ok(());
    }

    ensure_branch_pushed(&worker.worktree.path)?;

    let body = format!(
        "Failed run for {}. Draft PR generated by Ralph.",
        worker.task_id
    );
    let title = format!("{}: {}", worker.task_id, worker.task_title);
    let pr = git::create_pr(
        &resolved.repo_root,
        &title,
        &body,
        &worker.worktree.branch,
        base_branch,
        true,
    )?;

    state_file.upsert_pr(state::ParallelPrRecord::new(
        &worker.task_id,
        &pr,
        Some(&worker.worktree.path),
    ));
    state::save_state(state_path, state_file)?;
    log::info!(
        "Draft PR {} created for {}; skipping auto-merge.",
        pr.number,
        worker.task_id
    );

    Ok(())
}

fn commit_failure_changes(worktree_path: &Path, task_id: &str) -> Result<bool> {
    let status = git::status_porcelain(worktree_path)?;
    if status.trim().is_empty() {
        return Ok(false);
    }

    let message = format!("WIP: {} (failed run)", task_id);
    match git::commit_all(worktree_path, &message) {
        Ok(()) => Ok(true),
        Err(err) => match err {
            git::GitError::NoChangesToCommit => Ok(false),
            _ => Err(err.into()),
        },
    }
}

fn ensure_branch_pushed(worktree_path: &Path) -> Result<()> {
    match git::is_ahead_of_upstream(worktree_path) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
            git::push_upstream(worktree_path).with_context(|| "push branch to upstream")?;
            Ok(())
        }
        Err(git::GitError::NoUpstream) | Err(git::GitError::NoUpstreamConfigured) => {
            git::push_upstream_allow_create(worktree_path)
                .with_context(|| "push branch and create upstream")?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
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
        worktree_root: git::worktree_root(&resolved.repo_root, &resolved.config),
        branch_prefix: cfg
            .branch_prefix
            .clone()
            .unwrap_or_else(|| "ralph/".to_string()),
        delete_branch_on_merge: cfg.delete_branch_on_merge.unwrap_or(true),
        merge_runner: cfg.merge_runner.clone().unwrap_or_default(),
    })
}

fn build_override_args(overrides: &AgentOverrides) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(runner) = overrides.runner {
        args.push("--runner".to_string());
        args.push(runner.as_str().to_string());
    }
    if let Some(model) = overrides.model.clone() {
        args.push("--model".to_string());
        args.push(model.as_str().to_string());
    }
    if let Some(effort) = overrides.reasoning_effort {
        args.push("--effort".to_string());
        args.push(reasoning_effort_arg(effort).to_string());
    }
    if let Some(phases) = overrides.phases {
        args.push("--phases".to_string());
        args.push(phases.to_string());
    }
    if let Some(repo_prompt) = repo_prompt_arg(overrides) {
        args.push("--repo-prompt".to_string());
        args.push(repo_prompt.to_string());
    }
    if let Some(mode) = overrides.git_revert_mode {
        args.push("--git-revert-mode".to_string());
        args.push(git_revert_mode_arg(mode).to_string());
    }

    if overrides.include_draft.unwrap_or(false) {
        args.push("--include-draft".to_string());
    }

    if let Some(update) = overrides.update_task_before_run {
        if update {
            args.push("--update-task".to_string());
        } else {
            args.push("--no-update-task".to_string());
        }
    }

    if let Some(value) = overrides.notify_on_complete {
        args.push(if value {
            "--notify".to_string()
        } else {
            "--no-notify".to_string()
        });
    }

    if let Some(value) = overrides.notify_on_fail {
        args.push(if value {
            "--notify-fail".to_string()
        } else {
            "--no-notify-fail".to_string()
        });
    }

    if overrides.notify_sound.unwrap_or(false) {
        args.push("--notify-sound".to_string());
    }

    if overrides.lfs_check.unwrap_or(false) {
        args.push("--lfs-check".to_string());
    }

    if let Some(cli) = build_runner_cli_args(&overrides.runner_cli) {
        args.extend(cli);
    }

    if let Some(phase_args) = build_phase_override_args(overrides) {
        args.extend(phase_args);
    }

    args
}

fn build_runner_cli_args(cli: &crate::contracts::RunnerCliOptionsPatch) -> Option<Vec<String>> {
    let mut args = Vec::new();
    if let Some(value) = cli.output_format {
        args.push("--output-format".to_string());
        args.push(output_format_arg(value).to_string());
    }
    if let Some(value) = cli.verbosity {
        args.push("--verbosity".to_string());
        args.push(verbosity_arg(value).to_string());
    }
    if let Some(value) = cli.approval_mode {
        args.push("--approval-mode".to_string());
        args.push(approval_mode_arg(value).to_string());
    }
    if let Some(value) = cli.sandbox {
        args.push("--sandbox".to_string());
        args.push(sandbox_mode_arg(value).to_string());
    }
    if let Some(value) = cli.plan_mode {
        args.push("--plan-mode".to_string());
        args.push(plan_mode_arg(value).to_string());
    }
    if let Some(value) = cli.unsupported_option_policy {
        args.push("--unsupported-option-policy".to_string());
        args.push(unsupported_option_policy_arg(value).to_string());
    }

    if args.is_empty() { None } else { Some(args) }
}

fn build_phase_override_args(overrides: &AgentOverrides) -> Option<Vec<String>> {
    let overrides = overrides.phase_overrides.as_ref()?;
    let mut args = Vec::new();

    if let Some(phase1) = overrides.phase1.as_ref() {
        if let Some(runner) = phase1.runner {
            args.push("--runner-phase1".to_string());
            args.push(runner.as_str().to_string());
        }
        if let Some(model) = phase1.model.clone() {
            args.push("--model-phase1".to_string());
            args.push(model.as_str().to_string());
        }
        if let Some(effort) = phase1.reasoning_effort {
            args.push("--effort-phase1".to_string());
            args.push(reasoning_effort_arg(effort).to_string());
        }
    }

    if let Some(phase2) = overrides.phase2.as_ref() {
        if let Some(runner) = phase2.runner {
            args.push("--runner-phase2".to_string());
            args.push(runner.as_str().to_string());
        }
        if let Some(model) = phase2.model.clone() {
            args.push("--model-phase2".to_string());
            args.push(model.as_str().to_string());
        }
        if let Some(effort) = phase2.reasoning_effort {
            args.push("--effort-phase2".to_string());
            args.push(reasoning_effort_arg(effort).to_string());
        }
    }

    if let Some(phase3) = overrides.phase3.as_ref() {
        if let Some(runner) = phase3.runner {
            args.push("--runner-phase3".to_string());
            args.push(runner.as_str().to_string());
        }
        if let Some(model) = phase3.model.clone() {
            args.push("--model-phase3".to_string());
            args.push(model.as_str().to_string());
        }
        if let Some(effort) = phase3.reasoning_effort {
            args.push("--effort-phase3".to_string());
            args.push(reasoning_effort_arg(effort).to_string());
        }
    }

    if args.is_empty() { None } else { Some(args) }
}

fn repo_prompt_arg(overrides: &AgentOverrides) -> Option<&'static str> {
    match (
        overrides.repoprompt_plan_required,
        overrides.repoprompt_tool_injection,
    ) {
        (Some(true), Some(true)) => Some("plan"),
        (Some(false), Some(true)) => Some("tools"),
        (Some(false), Some(false)) => Some("off"),
        _ => None,
    }
}

fn reasoning_effort_arg(effort: crate::contracts::ReasoningEffort) -> &'static str {
    match effort {
        crate::contracts::ReasoningEffort::Low => "low",
        crate::contracts::ReasoningEffort::Medium => "medium",
        crate::contracts::ReasoningEffort::High => "high",
        crate::contracts::ReasoningEffort::XHigh => "xhigh",
    }
}

fn git_revert_mode_arg(mode: crate::contracts::GitRevertMode) -> &'static str {
    match mode {
        crate::contracts::GitRevertMode::Ask => "ask",
        crate::contracts::GitRevertMode::Enabled => "enabled",
        crate::contracts::GitRevertMode::Disabled => "disabled",
    }
}

fn output_format_arg(mode: crate::contracts::RunnerOutputFormat) -> &'static str {
    match mode {
        crate::contracts::RunnerOutputFormat::StreamJson => "stream-json",
        crate::contracts::RunnerOutputFormat::Json => "json",
        crate::contracts::RunnerOutputFormat::Text => "text",
    }
}

fn verbosity_arg(mode: crate::contracts::RunnerVerbosity) -> &'static str {
    match mode {
        crate::contracts::RunnerVerbosity::Quiet => "quiet",
        crate::contracts::RunnerVerbosity::Normal => "normal",
        crate::contracts::RunnerVerbosity::Verbose => "verbose",
    }
}

fn approval_mode_arg(mode: crate::contracts::RunnerApprovalMode) -> &'static str {
    match mode {
        crate::contracts::RunnerApprovalMode::Default => "default",
        crate::contracts::RunnerApprovalMode::AutoEdits => "auto-edits",
        crate::contracts::RunnerApprovalMode::Yolo => "yolo",
        crate::contracts::RunnerApprovalMode::Safe => "safe",
    }
}

fn sandbox_mode_arg(mode: crate::contracts::RunnerSandboxMode) -> &'static str {
    match mode {
        crate::contracts::RunnerSandboxMode::Default => "default",
        crate::contracts::RunnerSandboxMode::Enabled => "enabled",
        crate::contracts::RunnerSandboxMode::Disabled => "disabled",
    }
}

fn plan_mode_arg(mode: crate::contracts::RunnerPlanMode) -> &'static str {
    match mode {
        crate::contracts::RunnerPlanMode::Default => "default",
        crate::contracts::RunnerPlanMode::Enabled => "enabled",
        crate::contracts::RunnerPlanMode::Disabled => "disabled",
    }
}

fn unsupported_option_policy_arg(mode: crate::contracts::UnsupportedOptionPolicy) -> &'static str {
    match mode {
        crate::contracts::UnsupportedOptionPolicy::Ignore => "ignore",
        crate::contracts::UnsupportedOptionPolicy::Warn => "warn",
        crate::contracts::UnsupportedOptionPolicy::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        ConflictPolicy, MergeRunnerConfig, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort,
        Runner, RunnerApprovalMode, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
        RunnerVerbosity, UnsupportedOptionPolicy,
    };
    use std::path::PathBuf;

    #[test]
    fn build_override_args_emits_expected_flags() {
        let overrides = AgentOverrides {
            runner: Some(Runner::Codex),
            model: Some(crate::contracts::Model::Gpt52),
            reasoning_effort: Some(ReasoningEffort::High),
            phases: Some(2),
            repoprompt_plan_required: Some(true),
            repoprompt_tool_injection: Some(true),
            git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
            include_draft: Some(true),
            update_task_before_run: Some(false),
            notify_on_complete: Some(true),
            notify_on_fail: Some(false),
            notify_sound: Some(true),
            lfs_check: Some(true),
            ..Default::default()
        };

        let args = build_override_args(&overrides);
        let expected = vec![
            "--runner",
            "codex",
            "--model",
            "gpt-5.2",
            "--effort",
            "high",
            "--phases",
            "2",
            "--repo-prompt",
            "plan",
            "--git-revert-mode",
            "disabled",
            "--include-draft",
            "--no-update-task",
            "--notify",
            "--no-notify-fail",
            "--notify-sound",
            "--lfs-check",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        assert_eq!(args, expected);
    }

    #[test]
    fn build_runner_cli_args_serializes_patch() {
        let patch = crate::contracts::RunnerCliOptionsPatch {
            output_format: Some(RunnerOutputFormat::Json),
            verbosity: Some(RunnerVerbosity::Verbose),
            approval_mode: Some(RunnerApprovalMode::AutoEdits),
            sandbox: Some(RunnerSandboxMode::Disabled),
            plan_mode: Some(RunnerPlanMode::Enabled),
            unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
        };
        let args = build_runner_cli_args(&patch).expect("args");
        let expected = vec![
            "--output-format",
            "json",
            "--verbosity",
            "verbose",
            "--approval-mode",
            "auto-edits",
            "--sandbox",
            "disabled",
            "--plan-mode",
            "enabled",
            "--unsupported-option-policy",
            "error",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        assert_eq!(args, expected);
    }

    #[test]
    fn build_phase_override_args_serializes_phase_flags() {
        let overrides = PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Codex),
                model: Some(crate::contracts::Model::Gpt52Codex),
                reasoning_effort: Some(ReasoningEffort::Low),
            }),
            phase2: Some(PhaseOverrideConfig {
                runner: Some(Runner::Claude),
                model: Some(crate::contracts::Model::Gpt52),
                reasoning_effort: Some(ReasoningEffort::Medium),
            }),
            phase3: Some(PhaseOverrideConfig {
                runner: Some(Runner::Kimi),
                model: Some(crate::contracts::Model::Glm47),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
        };
        let agent_overrides = AgentOverrides {
            phase_overrides: Some(overrides),
            ..Default::default()
        };

        let args = build_phase_override_args(&agent_overrides).expect("args");
        let expected = vec![
            "--runner-phase1",
            "codex",
            "--model-phase1",
            "gpt-5.2-codex",
            "--effort-phase1",
            "low",
            "--runner-phase2",
            "claude",
            "--model-phase2",
            "gpt-5.2",
            "--effort-phase2",
            "medium",
            "--runner-phase3",
            "kimi",
            "--model-phase3",
            "zai-coding-plan/glm-4.7",
            "--effort-phase3",
            "high",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        assert_eq!(args, expected);
    }

    #[test]
    fn collect_excluded_ids_includes_state_and_in_flight() -> Result<()> {
        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            worktree_path: "/tmp/worktree/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(123),
        });
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 7,
            pr_url: "https://example.com/pr/7".to_string(),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            worktree_path: None,
            merged: false,
        });

        let mut in_flight = HashMap::new();
        let child = std::process::Command::new("true").spawn()?;
        in_flight.insert(
            "RQ-0004".to_string(),
            WorkerState {
                task_id: "RQ-0004".to_string(),
                task_title: "title".to_string(),
                worktree: git::WorktreeSpec {
                    task_id: "RQ-0004".to_string(),
                    path: PathBuf::from("/tmp/worktree/RQ-0004"),
                    branch: "ralph/RQ-0004".to_string(),
                },
                child,
            },
        );

        let excluded = collect_excluded_ids(&state_file, &in_flight);
        assert!(excluded.contains("RQ-0002"));
        assert!(excluded.contains("RQ-0003"));
        assert!(excluded.contains("RQ-0004"));

        for worker in in_flight.values_mut() {
            let _ = worker.child.wait();
        }

        Ok(())
    }

    #[test]
    fn collect_worktrees_for_cleanup_dedupes_sources() -> Result<()> {
        let settings = ParallelSettings {
            workers: 2,
            merge_when: ParallelMergeWhen::AsCreated,
            merge_method: ParallelMergeMethod::Squash,
            auto_pr: true,
            auto_merge: true,
            draft_on_failure: true,
            conflict_policy: ConflictPolicy::AutoResolve,
            merge_retries: 3,
            worktree_root: PathBuf::from("/tmp/worktrees"),
            branch_prefix: "ralph/".to_string(),
            delete_branch_on_merge: true,
            merge_runner: MergeRunnerConfig::default(),
        };

        let mut state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state_file.tasks_in_flight.push(state::ParallelTaskRecord {
            task_id: "RQ-0002".to_string(),
            worktree_path: "/tmp/worktrees/RQ-0002".to_string(),
            branch: "ralph/RQ-0002".to_string(),
            pid: Some(123),
        });
        state_file.prs.push(state::ParallelPrRecord {
            task_id: "RQ-0003".to_string(),
            pr_number: 9,
            pr_url: "https://example.com/pr/9".to_string(),
            head: Some("ralph/RQ-0003".to_string()),
            base: Some("main".to_string()),
            worktree_path: None,
            merged: false,
        });

        let mut in_flight = HashMap::new();
        let child = std::process::Command::new("true").spawn()?;
        in_flight.insert(
            "RQ-0001".to_string(),
            WorkerState {
                task_id: "RQ-0001".to_string(),
                task_title: "title".to_string(),
                worktree: git::WorktreeSpec {
                    task_id: "RQ-0001".to_string(),
                    path: PathBuf::from("/tmp/worktrees/RQ-0001"),
                    branch: "ralph/RQ-0001".to_string(),
                },
                child,
            },
        );

        let mut completed_worktrees = HashMap::new();
        completed_worktrees.insert(
            "RQ-0001".to_string(),
            git::WorktreeSpec {
                task_id: "RQ-0001".to_string(),
                path: PathBuf::from("/tmp/worktrees/RQ-0001"),
                branch: "ralph/RQ-0001".to_string(),
            },
        );

        let collected =
            collect_worktrees_for_cleanup(&settings, &in_flight, &completed_worktrees, &state_file);
        let paths = collected
            .iter()
            .map(|spec| spec.path.clone())
            .collect::<HashSet<_>>();
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&PathBuf::from("/tmp/worktrees/RQ-0001")));
        assert!(paths.contains(&PathBuf::from("/tmp/worktrees/RQ-0002")));
        assert!(paths.contains(&PathBuf::from("/tmp/worktrees/RQ-0003")));

        for worker in in_flight.values_mut() {
            let _ = worker.child.wait();
        }

        Ok(())
    }
}
