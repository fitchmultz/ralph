//! Queue list subcommand.

use anyhow::{bail, Result};
use clap::Args;

use crate::cli::{load_and_validate_queues, resolve_list_limit};
use crate::config::Resolved;
use crate::contracts::{Task, TaskStatus};
use crate::{outpututil, queue};

use super::{QueueListFormat, QueueSortBy, QueueSortOrder, StatusArg};

/// Arguments for `ralph queue list`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue list --status doing --scope crates/ralph\n  ralph queue list --include-done --limit 20\n  ralph queue list --only-done --all\n  ralph queue list --filter-deps=RQ-0100"
)]
pub struct QueueListArgs {
    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    pub tag: Vec<String>,

    /// Filter by scope token (repeatable, case-insensitive; substring match).
    #[arg(long)]
    pub scope: Vec<String>,

    /// Filter by tasks that depend on the given task ID (recursively).
    #[arg(long)]
    pub filter_deps: Option<String>,

    /// Include tasks from .ralph/done.json after active queue output.
    #[arg(long)]
    pub include_done: bool,

    /// Only list tasks from .ralph/done.json (ignores active queue).
    #[arg(long)]
    pub only_done: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueListFormat::Compact)]
    pub format: QueueListFormat,

    /// Maximum tasks to show (0 = no limit).
    #[arg(long, default_value_t = 50)]
    pub limit: u32,

    /// Show all tasks (ignores --limit).
    #[arg(long)]
    pub all: bool,

    /// Sort by field (supported: priority).
    #[arg(long, value_enum)]
    pub sort_by: Option<QueueSortBy>,

    /// Sort order (default: descending).
    #[arg(long, value_enum, default_value_t = QueueSortOrder::Descending)]
    pub order: QueueSortOrder,

    /// Suppress size warning output.
    #[arg(long, short)]
    pub quiet: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueListArgs) -> Result<()> {
    if args.include_done && args.only_done {
        bail!("Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only show done tasks.");
    }

    let (queue_file, done_file) =
        load_and_validate_queues(resolved, args.include_done || args.only_done)?;

    // Check queue size and print warning if needed
    if !args.quiet {
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
            queue::print_size_warning_if_needed(&result, args.quiet);
        }
    }
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    let statuses: Vec<TaskStatus> = args.status.into_iter().map(|s| s.into()).collect();
    let limit = resolve_list_limit(args.limit, args.all);

    let mut tasks: Vec<&Task> = Vec::new();
    if !args.only_done {
        tasks.extend(queue::filter_tasks(
            &queue_file,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }
    if args.include_done || args.only_done {
        if let Some(done_ref) = done_ref {
            tasks.extend(queue::filter_tasks(
                done_ref,
                &statuses,
                &args.tag,
                &args.scope,
                None,
            ));
        }
    }

    // Apply dependency filter if specified
    let tasks = if let Some(ref root_id) = args.filter_deps {
        let dependents_list = queue::get_dependents(root_id, &queue_file, done_ref);
        let dependents: std::collections::HashSet<&str> =
            dependents_list.iter().map(|s| s.as_str()).collect();
        tasks
            .into_iter()
            .filter(|t| dependents.contains(t.id.trim()))
            .collect()
    } else {
        tasks
    };

    // Apply sort if specified
    let tasks = if let Some(sort_by) = args.sort_by {
        match sort_by {
            QueueSortBy::Priority => {
                let mut sorted = tasks;
                sorted.sort_by(|a, b| {
                    // Since Ord has Critical > High > Medium > Low (semantically),
                    // we reverse for descending to put higher priority first.
                    let ord = if args.order.is_descending() {
                        a.priority.cmp(&b.priority).reverse()
                    } else {
                        a.priority.cmp(&b.priority)
                    };
                    match ord {
                        std::cmp::Ordering::Equal => a.id.cmp(&b.id),
                        other => other,
                    }
                });
                sorted
            }
        }
    } else {
        tasks
    };

    let max = limit.unwrap_or(usize::MAX);
    for task in tasks.into_iter().take(max) {
        match args.format {
            QueueListFormat::Compact => println!("{}", outpututil::format_task_compact(task)),
            QueueListFormat::Long => println!("{}", outpututil::format_task_detailed(task)),
        }
    }

    Ok(())
}
