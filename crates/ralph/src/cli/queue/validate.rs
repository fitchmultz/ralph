//! Queue validation subcommand.
//!
//! Purpose:
//! - Queue validation subcommand.
//!
//! Responsibilities:
//! - Inspect whether Ralph can safely continue from the current queue state.
//! - Keep the command read-only while providing continuation guidance.
//! - Reuse the same blocking and continuation language as machine/app surfaces.
//!
//! Not handled here:
//! - Queue mutation or repair writes.
//! - Task-level edits.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Validation remains read-only.
//! - Invalid queues return a non-zero exit status after printing guidance.

use anyhow::{Result, bail};

use crate::config::Resolved;

pub(crate) fn handle(resolved: &Resolved) -> Result<()> {
    let document = crate::cli::machine::build_queue_validate_document(resolved);

    println!("{}", document.continuation.headline);
    println!("{}", document.continuation.detail);

    if let Some(blocking) = document
        .blocking
        .as_ref()
        .or(document.continuation.blocking.as_ref())
    {
        println!();
        println!(
            "Operator state: {}",
            format!("{:?}", blocking.status).to_lowercase()
        );
        println!("{}", blocking.message);
        if !blocking.detail.is_empty() {
            println!("{}", blocking.detail);
        }
    }

    if !document.warnings.is_empty() {
        println!();
        println!("Warnings:");
        for warning in &document.warnings {
            println!("  - [{}] {}", warning.task_id, warning.message);
        }
    }

    if !document.continuation.next_steps.is_empty() {
        println!();
        println!("Next:");
        for (index, step) in document.continuation.next_steps.iter().enumerate() {
            println!("  {}. {} — {}", index + 1, step.command, step.detail);
        }
    }

    if document.valid {
        Ok(())
    } else {
        bail!("queue validation failed")
    }
}
