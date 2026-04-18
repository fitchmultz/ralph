//! Parallel run-loop preflight and bootstrap.
//!
//! Responsibilities:
//! - Validate repo/workspace prerequisites.
//! - Initialize persisted parallel state, cleanup guard ownership, and loop webhook context.
//!
//! Not handled here:
//! - The active worker orchestration loop.
//! - Final shutdown behavior.
//!
//! Invariants/assumptions:
//! - Called after queue-lock acquisition.
//! - Preflight fully validates queue/done paths before workers are spawned.

use std::collections::HashSet;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use crate::config;
use crate::{git, queue, signal, timeutil};

use super::super::cleanup_guard::ParallelCleanupGuard;
use super::super::orchestration::events::announce_blocked_tasks_at_loop_start;
use super::super::orchestration::stats::ParallelRunStats;
use super::super::{
    ParallelRunOptions, ParallelSettings, initial_tasks_started, load_or_init_parallel_state,
    overrides_for_parallel_workers, preflight_parallel_workspace_root_is_gitignored,
    resolve_parallel_settings,
};
use super::super::{path_map, state as parallel_state};

pub(super) struct PreparedParallelRun {
    pub(super) cache_dir: std::path::PathBuf,
    pub(super) state_path: std::path::PathBuf,
    pub(super) settings: ParallelSettings,
    pub(super) guard: ParallelCleanupGuard,
    pub(super) target_branch: String,
    pub(super) include_draft: bool,
    pub(super) worker_overrides: crate::agent::AgentOverrides,
    pub(super) tasks_started: u32,
    pub(super) stats: ParallelRunStats,
    pub(super) attempted_task_ids: HashSet<String>,
    pub(super) stop_requested: bool,
    pub(super) interrupted: bool,
    pub(super) loop_start_time: Instant,
    pub(super) loop_webhook_ctx: crate::webhook::WebhookContext,
}

pub(super) fn prepare_parallel_run(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
) -> Result<PreparedParallelRun> {
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    let cache_dir = resolved.repo_root.join(".ralph/cache");

    let ctrlc = crate::runner::ctrlc_state();
    if let Ok(ctrlc) = ctrlc {
        if ctrlc.interrupted.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(crate::runutil::RunAbort::new(
                crate::runutil::RunAbortReason::Interrupted,
                "Ctrl+C was pressed before parallel execution started",
            )
            .into());
        }
        ctrlc
            .interrupted
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    signal::clear_stop_signal_at_loop_start(&cache_dir);

    let (queue_file, _done_file) =
        queue::load_and_validate_queues(resolved, true).context(
            "Parallel preflight is read-only; run `ralph queue repair --dry-run` and then `ralph queue repair` to apply undo-backed normalization before retrying",
        )?;

    path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        &resolved.repo_root,
        &resolved.queue_path,
        "queue",
    )
    .with_context(|| {
        "Parallel preflight: queue.file must be under repo root (try a repo-relative path like '.ralph/queue.jsonc')".to_string()
    })?;

    path_map::map_resolved_path_into_workspace(
        &resolved.repo_root,
        &resolved.repo_root,
        &resolved.done_path,
        "done",
    )
    .with_context(|| {
        "Parallel preflight: queue.done_file must be under repo root (try a repo-relative path like '.ralph/done.jsonc')".to_string()
    })?;

    let settings = resolve_parallel_settings(resolved, opts)?;
    preflight_parallel_workspace_root_is_gitignored(&resolved.repo_root, &settings.workspace_root)?;
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
    let state_path = parallel_state::state_file_path(&resolved.repo_root);
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
    crate::webhook::init_worker_for_parallel(&resolved.config.agent.webhook, settings.workers);

    let loop_start_time = Instant::now();
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
    let tasks_started = initial_tasks_started(&state_file);
    let guard = ParallelCleanupGuard::new_simple(
        state_path.clone(),
        state_file,
        settings.workspace_root.clone(),
    );

    Ok(PreparedParallelRun {
        cache_dir,
        state_path,
        settings,
        guard,
        target_branch,
        include_draft,
        worker_overrides,
        tasks_started,
        stats: ParallelRunStats::default(),
        attempted_task_ids: HashSet::new(),
        stop_requested: false,
        interrupted: false,
        loop_start_time,
        loop_webhook_ctx,
    })
}
