//! Clap arguments for `ralph queue export`.
//!
//! Purpose:
//! - Clap arguments for `ralph queue export`.
//!
//! Responsibilities:
//! - Define the user-facing flags and help text for queue export.
//! - Keep argument parsing concerns separate from queue loading and rendering.
//! - Preserve the public CLI contract for export workflows.
//!
//! Not handled here:
//! - Export execution logic.
//! - Task filtering or format rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Defaults match the existing CLI surface.
//! - Help examples stay aligned with supported output formats and filters.

use std::path::PathBuf;

use clap::Args;

use crate::cli::queue::{QueueExportFormat, StatusArg};

/// Arguments for `ralph queue export`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue export\n  ralph queue export --format csv --output tasks.csv\n  ralph queue export --format json --status done\n  ralph queue export --format tsv --tag rust --tag cli\n  ralph queue export --include-archive --format csv\n  ralph queue export --format csv --created-after 2026-01-01\n  ralph queue export --format md --status todo\n  ralph queue export --format gh --status doing"
)]
pub struct QueueExportArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueExportFormat::Csv)]
    pub format: QueueExportFormat,

    /// Output file path (default: stdout).
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    pub tag: Vec<String>,

    /// Filter by scope token (repeatable, case-insensitive; substring match).
    #[arg(long)]
    pub scope: Vec<String>,

    /// Filter by task ID pattern (substring match).
    #[arg(long)]
    pub id_pattern: Option<String>,

    /// Filter tasks created after this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    pub created_after: Option<String>,

    /// Filter tasks created before this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    pub created_before: Option<String>,

    /// Include tasks from .ralph/done.jsonc archive.
    #[arg(long)]
    pub include_archive: bool,

    /// Only export tasks from .ralph/done.jsonc (ignores active queue).
    #[arg(long)]
    pub only_archive: bool,

    /// Suppress size warning output.
    #[arg(long, short)]
    pub quiet: bool,
}
