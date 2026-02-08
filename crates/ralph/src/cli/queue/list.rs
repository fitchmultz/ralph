//! Queue list subcommand.
//!
//! Responsibilities:
//! - List tasks from queue and done archive with various filters.
//! - Support status, tag, scope, dependency, and scheduled time filters.
//! - Output in compact, long, or JSON formats.
//! - Optionally display ETA estimates from execution history.
//!
//! Not handled here:
//! - Task creation, modification, or deletion (see other queue subcommands).
//! - Content-based search (see `search.rs`).
//! - Complex reporting or aggregation (see `reports` module).
//! - Real-time progress tracking (handled by external UI clients).
//!
//! Invariants/assumptions:
//! - Queue files are loaded and validated before filtering.
//! - Output ordering:
//!   - Default: active queue tasks are emitted in queue file order.
//!   - With --include-done: done tasks are appended after active tasks.
//!   - With --sort-by: tasks are sorted by the requested field (mixing statuses as needed).
//! - ETA is based on execution history only; missing history shows "n/a".
//! - ETA column is only added to text formats (compact/long), not JSON.
//! - Scheduled filters use RFC3339 or relative time expressions.
//! - Sorting is stable: all comparisons end with id tie-breaker for deterministic output.
//! - Missing/invalid timestamps sort last regardless of sort order (known before unknown).

use std::cmp::Ordering;

use anyhow::{Result, bail};
use clap::Args;
use time::OffsetDateTime;

use crate::cli::queue::shared::task_eta_display;
use crate::cli::{load_and_validate_queues, resolve_list_limit};
use crate::config::Resolved;
use crate::contracts::{Task, TaskStatus};
use crate::eta_calculator::EtaCalculator;
use crate::{outpututil, queue};

use super::{QueueListFormat, QueueListSortBy, QueueSortOrder, StatusArg};

/// Arguments for `ralph queue list`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue list --status doing --scope crates/ralph\n  ralph queue list --include-done --limit 20\n  ralph queue list --only-done --all\n  ralph queue list --filter-deps=RQ-0100\n  ralph queue list --format json\n  ralph queue list --format json | jq '.[] | select(.status == \"todo\")'\n  ralph queue list --scheduled\n  ralph queue list --scheduled-after '2026-01-01T00:00:00Z'\n  ralph queue list --scheduled-before '+7d'\n  ralph queue list --with-eta\n  ralph queue list --with-eta --format long\n  ralph queue list --sort-by updated_at\n  ralph queue list --scheduled --sort-by scheduled_start --order ascending\n  ralph queue list --scheduled-after '+0d' --sort-by scheduled_start --order ascending"
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

    /// Sort by field (supported: priority, created_at, updated_at, started_at, scheduled_start, status, title).
    /// Missing/invalid timestamps sort last regardless of order.
    #[arg(long, value_enum)]
    pub sort_by: Option<QueueListSortBy>,

    /// Sort order (default: descending).
    #[arg(long, value_enum, default_value_t = QueueSortOrder::Descending)]
    pub order: QueueSortOrder,

    /// Suppress size warning output.
    #[arg(long, short)]
    pub quiet: bool,

    /// Filter to only show scheduled tasks (have scheduled_start set).
    #[arg(long)]
    pub scheduled: bool,

    /// Filter tasks scheduled after this time (RFC3339 or relative expression).
    #[arg(long, value_name = "TIMESTAMP")]
    pub scheduled_after: Option<String>,

    /// Filter tasks scheduled before this time (RFC3339 or relative expression).
    #[arg(long, value_name = "TIMESTAMP")]
    pub scheduled_before: Option<String>,

    /// Include an execution-history-based ETA estimate column (text formats only).
    #[arg(long)]
    pub with_eta: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueListArgs) -> Result<()> {
    if args.include_done && args.only_done {
        bail!(
            "Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only show done tasks."
        );
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
    if (args.include_done || args.only_done)
        && let Some(done_ref) = done_ref
    {
        tasks.extend(queue::filter_tasks(
            done_ref,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
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

    // Apply scheduling filters
    let tasks: Vec<&Task> = tasks
        .into_iter()
        .filter(|t| {
            // --scheduled flag: only show tasks with scheduled_start set
            if args.scheduled && t.scheduled_start.is_none() {
                return false;
            }

            // --scheduled-after filter
            if let Some(ref after) = args.scheduled_after {
                if let Some(ref scheduled) = t.scheduled_start {
                    if let Ok(scheduled_dt) = crate::timeutil::parse_rfc3339(scheduled)
                        && let Ok(after_dt) = crate::timeutil::parse_relative_time(after)
                            .and_then(|s| crate::timeutil::parse_rfc3339(&s))
                        && scheduled_dt <= after_dt
                    {
                        return false;
                    }
                } else {
                    // Task has no scheduled_start, so it doesn't satisfy "after" filter
                    return false;
                }
            }

            // --scheduled-before filter
            if let Some(ref before) = args.scheduled_before {
                if let Some(ref scheduled) = t.scheduled_start {
                    if let Ok(scheduled_dt) = crate::timeutil::parse_rfc3339(scheduled)
                        && let Ok(before_dt) = crate::timeutil::parse_relative_time(before)
                            .and_then(|s| crate::timeutil::parse_rfc3339(&s))
                        && scheduled_dt >= before_dt
                    {
                        return false;
                    }
                } else {
                    // Task has no scheduled_start, so it doesn't satisfy "before" filter
                    return false;
                }
            }

            true
        })
        .collect();

    // Apply sort if specified
    let tasks = if let Some(sort_by) = args.sort_by {
        let descending = args.order.is_descending();
        let mut sorted = tasks;
        sorted.sort_by(|a, b| {
            let ord = match sort_by {
                QueueListSortBy::Priority => {
                    let ord = a.priority.cmp(&b.priority);
                    if descending { ord.reverse() } else { ord }
                }
                QueueListSortBy::CreatedAt => cmp_optional_rfc3339_missing_last(
                    a.created_at.as_deref(),
                    b.created_at.as_deref(),
                    descending,
                ),
                QueueListSortBy::UpdatedAt => cmp_optional_rfc3339_missing_last(
                    a.updated_at.as_deref(),
                    b.updated_at.as_deref(),
                    descending,
                ),
                QueueListSortBy::StartedAt => cmp_optional_rfc3339_missing_last(
                    a.started_at.as_deref(),
                    b.started_at.as_deref(),
                    descending,
                ),
                QueueListSortBy::ScheduledStart => cmp_optional_rfc3339_missing_last(
                    a.scheduled_start.as_deref(),
                    b.scheduled_start.as_deref(),
                    descending,
                ),
                QueueListSortBy::Status => {
                    let ord = status_rank(a.status).cmp(&status_rank(b.status));
                    if descending { ord.reverse() } else { ord }
                }
                QueueListSortBy::Title => {
                    let ord = cmp_ascii_case_insensitive(&a.title, &b.title)
                        .then_with(|| a.title.cmp(&b.title));
                    if descending { ord.reverse() } else { ord }
                }
            };

            ord.then_with(|| a.id.cmp(&b.id))
        });
        sorted
    } else {
        tasks
    };

    // Helper functions for sorting
    fn cmp_optional_rfc3339_missing_last(
        a: Option<&str>,
        b: Option<&str>,
        descending: bool,
    ) -> Ordering {
        let a_dt: Option<OffsetDateTime> = a.and_then(crate::timeutil::parse_rfc3339_opt);
        let b_dt: Option<OffsetDateTime> = b.and_then(crate::timeutil::parse_rfc3339_opt);

        match (a_dt, b_dt) {
            (Some(a_dt), Some(b_dt)) => {
                let ord = a_dt.cmp(&b_dt);
                if descending { ord.reverse() } else { ord }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }

    fn status_rank(s: TaskStatus) -> u8 {
        match s {
            TaskStatus::Draft => 0,
            TaskStatus::Todo => 1,
            TaskStatus::Doing => 2,
            TaskStatus::Done => 3,
            TaskStatus::Rejected => 4,
        }
    }

    fn cmp_ascii_case_insensitive(a: &str, b: &str) -> Ordering {
        a.bytes()
            .map(|c| c.to_ascii_lowercase())
            .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
    }

    let max = limit.unwrap_or(usize::MAX);
    let tasks: Vec<&Task> = tasks.into_iter().take(max).collect();

    // Load ETA calculator if needed (only for text formats)
    let eta_calculator = if args.with_eta && args.format != QueueListFormat::Json {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        Some(EtaCalculator::load(&cache_dir))
    } else {
        None
    };

    match args.format {
        QueueListFormat::Compact => {
            for task in tasks {
                let base = outpututil::format_task_compact(task);
                if let Some(ref calc) = eta_calculator {
                    let eta = task_eta_display(resolved, calc, task);
                    println!("{}\t{}", base, eta);
                } else {
                    println!("{}", base);
                }
            }
        }
        QueueListFormat::Long => {
            for task in tasks {
                let base = outpututil::format_task_detailed(task);
                if let Some(ref calc) = eta_calculator {
                    let eta = task_eta_display(resolved, calc, task);
                    println!("{}\t{}", base, eta);
                } else {
                    println!("{}", base);
                }
            }
        }
        QueueListFormat::Json => {
            // JSON format ignores --with-eta per design
            let owned_tasks: Vec<Task> = tasks.into_iter().cloned().collect();
            let json = serde_json::to_string_pretty(&owned_tasks)?;
            println!("{json}");
        }
    }

    Ok(())
}
