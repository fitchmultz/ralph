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
        if let Some(issue) = crate::config::detect_ci_gate_argv_issue(argv) {
            match issue {
                crate::config::CiGateArgvIssue::EmptyArgv => {
                    bail!("CI gate argv must contain at least one element");
                }
                crate::config::CiGateArgvIssue::EmptyEntry => {
                    bail!("CI gate argv entries must be non-empty");
                }
                crate::config::CiGateArgvIssue::ShellLauncher => {
                    bail!("CI gate shell launcher argv is not supported");
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_ci_gate(argv: &[&str]) -> CiGateConfig {
        CiGateConfig {
            enabled: Some(true),
            argv: Some(argv.iter().map(|arg| (*arg).to_string()).collect()),
        }
    }

    #[test]
    fn ci_gate_to_safe_command_rejects_shell_launchers() {
        for argv in [
            vec!["sh", "-c", "make ci"],
            vec!["cmd", "/C", "make ci"],
            vec!["pwsh", "-Command", "make ci"],
            vec!["powershell", "-Command", "make ci"],
        ] {
            let err = ci_gate_to_safe_command(&enabled_ci_gate(&argv))
                .expect_err("shell launcher argv should be rejected");
            assert!(
                err.to_string()
                    .contains("shell launcher argv is not supported"),
                "unexpected error for {argv:?}: {err:#}"
            );
        }
    }

    #[test]
    fn ci_gate_to_safe_command_allows_direct_argv() {
        let command =
            ci_gate_to_safe_command(&enabled_ci_gate(&["make", "ci"])).expect("direct argv");
        assert!(matches!(
            command,
            SafeCommand::Argv { argv } if argv == ["make".to_string(), "ci".to_string()]
        ));
    }

    #[test]
    fn ci_gate_to_safe_command_rejects_whitespace_only_entries() {
        let err = ci_gate_to_safe_command(&enabled_ci_gate(&["cargo", "   "]))
            .expect_err("whitespace-only argv should be rejected");
        assert!(err.to_string().contains("must be non-empty"));
    }
}
