//! Queue repair subcommand.
//!
//! Purpose:
//! - Queue repair subcommand.
//!
//! Responsibilities:
//! - Preview or apply recoverable queue normalization.
//! - Ensure mutating repairs are undoable.
//! - Narrate the operator continuation state instead of emergency-only repair wording.
//!
//! Not handled here:
//! - Arbitrary manual queue surgery.
//! - Non-queue recovery workflows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `--dry-run` is read-only.
//! - Mutating repairs create an undo checkpoint before queue files change.

use anyhow::Result;
use clap::Args;

use crate::cli::machine::MachineQueueRepairArgs;
use crate::config::Resolved;

/// Arguments for `ralph queue repair`.
#[derive(Args)]
pub struct RepairArgs {
    /// Show what Ralph would normalize without writing queue files.
    #[arg(long)]
    pub dry_run: bool,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: RepairArgs) -> Result<()> {
    let document = crate::cli::machine::build_queue_repair_document(
        resolved,
        force,
        &MachineQueueRepairArgs {
            dry_run: args.dry_run,
        },
    )?;

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

    println!();
    println!("Repair report:");
    println!("{}", serde_json::to_string_pretty(&document.report)?);

    if !document.continuation.next_steps.is_empty() {
        println!();
        println!("Next:");
        for (index, step) in document.continuation.next_steps.iter().enumerate() {
            println!("  {}. {} — {}", index + 1, step.command, step.detail);
        }
    }

    Ok(())
}
