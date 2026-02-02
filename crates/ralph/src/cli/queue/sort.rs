//! Queue sort subcommand.

use anyhow::Result;
use clap::Args;

use crate::config::Resolved;
use crate::queue;

use super::{QueueSortBy, QueueSortOrder};

/// Arguments for `ralph queue sort`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue sort\n  ralph queue sort --order descending\n  ralph queue sort --order ascending"
)]
pub struct QueueSortArgs {
    /// Sort by field (supported: priority; default: priority).
    #[arg(long, value_enum, default_value_t = QueueSortBy::Priority)]
    pub sort_by: QueueSortBy,

    /// Sort order (default: descending, highest priority first).
    #[arg(long, value_enum, default_value_t = QueueSortOrder::Descending)]
    pub order: QueueSortOrder,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueSortArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue sort", force)?;
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    match args.sort_by {
        QueueSortBy::Priority => {
            queue::sort_tasks_by_priority(&mut queue_file, args.order.is_descending());
        }
    }

    queue::save_queue(&resolved.queue_path, &queue_file)?;
    log::info!("Queue sorted by {} (order: {}).", args.sort_by, args.order);
    Ok(())
}
