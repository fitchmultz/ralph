//! CLI arguments for task mutation transactions.
//!
//! Purpose:
//! - CLI arguments for task mutation transactions.
//!
//! Responsibilities:
//! - Define args for structured task mutation requests.
//! - Expose human vs JSON formatting for continuation-first mutation output.
//!
//! Not handled here:
//! - JSON parsing or queue mutation execution.
//! - Legacy single-field edit arguments.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Input is a JSON document that matches `TaskMutationRequest`.
//! - Missing `--input` means read the JSON request from stdin.

use clap::Args;
use clap::ValueEnum;

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskMutateFormatArg {
    Text,
    Json,
}

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

    /// Output format for the continuation report.
    #[arg(long, value_enum, default_value_t = TaskMutateFormatArg::Text)]
    pub format: TaskMutateFormatArg,
}
