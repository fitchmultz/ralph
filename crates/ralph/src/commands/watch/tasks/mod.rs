//! Watch-task orchestration.
//!
//! Purpose:
//! - Watch-task orchestration.
//!
//! Responsibilities:
//! - Coordinate detected-comment ingestion into queue task creation or suggestion output.
//! - Delegate task reconciliation and task construction to focused helpers.
//! - Keep the public watch-task surface stable for the rest of watch mode.
//!
//! Not handled here:
//! - Comment detection (see `super::comments`).
//! - File watching and debounce orchestration (see `super::event_loop` / `super::processor`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Watch identity is explicit and location-aware.
//! - Queue writes happen only when tasks were created, upgraded, or reconciled.

mod materialize;
mod orchestrator;
mod reconcile;
#[cfg(test)]
mod tests;
pub use orchestrator::handle_detected_comments;
#[cfg(test)]
pub(crate) use orchestrator::reconcile_watch_tasks;
#[cfg(test)]
pub(crate) use reconcile::task_exists_for_comment;
