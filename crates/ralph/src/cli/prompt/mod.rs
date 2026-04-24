//! `ralph prompt ...` CLI facade.
//!
//! Purpose:
//! - `ralph prompt ...` CLI facade.
//!
//! Responsibilities:
//! - Re-export clap argument types for prompt commands.
//! - Expose the prompt command handler while keeping parsing and routing separate.
//!
//! Not handled here:
//! - Prompt construction logic (see `crate::commands::prompt`).
//! - Queue persistence or runner execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - CLI parsing stays isolated from command implementation details.

mod args;
mod handle;

pub use args::{
    PromptArgs, PromptCommand, PromptDiffArgs, PromptExportArgs, PromptScanArgs, PromptShowArgs,
    PromptSyncArgs, PromptTaskBuilderArgs, PromptWorkerArgs, parse_phase,
};
pub use handle::handle_prompt;
