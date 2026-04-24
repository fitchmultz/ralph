//! Prompt inspection command facade.
//!
//! Purpose:
//! - Prompt inspection command facade.
//!
//! Responsibilities:
//! - Expose prompt management and preview entrypoints for the CLI layer.
//! - Re-export shared option types used by integration tests and callers.
//! - Keep the command surface thin while preview logic lives in focused helpers.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli::prompt`).
//! - Runner execution or task mutations.
//! - Prompt template persistence details beyond delegated management helpers.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview rendering reuses production prompt-building flows for fidelity.
//! - Each helper module owns a single prompt responsibility.

mod management;
mod scan;
mod source;
mod task_builder;
#[cfg(test)]
mod tests;
mod types;
mod worker;

pub use management::{diff_prompt, export_prompts, list_prompts, show_prompt, sync_prompts};
pub use scan::build_scan_prompt;
pub use task_builder::build_task_builder_prompt;
pub use types::{ScanPromptOptions, TaskBuilderPromptOptions, WorkerMode, WorkerPromptOptions};
pub use worker::build_worker_prompt;
