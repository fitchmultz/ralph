//! Queue search subcommand.
//!
//! Responsibilities:
//! - Search tasks by content (title, evidence, plan, notes, request, tags, scope, custom fields).
//! - Support regex, fuzzy, and case-sensitive matching modes.
//! - Apply status, tag, scope, and scheduled filters alongside content search.
//! - Output in compact, long, or JSON formats.
//!
//! Not handled here:
//! - Task listing without content search (see `list.rs`).
//! - Task creation, modification, or deletion.
//!
//! Invariants/assumptions:
//! - Queue files are loaded and validated before searching.
//! - Search is performed after pre-filtering by status/tag/scope.
//! - JSON output uses the same task shape as queue export/show.

use anyhow::{Result, bail};
use clap::Args;

use crate::cli::{load_and_validate_queues, resolve_list_limit};
use crate::config::Resolved;
use crate::contracts::{Task, TaskStatus};
use crate::{outpututil, queue};

use super::{QueueListFormat, StatusArg};

/// Arguments for `ralph queue search`.
#[derive(Args)]
/// Search tasks by content (title, evidence, plan, notes, request, tags, scope, custom fields).
#[command(
    after_long_help = "Examples:\n  ralph queue search \"authentication\"\n  ralph queue search \"RQ-\\d{4}\" --regex\n  ralph queue search \"TODO\" --match-case\n  ralph queue search \"fix\" --status todo --tag rust\n  ralph queue search \"refactor\" --scope crates/ralph --tag rust\n  ralph queue search \"auth bug\" --fuzzy\n  ralph queue search \"fuzzy search\" --fuzzy --match-case"
)]
pub struct QueueSearchArgs {
    /// Search query (substring or regex pattern).
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Interpret query as a regular expression.
    #[arg(long)]
    pub regex: bool,

    /// Case-sensitive search (default: case-insensitive).
    #[arg(long)]
    pub match_case: bool,

    /// Use fuzzy matching for search (default: substring).
    #[arg(long)]
    pub fuzzy: bool,

    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    pub tag: Vec<String>,

    /// Filter by scope token (repeatable, case-insensitive; substring match).
    #[arg(long)]
    pub scope: Vec<String>,

    /// Include tasks from .ralph/done.json in search.
    #[arg(long)]
    pub include_done: bool,

    /// Only search tasks in .ralph/done.json (ignores active queue).
    #[arg(long)]
    pub only_done: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueListFormat::Compact)]
    pub format: QueueListFormat,

    /// Maximum results to show (0 = no limit).
    #[arg(long, default_value_t = 50)]
    pub limit: u32,

    /// Show all results (ignores --limit).
    #[arg(long)]
    pub all: bool,

    /// Filter to only show scheduled tasks (have scheduled_start set).
    #[arg(long)]
    pub scheduled: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueSearchArgs) -> Result<()> {
    if args.include_done && args.only_done {
        bail!(
            "Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only search done tasks."
        );
    }

    if args.fuzzy && args.regex {
        bail!(
            "Conflicting flags: --fuzzy and --regex are mutually exclusive. Choose either fuzzy matching or regex matching."
        );
    }

    let (queue_file, done_file) =
        load_and_validate_queues(resolved, args.include_done || args.only_done)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    let statuses: Vec<TaskStatus> = args.status.into_iter().map(|s| s.into()).collect();

    // Pre-filter by status/tag/scope using filter_tasks
    let mut prefiltered: Vec<&Task> = Vec::new();
    if !args.only_done {
        prefiltered.extend(queue::filter_tasks(
            &queue_file,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }
    if (args.include_done || args.only_done)
        && let Some(done_ref) = done_ref
    {
        prefiltered.extend(queue::filter_tasks(
            done_ref,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }

    // Apply scheduled filter if requested
    if args.scheduled {
        prefiltered.retain(|task| task.scheduled_start.is_some());
    }

    // Build search options
    let search_options = queue::SearchOptions {
        use_regex: args.regex,
        case_sensitive: args.match_case,
        use_fuzzy: args.fuzzy,
        scopes: args.scope.clone(),
    };

    // Apply content search
    let results =
        queue::search_tasks_with_options(prefiltered.into_iter(), &args.query, &search_options)?;

    let limit = resolve_list_limit(args.limit, args.all);
    let max = limit.unwrap_or(usize::MAX);
    let results: Vec<&Task> = results.into_iter().take(max).collect();

    match args.format {
        QueueListFormat::Compact => {
            for task in results {
                println!("{}", outpututil::format_task_compact(task));
            }
        }
        QueueListFormat::Long => {
            for task in results {
                println!("{}", outpututil::format_task_detailed(task));
            }
        }
        QueueListFormat::Json => {
            let owned_tasks: Vec<Task> = results.into_iter().cloned().collect();
            let json = serde_json::to_string_pretty(&owned_tasks)?;
            println!("{json}");
        }
    }

    Ok(())
}
