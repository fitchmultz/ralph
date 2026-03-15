//! Post-run supervision scenario coverage.
//!
//! Responsibilities:
//! - Group post-run supervision regressions by queue/archive behavior, CI/maintenance edges,
//!   and git publish-mode outcomes.
//!
//! Not handled here:
//! - Continue-session resume fallback logic.
//! - Parallel-worker bookkeeping restore.
//!
//! Invariants/assumptions:
//! - Child modules stay focused on one post-run behavior seam each.
//! - Shared builders live in `runtime_tests/support.rs`.

#[path = "post_run_supervise/archive_and_dirty.rs"]
mod archive_and_dirty;
#[path = "post_run_supervise/ci_and_maintenance.rs"]
mod ci_and_maintenance;
#[path = "post_run_supervise/publish_modes.rs"]
mod publish_modes;
#[path = "post_run_supervise/revert_modes.rs"]
mod revert_modes;
