//! Queue validation subcommand.

use anyhow::Result;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;

pub(crate) fn handle(resolved: &Resolved) -> Result<()> {
    load_and_validate_queues(resolved, true)?;
    Ok(())
}
