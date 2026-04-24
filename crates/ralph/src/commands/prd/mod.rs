//! PRD command facade.
//!
//! Purpose:
//! - PRD command facade.
//!
//! Responsibilities:
//! - Expose the PRD-to-task workflow entrypoint and shared public options.
//! - Keep PRD parsing, generation, and queue workflow code in focused helper modules.
//!
//! Not handled here:
//! - CLI parsing (see `crate::cli::prd`).
//! - Queue persistence internals beyond delegated queue helpers.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - PRD markdown parsing and task generation stay deterministic.
//! - The facade remains a thin re-export surface.

mod generate;
mod parse;
#[cfg(test)]
mod tests;
mod workflow;

pub use workflow::{CreateOptions, create_from_prd};
