//! Shared request-reading helpers for task command entrypoints.
//!
//! Purpose:
//! - Provide one canonical path for reading task requests from CLI args or stdin.
//!
//! Responsibilities:
//! - Join positional args into a trimmed request.
//! - Read piped stdin when no args are provided.
//! - Reject empty request inputs with task-specific error messages.
//!
//! Not handled here:
//! - Prompt rendering.
//! - Task creation/update logic.
//! - CLI argument definition.
//!
//! Usage:
//! - Used by human CLI and machine command handlers that accept request/source input.
//!
//! Invariants/assumptions:
//! - Terminal stdin means no piped request content is available.
//! - Returned requests are trimmed and non-empty.

use anyhow::{Context, Result, bail};
use std::io::{IsTerminal, Read};

pub fn read_request_from_args_or_reader(
    args: &[String],
    stdin_is_terminal: bool,
    mut reader: impl Read,
) -> Result<String> {
    if !args.is_empty() {
        let joined = args.join(" ");
        let trimmed = joined.trim();
        if trimmed.is_empty() {
            bail!(
                "Missing request: task requires a request description. Pass arguments or pipe input to the command."
            );
        }
        return Ok(trimmed.to_string());
    }

    if stdin_is_terminal {
        bail!(
            "Missing request: task requires a request description. Pass arguments or pipe input to the command."
        );
    }

    let mut buf = String::new();
    reader.read_to_string(&mut buf).context("read stdin")?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        bail!(
            "Missing request: task requires a request description (pass arguments or pipe input to the command)."
        );
    }
    Ok(trimmed.to_string())
}

// read_request_from_args_or_stdin joins any positional args, otherwise reads stdin.
pub fn read_request_from_args_or_stdin(args: &[String]) -> Result<String> {
    let stdin = std::io::stdin();
    let stdin_is_terminal = stdin.is_terminal();
    let handle = stdin.lock();
    read_request_from_args_or_reader(args, stdin_is_terminal, handle)
}
