//! Shell command helpers.
//!
//! Responsibilities:
//! - Build a platform-appropriate shell command wrapper.
//!
//! Not handled here:
//! - Process output handling, streaming, or error classification.
//!
//! Invariants/assumptions:
//! - Unix uses `sh -c`, Windows uses `cmd /C`.

use std::process::Command;

/// Build a shell command for the current platform (sh -c on Unix, cmd /C on Windows).
pub fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}
