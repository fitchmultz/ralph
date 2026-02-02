//! Runner execution facade and submodules.
//!
//! This module provides the implementation details for runner execution, delegated from
//! the parent `runner` module. It contains runner-specific CLI handling, process
//! management, and response extraction.
//!
//! Responsibilities:
//! - Define runner execution submodules and expose crate-only helpers.
//! - Implement runner-specific execution logic for all 7 supported runners.
//! - Handle CLI option resolution, command building, and process spawning.
//! - Extract and normalize runner responses (session IDs, assistant output).
//!
//! Does not handle:
//! - Runner selection or configuration validation (handled by parent module).
//! - Prompt templating or composition (handled by prompt modules).
//! - Public API surface (this is an internal implementation detail).
//!
//! Assumptions/invariants:
//! - Callers pass validated runner inputs (binaries resolved, models validated).
//! - Callers manage temporary file lifetimes for prompt files.
//! - The parent module handles error context and user-facing error messages.
//!
//! Submodule Organization:
//! - `cli_options.rs`: CLI option resolution from config/task/override sources.
//! - `cli_spec.rs`: CLI specification types for runner command construction.
//! - `command.rs`: Command building for runner subprocesses.
//! - `json.rs`: JSON handling for runner input/output.
//! - `process.rs`: Process management and execution.
//! - `response.rs`: Response extraction (session IDs, assistant messages).
//! - `runners.rs`: Individual runner implementations (run + resume; Kimi reuses run for resume).
//! - `stream.rs`: Output streaming to handlers and terminals.
//! - `tests.rs`: Execution-specific integration tests.

mod cli_options;
mod cli_spec;
mod command;
mod json;
mod process;
mod response;
mod runners;
mod stream;

#[cfg(test)]
mod tests;

pub(super) use response::extract_final_assistant_response;
pub(super) use runners::{
    run_claude, run_claude_resume, run_codex, run_codex_resume, run_cursor, run_cursor_resume,
    run_gemini, run_gemini_resume, run_kimi, run_opencode, run_opencode_resume, run_pi,
    run_pi_resume,
};

pub(crate) use cli_options::{ResolvedRunnerCliOptions, resolve_runner_cli_options};
pub(crate) use process::ctrlc_state;
