//! Purpose: Build argv-only managed subprocess commands.
//!
//! Responsibilities:
//! - Validate argv inputs before constructing `std::process::Command` values.
//! - Reject empty command vectors or blank program entries.
//!
//! Scope:
//! - Argv validation and command construction only.
//!
//! Usage:
//! - Used by managed subprocess entrypoints that accept `SafeCommand` inputs.
//!
//! Invariants/Assumptions:
//! - Managed subprocess execution stays argv-first with no implicit shell parsing.
//! - The first argv element is always the executable path/program name.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};

pub(super) fn build_argv_command(argv: &[String]) -> Result<Command> {
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
