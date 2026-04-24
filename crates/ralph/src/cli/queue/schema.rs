//! Queue schema subcommand.
//!
//! Purpose:
//! - Queue schema subcommand.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;

use crate::contracts;

pub(crate) fn handle() -> Result<()> {
    let schema = schemars::schema_for!(contracts::QueueFile);
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
