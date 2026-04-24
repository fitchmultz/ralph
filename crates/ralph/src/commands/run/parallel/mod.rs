//! Parallel run loop supervisor and worker orchestration for direct-push mode.
//!
//! Purpose:
//! - Parallel run loop supervisor and worker orchestration for direct-push mode.
//!
//! Responsibilities:
//! - Coordinate parallel task execution across multiple workers.
//! - Manage settings resolution and preflight validation.
//! - Track worker capacity and task pruning.
//! - Handle direct-push integration from workers.
//!
//! Policy helpers live in focused modules (`settings`, `preflight`, `spawn`, `pruning`, `capacity`);
//! this file is the crate facade: module graph, shared constants, and re-exports.
//!
//! Not handled here:
//! - Main orchestration loop (see `orchestration.rs`).
//! - State initialization (see `state_init.rs`).
//! - Worker lifecycle (see `worker.rs`).
//! - Integration loop logic (see `integration.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue order is authoritative for task selection.
//! - Workers run in isolated per-task workspaces on the target base branch.
//! - Workers push directly to the target branch (no PRs).
//! - One active worker per task ID (enforced by upsert_worker).

mod args;
mod capacity;
mod cleanup_guard;
mod integration;
mod orchestration;
mod path_map;
mod preflight;
mod pruning;
mod settings;
mod spawn;
pub mod state;
mod state_init;
mod sync;
mod worker;
mod workspace_cleanup;

use state_init::load_or_init_parallel_state;

// =============================================================================
// Marker File Constants (for CI failure detection)
// =============================================================================

/// Marker file name for CI gate failure diagnostics.
/// Written to workspace when CI fails so coordinator/status tooling can inspect failures.
pub const CI_FAILURE_MARKER_FILE: &str = ".ralph/cache/ci-failure-marker";

/// Marker file name for blocked push outcomes from integration loop.
pub const BLOCKED_PUSH_MARKER_FILE: &str = ".ralph/cache/parallel/blocked_push.json";

/// Fallback marker file used only when primary marker path is unavailable.
pub const CI_FAILURE_MARKER_FALLBACK_FILE: &str = ".ralph-ci-failure-marker";

// Re-export public APIs from submodules
pub use integration::{IntegrationConfig, IntegrationOutcome, RemediationHandoff};
pub(crate) use integration::{read_blocked_push_marker, run_integration_loop};
pub(crate) use orchestration::run_loop_parallel;
pub use settings::default_push_backoff_ms;
pub use state::{WorkerLifecycle, WorkerRecord};

pub(crate) use capacity::{
    can_start_more_tasks, effective_active_worker_count, initial_tasks_started,
};
pub(crate) use preflight::preflight_parallel_workspace_root_is_gitignored;
pub(crate) use pruning::prune_stale_workers;
pub(crate) use settings::{
    ParallelRunOptions, ParallelSettings, overrides_for_parallel_workers, resolve_parallel_settings,
};
pub(crate) use spawn::spawn_worker_with_registered_workspace;
