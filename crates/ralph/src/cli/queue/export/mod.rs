//! Queue export command facade and orchestration.
//!
//! Responsibilities:
//! - Re-export the queue export argument type and shared rendering helpers.
//! - Route queue export requests through focused filtering and rendering modules.
//! - Keep the command-facing API stable while implementation details stay split by concern.
//!
//! Not handled here:
//! - Queue mutation or task modification (see `crate::queue::operations`).
//! - Complex reporting/aggregation logic beyond export shaping.
//!
//! Invariants/assumptions:
//! - Export behavior remains deterministic for the same queue state and flags.
//! - Format-specific rendering is delegated to dedicated helpers.

mod args;
mod filter;
mod handle;
mod render;

pub use args::QueueExportArgs;
pub(crate) use handle::handle;
pub(crate) use render::render_task_as_github_issue_body;

#[cfg(test)]
mod tests;
