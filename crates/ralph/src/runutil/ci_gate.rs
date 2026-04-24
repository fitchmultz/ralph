//! CI gate command translation helpers.
//!
//! Purpose:
//! - CI gate command translation helpers.
//!
//! Responsibilities:
//! - Convert structured `agent.ci_gate` config into executable commands.
//! - Provide one shared execution path for standard and parallel CI checks.
//!
//! Not handled here:
//! - CI failure classification or continue-session logic.
//! - Config source trust checks (see `crate::config::resolution`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Callers pass validated `CiGateConfig` values.
//! - Disabled CI gates return `None` and must be handled by the caller.

use crate::contracts::CiGateConfig;
use anyhow::{Result, bail};
use std::path::Path;
use std::process::Output;

use super::shell::{SafeCommand, execute_safe_command};

/// Convert a CI gate config into an executable command.
pub fn ci_gate_to_safe_command(ci_gate: &CiGateConfig) -> Result<SafeCommand> {
    if let Some(argv) = &ci_gate.argv {
        if argv.is_empty() {
            bail!("CI gate argv must contain at least one element");
        }
        if argv.iter().any(|arg| arg.trim().is_empty()) {
            bail!("CI gate argv entries must be non-empty");
        }
        return Ok(SafeCommand::Argv { argv: argv.clone() });
    }

    bail!("CI gate is enabled but no argv is configured");
}

/// Execute the configured CI gate command.
pub fn execute_ci_gate(ci_gate: &CiGateConfig, cwd: &Path) -> Result<Output> {
    let command = ci_gate_to_safe_command(ci_gate)?;
    execute_safe_command(&command, cwd)
}
