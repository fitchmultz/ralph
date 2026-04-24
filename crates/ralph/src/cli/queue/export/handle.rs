//! Queue export command execution orchestration.
//!
//! Purpose:
//! - Queue export command execution orchestration.
//!
//! Responsibilities:
//! - Validate export flags and coordinate queue loading, filtering, rendering, and output.
//! - Emit queue size warnings before exporting when requested.
//! - Keep command flow readable by delegating filtering and rendering details.
//!
//! Not handled here:
//! - Argument definitions.
//! - Format-specific rendering internals.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `--include-archive` and `--only-archive` remain mutually exclusive.
//! - Output is written either to the requested file path or stdout.

use std::io::Write;

use anyhow::{Context, Result, bail};

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;
use crate::queue;

use super::args::QueueExportArgs;
use super::filter::{collect_tasks, parse_created_after, parse_created_before};
use super::render::render_export;

pub(crate) fn handle(resolved: &Resolved, args: QueueExportArgs) -> Result<()> {
    validate_archive_flags(&args)?;
    let created_after = parse_created_after(&args)?;
    let created_before = parse_created_before(&args)?;

    let (queue_file, done_file) =
        load_and_validate_queues_read_only(resolved, args.include_archive || args.only_archive)?;

    maybe_print_queue_size_warning(resolved, &queue_file, args.quiet);

    let done_ref = done_file
        .as_ref()
        .filter(|done| !done.tasks.is_empty() || resolved.done_path.exists());
    let tasks = collect_tasks(&queue_file, done_ref, &args, created_after, created_before);
    let output = render_export(args.format, &tasks)?;

    write_output(args.output.as_deref(), &output)
}

fn validate_archive_flags(args: &QueueExportArgs) -> Result<()> {
    if args.include_archive && args.only_archive {
        bail!(
            "Conflicting flags: --include-archive and --only-archive are mutually exclusive. Choose either to include archive tasks or to only show archive tasks."
        );
    }
    Ok(())
}

fn maybe_print_queue_size_warning(
    resolved: &Resolved,
    queue_file: &crate::contracts::QueueFile,
    quiet: bool,
) {
    if quiet {
        return;
    }

    let size_threshold =
        queue::size_threshold_or_default(resolved.config.queue.size_warning_threshold_kb);
    let count_threshold =
        queue::count_threshold_or_default(resolved.config.queue.task_count_warning_threshold);
    if let Ok(result) = queue::check_queue_size(
        &resolved.queue_path,
        queue_file.tasks.len(),
        size_threshold,
        count_threshold,
    ) {
        queue::print_size_warning_if_needed(&result, quiet);
    }
}

fn write_output(path: Option<&std::path::Path>, output: &str) -> Result<()> {
    if let Some(path) = path {
        std::fs::write(path, output)
            .with_context(|| format!("Failed to write export to {}", path.display()))?;
    } else {
        std::io::stdout()
            .write_all(output.as_bytes())
            .context("Failed to write to stdout")?;
    }

    Ok(())
}
