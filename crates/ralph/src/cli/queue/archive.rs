//! Queue archive subcommand.

use anyhow::Result;

use crate::config::Resolved;
use crate::queue;

pub(crate) fn handle(resolved: &Resolved, force: bool) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue archive", force)?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let report = queue::archive_terminal_tasks(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    if report.moved_ids.is_empty() {
        log::info!("No terminal tasks (done/rejected) to archive.");
    } else {
        log::info!(
            "Archived {} terminal task(s) (done/rejected).",
            report.moved_ids.len()
        );
    }
    Ok(())
}
