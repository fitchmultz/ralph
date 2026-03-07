//! Safe process execution helpers.
//!
//! Responsibilities:
//! - Define a small execution model for argv-first subprocess spawning.
//! - Gate shell execution behind explicit repo trust checks.
//! - Normalize subprocess stdio defaults for CI-style command execution.
//!
//! Not handled here:
//! - CI gate config translation (see `crate::runutil::ci_gate`).
//! - Output interpretation or retry logic (see supervision/integration call sites).
//!
//! Invariants/assumptions:
//! - Direct argv execution is the default and preferred path.
//! - Shell execution is only allowed when repo trust is explicitly enabled.

use crate::config::load_repo_trust;
use crate::contracts::ShellMode;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Structured command execution without implicit shell interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SafeCommand {
    Argv { argv: Vec<String> },
    Shell { command: String, mode: ShellMode },
}

pub(crate) fn execute_safe_command(safe_command: &SafeCommand, cwd: &Path) -> Result<Output> {
    let mut command = match safe_command {
        SafeCommand::Argv { argv } => build_argv_command(argv)?,
        SafeCommand::Shell {
            command: shell_command,
            mode,
        } => {
            let repo_trust = load_repo_trust(cwd)?;
            if !repo_trust.is_trusted() {
                bail!(
                    "Shell-mode command execution requires repo trust. Create .ralph/trust.jsonc with allow_project_commands=true or use argv mode."
                );
            }
            build_shell_command(shell_command, *mode)?
        }
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

fn build_shell_command(command: &str, mode: ShellMode) -> Result<Command> {
    match mode {
        ShellMode::Posix => {
            if cfg!(windows) {
                bail!("shell.mode=posix is not supported on Windows.");
            }
            let mut process = Command::new("sh");
            process.arg("-c").arg(command);
            Ok(process)
        }
        ShellMode::WindowsCmd => {
            if !cfg!(windows) {
                bail!("shell.mode=windows_cmd is only supported on Windows.");
            }
            let mut process = Command::new("cmd");
            process.arg("/C").arg(command);
            Ok(process)
        }
    }
}
