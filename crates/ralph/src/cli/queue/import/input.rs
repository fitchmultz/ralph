//! Queue import input loading helpers.
//!
//! Purpose:
//! - Queue import input loading helpers.
//!
//! Responsibilities:
//! - Read queue import payloads from stdin or a file path.
//! - Keep filesystem and stdin behavior consistent across formats.
//!
//! Not handled here:
//! - Parsing imported task payloads.
//! - Queue mutation or validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `-` always means stdin.
//! - Missing path means stdin.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};

pub(super) fn read_input(path: Option<&PathBuf>) -> Result<String> {
    let use_stdin = path.is_none() || path.is_some_and(|value| value.as_os_str() == "-");
    if use_stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("read from stdin")?;
        return Ok(buffer);
    }

    let path = path.expect("checked path above");
    std::fs::read_to_string(path).with_context(|| format!("read import file {}", path.display()))
}
