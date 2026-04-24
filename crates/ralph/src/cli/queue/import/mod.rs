//! Queue import subcommand for importing tasks from CSV, TSV, or JSON.
//!
//! Purpose:
//! - Queue import subcommand for importing tasks from CSV, TSV, or JSON.
//!
//! Responsibilities:
//! - Define the CLI surface for queue import policy and input selection.
//! - Orchestrate input loading, parsing, normalization, merging, and validation.
//! - Keep the facade thin while delegating parsing and mutation details to helpers.
//!
//! Not handled here:
//! - Export functionality (see `crate::cli::queue::export`).
//! - GUI-specific import workflows (this is a CLI command).
//! - Complex schema migration between versions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Always acquire queue lock before mutating queue files.
//! - Never write to disk on parse or validation failures.
//! - Undo snapshots are created only after the merged queue validates cleanly.

mod input;
mod merge;
mod normalize;
mod parse;
mod report;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::config::Resolved;
use crate::queue;

use super::QueueImportFormat;
use input::read_input;
use merge::merge_imported_tasks;
use normalize::normalize_task;
use parse::{parse_csv_tasks, parse_json_tasks};
use report::ImportReport;

/// Arguments for `ralph queue import`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue export --format json | ralph queue import --format json --dry-run\n  ralph queue import --format csv --input tasks.csv\n  ralph queue import --format tsv --input - --on-duplicate rename < tasks.tsv\n  ralph queue import --format json --input tasks.json --on-duplicate skip"
)]
pub struct QueueImportArgs {
    /// Input format.
    #[arg(long, value_enum)]
    pub format: QueueImportFormat,

    /// Input file path (default: stdin). Use '-' for stdin.
    #[arg(long, short)]
    pub input: Option<PathBuf>,

    /// Show what would change without writing to disk.
    #[arg(long)]
    pub dry_run: bool,

    /// What to do if an imported task ID already exists.
    #[arg(long, value_enum, default_value_t = OnDuplicate::Fail)]
    pub on_duplicate: OnDuplicate,
}

/// Policy for handling duplicate task IDs during import.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum OnDuplicate {
    /// Fail with an error if a duplicate ID is found.
    Fail,
    /// Skip duplicate tasks and continue importing others.
    Skip,
    /// Generate a new ID for duplicate tasks.
    Rename,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueImportArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue import", force)?;
    let input = read_input(args.input.as_ref()).context("read import input")?;

    let mut imported = match args.format {
        QueueImportFormat::Json => parse_json_tasks(&input)?,
        QueueImportFormat::Csv => parse_csv_tasks(&input, b',')?,
        QueueImportFormat::Tsv => parse_csv_tasks(&input, b'\t')?,
    };

    let now = crate::timeutil::now_utc_rfc3339_or_fallback();
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    let (mut queue_file, done_file) = crate::queue::load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|done| !done.tasks.is_empty() || resolved.done_path.exists());

    for task in &mut imported {
        normalize_task(task, &now);
    }

    let report = merge_imported_tasks(
        &mut queue_file,
        done_ref,
        imported,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
        &now,
        args.on_duplicate,
    )?;

    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    if !args.dry_run {
        crate::undo::create_undo_snapshot(resolved, "queue import")?;
    }

    if args.dry_run {
        log::info!("Dry run: no changes written. {}", report.summary());
        return Ok(());
    }

    queue::save_queue(&resolved.queue_path, &queue_file)?;
    log::info!("Imported tasks. {}", report.summary());
    Ok(())
}
