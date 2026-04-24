//! Project context (AGENTS.md) generation and management.
//!
//! Purpose:
//! - Project context (AGENTS.md) generation and management.
//!
//! Responsibilities:
//! - Generate initial AGENTS.md from project type detection.
//! - Update AGENTS.md with new learnings.
//! - Validate AGENTS.md against project structure.
//!
//! Not handled here:
//! - CLI argument parsing (see `cli::context`).
//! - Interactive prompts (see `wizard` module).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Templates are embedded at compile time.
//! - Project type detection uses simple file-based heuristics.
//! - AGENTS.md updates preserve manual edits (section-based merging).

mod detect;
mod markdown;
mod render;
mod types;
mod validate;
mod workflow;

pub mod merge;
pub mod wizard;

pub use types::{
    ContextInitOptions, ContextUpdateOptions, ContextValidateOptions, DetectedProjectType,
    FileInitStatus, InitReport, UpdateReport, ValidateReport,
};
pub use workflow::{run_context_init, run_context_update, run_context_validate};

#[cfg(test)]
mod tests;
