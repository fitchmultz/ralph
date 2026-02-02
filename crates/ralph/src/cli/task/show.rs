//! Task display command handler for `ralph task show` subcommand.
//!
//! Responsibilities:
//! - Handle `show` and `details` commands.
//! - Display task information from queue or done archive.
//!
//! Not handled here:
//! - Task building or editing (see `build.rs`, `edit.rs`).
//! - Queue listing or searching (see `cli/queue/show.rs`, `cli/queue/list.rs`).
//!
//! Invariants/assumptions:
//! - Task is searched in both queue and done archive.
//! - Output format can be JSON or compact.

use anyhow::Result;

use crate::cli::queue::show_task;
use crate::cli::task::args::TaskShowArgs;
use crate::config;

/// Handle the `show` command.
pub fn handle(args: &TaskShowArgs, resolved: &config::Resolved) -> Result<()> {
    show_task(resolved, &args.task_id, args.format)
}
