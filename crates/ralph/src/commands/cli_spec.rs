//! CLI spec command implementation (wired into the hidden/internal `ralph __cli-spec` command).
//!
//! Purpose:
//! - CLI spec command implementation (wired into the hidden/internal `ralph __cli-spec` command).
//!
//! Responsibilities:
//! - Provide a small "command layer" entrypoint that produces the deterministic CLI spec JSON for
//!   the current build of Ralph.
//! - Keep the execution/IO boundary separate from clap introspection and contract modeling.
//!
//! Not handled here:
//! - Registering a user-facing `ralph cli-spec` (or similar) top-level command in clap.
//! - Reading/writing files or printing to stdout/stderr.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Uses `Cli::command()` as the canonical source of the clap command tree.
//! - Output must be deterministic across invocations within the same build.

use anyhow::Result;

use clap::CommandFactory;

use crate::contracts::CliSpec;

/// Build the structured `CliSpec` for the current Ralph CLI.
pub fn build_cli_spec() -> CliSpec {
    let command = crate::cli::Cli::command();
    crate::cli_spec::cli_spec_from_command(&command)
}

/// Emit deterministic pretty JSON for the current Ralph CLI.
pub fn emit_cli_spec_json_pretty() -> Result<String> {
    let command = crate::cli::Cli::command();
    crate::cli_spec::cli_spec_json_pretty_from_command(&command)
}
