//! CI gate argv validation.
//!
//! Purpose:
//! - CI gate argv validation.
//!
//! Responsibilities:
//! - Validate CI gate enablement and argv shape.
//! - Reject shell-launcher argv to preserve argv-only execution semantics.
//!
//! Not handled here:
//! - Trust decisions for project-local execution.
//! - Agent-level iteration or phase validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Enabled CI gates must provide a non-empty argv array.
//! - Shell wrappers such as `sh -c` remain unsupported.

use crate::contracts::CiGateConfig;
use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CiGateArgvIssue {
    EmptyArgv,
    EmptyEntry,
    ShellLauncher,
}

pub(crate) fn validate_ci_gate_config(ci_gate: Option<&CiGateConfig>, label: &str) -> Result<()> {
    let Some(ci_gate) = ci_gate else {
        return Ok(());
    };

    if !ci_gate.is_enabled() {
        return Ok(());
    }

    match ci_gate.argv.as_ref() {
        Some(argv) => validate_ci_gate_argv(argv, label),
        None => bail!("Invalid {label}.ci_gate: enabled CI gate requires argv settings."),
    }
}

pub(crate) fn validate_ci_gate_argv(argv: &[String], label: &str) -> Result<()> {
    if let Some(issue) = detect_ci_gate_argv_issue(argv) {
        match issue {
            CiGateArgvIssue::EmptyArgv => {
                bail!("Invalid {label}.ci_gate.argv: at least one argv element is required.");
            }
            CiGateArgvIssue::EmptyEntry => {
                bail!("Invalid {label}.ci_gate.argv: argv entries must not be empty strings.");
            }
            CiGateArgvIssue::ShellLauncher => {
                bail!(
                    "Invalid {label}.ci_gate.argv: shell launcher argv is not supported. Use direct executable argv instead."
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn detect_ci_gate_argv_issue(argv: &[String]) -> Option<CiGateArgvIssue> {
    if argv.is_empty() {
        return Some(CiGateArgvIssue::EmptyArgv);
    }
    if argv.iter().any(|arg| arg.trim().is_empty()) {
        return Some(CiGateArgvIssue::EmptyEntry);
    }
    if argv_launches_shell(argv) {
        return Some(CiGateArgvIssue::ShellLauncher);
    }
    None
}

fn argv_launches_shell(argv: &[String]) -> bool {
    let Some(program) = argv.first() else {
        return false;
    };
    let Some(program_name) = std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return false;
    };

    let shell_program = matches!(
        program_name,
        "sh" | "bash" | "zsh" | "dash" | "fish" | "cmd" | "pwsh" | "powershell"
    );
    shell_program
        && argv.iter().skip(1).any(|arg| {
            arg == "-c" || arg.eq_ignore_ascii_case("/c") || arg.eq_ignore_ascii_case("-command")
        })
}
