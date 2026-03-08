//! Queue validation subcommand.

use anyhow::Result;

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;

pub(crate) fn handle(resolved: &Resolved) -> Result<()> {
    load_and_validate_queues_read_only(resolved, true)?;
    Ok(())
}
