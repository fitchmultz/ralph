//! Shared helpers for the top-level CLI surface.
//!
//! Responsibilities:
//! - Emit top-level helper commands such as `help-all` and `cli-spec`.
//! - Centralize small queue/list helper utilities reused by subcommands.
//! - Keep stdout handling consistent for machine-readable CLI helpers.
//!
//! Not handled here:
//! - Clap argument definitions.
//! - Subcommand business logic beyond small shared helpers.
//! - Parse-regression tests.
//!
//! Invariants/assumptions:
//! - `cli-spec` remains machine-readable and broken-pipe tolerant.
//! - Shared queue/list helpers preserve existing read-only semantics.

use anyhow::Result;
use std::io::{self, Write};

use crate::contracts::QueueFile;

use super::args::{CliSpecArgs, CliSpecFormatArg};

pub fn handle_cli_spec(args: CliSpecArgs) -> Result<()> {
    match args.format {
        CliSpecFormatArg::Json => {
            let json = crate::commands::cli_spec::emit_cli_spec_json_pretty()?;
            let mut stdout = io::stdout().lock();
            if let Err(err) = writeln!(stdout, "{json}") {
                if err.kind() == io::ErrorKind::BrokenPipe {
                    return Ok(());
                }
                return Err(err.into());
            }
            Ok(())
        }
    }
}

pub fn handle_help_all() {
    println!(
        "Core:\n  init\n  app\n  queue\n  task\n  scan\n  run\n  config\n  version\n\nAdvanced:\n  prompt\n  doctor\n  context\n  prd\n  completions\n  migrate\n  cleanup\n  watch\n  webhook\n  productivity\n  plugin\n  runner\n  tutorial\n  undo\n  machine\n  cli-spec\n\nExperimental:\n  run loop --parallel\n  run parallel status\n  run parallel retry"
    );
}

pub(crate) fn load_and_validate_queues_read_only(
    resolved: &crate::config::Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    crate::queue::load_and_validate_queues(resolved, include_done)
}

pub(crate) fn resolve_list_limit(limit: u32, all: bool) -> Option<usize> {
    if all || limit == 0 {
        None
    } else {
        Some(limit as usize)
    }
}
