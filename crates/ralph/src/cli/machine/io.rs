//! JSON and stdin/stdout helpers for machine commands.
//!
//! Responsibilities:
//! - Read JSON request payloads from files or stdin.
//! - Emit pretty JSON documents and NDJSON event lines.
//! - Keep machine IO behavior centralized and deterministic.
//!
//! Not handled here:
//! - Machine command routing.
//! - Queue/task/run business logic.
//! - Machine contract type definitions.
//!
//! Invariants/assumptions:
//! - Machine JSON input must be non-empty.
//! - Pretty JSON is used for document responses; NDJSON is used for event streams.

use std::io::Read;

use anyhow::{Context, Result, bail};
use serde::Serialize;

pub(super) fn read_json_input(path: Option<&str>) -> Result<String> {
    if let Some(path) = path {
        return std::fs::read_to_string(path)
            .with_context(|| format!("read JSON input from {}", path));
    }

    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .context("read JSON input from stdin")?;
    if raw.trim().is_empty() {
        bail!("JSON input is empty");
    }
    Ok(raw)
}

pub(super) fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub(super) fn print_json_line<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}
