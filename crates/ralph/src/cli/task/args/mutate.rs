//! CLI arguments for task mutation transactions.
//!
//! Responsibilities:
//! - Define args for structured task mutation requests.
//!
//! Not handled here:
//! - JSON parsing or queue mutation execution.
//! - Legacy single-field edit arguments.
//!
//! Invariants/assumptions:
//! - Input is a JSON document that matches `TaskMutationRequest`.
//! - Missing `--input` means read the JSON request from stdin.

use clap::Args;

#[derive(Args, Clone)]
pub struct TaskMutateArgs {
    /// Read the mutation request from a JSON file.
    ///
    /// When omitted, Ralph reads the JSON request from stdin.
    #[arg(long, value_name = "PATH")]
    pub input: Option<String>,

    /// Preview validation and conflict checks without saving queue changes.
    #[arg(long)]
    pub dry_run: bool,
}
