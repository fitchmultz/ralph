//! Safe process execution helpers.
//!
//! Responsibilities:
//! - Define a small execution model for argv-first subprocess spawning.
//! - Normalize subprocess stdio defaults for CI-style command execution.
//!
//! Not handled here:
//! - CI gate config translation (see `crate::runutil::ci_gate`).
//! - Output interpretation or retry logic (see supervision/integration call sites).
//!
//! Invariants/assumptions:
//! - Direct argv execution is the only supported execution path.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Structured command execution without implicit shell interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SafeCommand {
    Argv { argv: Vec<String> },
}

pub(crate) fn execute_safe_command(safe_command: &SafeCommand, cwd: &Path) -> Result<Output> {
    let mut command = match safe_command {
        SafeCommand::Argv { argv } => build_argv_command(argv)?,
    };

    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    Ok(command.output()?)
}

fn build_argv_command(argv: &[String]) -> Result<Command> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("argv command must include at least one element"))?;
    if program.trim().is_empty() {
        bail!("argv command program must be non-empty.");
    }

    let mut command = Command::new(PathBuf::from(program));
    command.args(args);
    Ok(command)
}
