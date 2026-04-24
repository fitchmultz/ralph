//! Cleanup command CLI arguments.
//!
//! Purpose:
//! - Cleanup command CLI arguments.
//!
//! Responsibilities:
//! - Define CLI arguments for the `ralph cleanup` command.
//!
//! Not handled here:
//! - Actual cleanup logic (see `crate::commands::cleanup`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Arguments are validated by Clap before being passed to the handler.

use clap::Args;

/// Arguments for `ralph cleanup`.
#[derive(Args, Debug)]
#[command(after_long_help = "Examples:\n\
  ralph cleanup              # Clean temp files older than 7 days\n\
  ralph cleanup --force      # Clean all ralph temp files\n\
  ralph cleanup --dry-run    # Show what would be deleted without deleting")]
pub struct CleanupArgs {
    /// Force cleanup of all ralph temp files regardless of age.
    #[arg(long)]
    pub force: bool,

    /// Dry run - show what would be deleted without deleting.
    #[arg(long)]
    pub dry_run: bool,
}

/// Handle the cleanup command.
pub fn handle_cleanup(args: CleanupArgs) -> anyhow::Result<()> {
    crate::commands::cleanup::run(&args)
}
