//! `ralph queue ...` command group: Clap types and handler.

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};

use super::{load_and_validate_queues, resolve_list_limit};
use crate::contracts::{Task, TaskStatus};
use crate::{completions, config, contracts, fsutil, outpututil, queue, reports, timeutil};

pub fn handle_queue(cmd: QueueCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        QueueCommand::Validate => {
            load_and_validate_queues(&resolved, true)?;
        }
        QueueCommand::Next(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            if let Some(next) = queue::next_todo_task(&queue_file) {
                if args.with_title {
                    println!(
                        "{}",
                        outpututil::format_task_id_title(&next.id, &next.title)
                    );
                } else {
                    println!("{}", outpututil::format_task_id(&next.id));
                }
                return Ok(());
            }

            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            let next_id = queue::next_id_across(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            println!("{next_id}");
        }
        QueueCommand::NextId => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            let next = queue::next_id_across(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            println!("{next}");
        }
        QueueCommand::Show(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

            let task = queue::find_task_across(&queue_file, done_ref, &args.task_id)
                .ok_or_else(|| anyhow::anyhow!("task not found: {}", args.task_id.trim()))?;

            match args.format {
                QueueShowFormat::Json => {
                    let rendered = serde_json::to_string_pretty(task)?;
                    print!("{rendered}");
                }
                QueueShowFormat::Compact => {
                    println!("{}", outpututil::format_task_compact(task));
                }
            }
        }
        QueueCommand::List(args) => {
            if args.include_done && args.only_done {
                bail!("Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only show done tasks.");
            }

            let (queue_file, done_file) =
                load_and_validate_queues(&resolved, args.include_done || args.only_done)?;
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
            let tasks = if let Some(ref sort_by) = args.sort_by {
                match sort_by.as_str() {
                    "priority" => {
                        let mut sorted = tasks;
                        sorted.sort_by(|a, b| {
                            // Since Ord has Critical > High > Medium > Low (semantically),
                            // we reverse for descending to put higher priority first
                            let ord = if args.descending {
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
                    _ => tasks,
                }
            } else {
                tasks
            };

            let max = limit.unwrap_or(usize::MAX);
            for task in tasks.into_iter().take(max) {
                match args.format {
                    QueueListFormat::Compact => {
                        println!("{}", outpututil::format_task_compact(task))
                    }
                    QueueListFormat::Long => println!("{}", outpututil::format_task_detailed(task)),
                }
            }
        }
        QueueCommand::Search(args) => {
            if args.include_done && args.only_done {
                bail!("Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only search done tasks.");
            }

            let (queue_file, done_file) =
                load_and_validate_queues(&resolved, args.include_done || args.only_done)?;
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
            if args.include_done || args.only_done {
                if let Some(done_ref) = done_ref {
                    prefiltered.extend(queue::filter_tasks(
                        done_ref,
                        &statuses,
                        &args.tag,
                        &args.scope,
                        None,
                    ));
                }
            }

            // Apply content search
            let results = queue::search_tasks(
                prefiltered.into_iter(),
                &args.query,
                args.regex,
                args.match_case,
            )?;

            let limit = resolve_list_limit(args.limit, args.all);
            let max = limit.unwrap_or(usize::MAX);
            for task in results.into_iter().take(max) {
                match args.format {
                    QueueListFormat::Compact => {
                        println!("{}", outpututil::format_task_compact(task))
                    }
                    QueueListFormat::Long => println!("{}", outpututil::format_task_detailed(task)),
                }
            }
        }
        QueueCommand::Done => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue done", force)?;
            let report = queue::archive_done_tasks(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            if report.moved_ids.is_empty() {
                log::info!("No done tasks to move.");
            } else {
                log::info!("Moved {} done task(s).", report.moved_ids.len());
            }
        }
        QueueCommand::Complete(args) => {
            let status = match args.status {
                StatusArg::Done => TaskStatus::Done,
                StatusArg::Rejected => TaskStatus::Rejected,
                _ => bail!("Invalid completion status: only 'done' or 'rejected' are allowed."),
            };
            let lock_dir = fsutil::queue_lock_dir(&resolved.repo_root);
            if fsutil::is_supervising_process(&lock_dir)? {
                let signal = completions::CompletionSignal {
                    task_id: args.task_id.clone(),
                    status,
                    notes: args.note.clone(),
                };
                let path = completions::write_completion_signal(&resolved.repo_root, &signal)?;
                log::info!(
                    "Running under supervision - wrote completion signal at {}",
                    path.display()
                );
                return Ok(());
            }
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue complete", force)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::complete_task(
                &resolved.queue_path,
                &resolved.done_path,
                &args.task_id,
                status,
                &now,
                &args.note,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            log::info!("Task {} completed and moved to done archive.", args.task_id);
        }
        QueueCommand::Repair(args) => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue repair", force)?;
            let report = queue::repair_queue(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
                args.dry_run,
            )?;

            if report.is_empty() {
                log::info!("No issues found. Queue is healthy.");
            } else {
                log::info!("Repair report:");
                if report.fixed_tasks > 0 {
                    log::info!("  Fixed missing fields in {} task(s)", report.fixed_tasks);
                }
                if report.fixed_timestamps > 0 {
                    log::info!(
                        "  Fixed invalid timestamps in {} task(s)",
                        report.fixed_timestamps
                    );
                }
                if !report.remapped_ids.is_empty() {
                    log::info!("  Remapped {} duplicate ID(s):", report.remapped_ids.len());
                    for (old, new) in &report.remapped_ids {
                        log::info!("    {} -> {}", old, new);
                    }
                }
                if args.dry_run {
                    log::info!("Dry run: no changes written to disk.");
                } else {
                    log::info!("Repaired queue written to disk.");
                }
            }
        }
        QueueCommand::Unlock => {
            let lock_dir = fsutil::queue_lock_dir(&resolved.repo_root);
            if lock_dir.exists() {
                std::fs::remove_dir_all(&lock_dir)
                    .with_context(|| format!("remove lock dir {}", lock_dir.display()))?;
                log::info!("Queue unlocked (removed {}).", lock_dir.display());
            } else {
                log::info!("Queue is not locked.");
            }
        }
        QueueCommand::SetStatus {
            task_id,
            status,
            note,
        } => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue set-status", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(
                &mut queue_file,
                &task_id,
                status.into(),
                &now,
                note.as_deref(),
            )?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
        }
        QueueCommand::SetField {
            task_id,
            key,
            value,
        } => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue set-field", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_field(&mut queue_file, &task_id, &key, &value, &now)?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!("Set field '{}' on task {}.", key, task_id);
        }
        QueueCommand::Sort(args) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue sort", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            match args.sort_by.as_str() {
                "priority" => {
                    queue::sort_tasks_by_priority(&mut queue_file, args.descending);
                }
                _ => {
                    bail!(
                        "Unsupported sort field: {}. Supported fields: priority",
                        args.sort_by
                    );
                }
            }

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!(
                "Queue sorted by {} (descending: {}).",
                args.sort_by,
                args.descending
            );
        }
        QueueCommand::Stats(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            reports::print_stats(&queue_file, done_ref, &args.tag)?;
        }
        QueueCommand::History(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            reports::print_history(&queue_file, done_ref, args.days)?;
        }
        QueueCommand::Burndown(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            reports::print_burndown(&queue_file, done_ref, args.days)?;
        }
        QueueCommand::Schema => {
            let schema = schemars::schema_for!(contracts::QueueFile);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
        QueueCommand::Prune(args) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue prune", force)?;
            let report: queue::PruneReport = queue::prune_done_tasks(
                &resolved.done_path,
                queue::PruneOptions {
                    age_days: args.age,
                    statuses: args.status.into_iter().map(|s| s.into()).collect(),
                    keep_last: args.keep_last,
                    dry_run: args.dry_run,
                },
            )?;
            if args.dry_run {
                log::info!("Dry run: would prune {} task(s).", report.pruned_ids.len());
                if !report.pruned_ids.is_empty() {
                    log::info!("Pruned IDs: {}", report.pruned_ids.join(", "));
                }
                if !report.kept_ids.is_empty() {
                    log::info!("Kept IDs: {}", report.kept_ids.join(", "));
                }
            } else {
                if report.pruned_ids.is_empty() {
                    log::info!("No tasks pruned.");
                } else {
                    log::info!("Pruned {} task(s).", report.pruned_ids.len());
                }
                if !report.kept_ids.is_empty() {
                    log::debug!("Kept {} task(s).", report.kept_ids.len());
                }
            }
        }
    }
    Ok(())
}

#[derive(Args)]
#[command(
    about = "Inspect and manage the task queue",
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph queue next-id\n  ralph queue complete RQ-0001 done --note \"Completed task\"\n  ralph queue set-status RQ-0001 doing --note \"Starting work\""
)]
pub struct QueueArgs {
    #[command(subcommand)]
    pub command: QueueCommand,
}

#[derive(Subcommand)]
pub enum QueueCommand {
    /// Validate the active queue (and done archive if present).
    #[command(after_long_help = "Example:\n  ralph queue validate")]
    Validate,
    /// Prune tasks from the done archive based on age, status, or keep-last rules.
    #[command(
        after_long_help = "Prune removes old tasks from .ralph/done.json while preserving recent history.\n\nSafety:\n  --keep-last always protects the N most recently completed tasks (by completed_at).\n  If no filters are provided, all tasks are pruned except those protected by --keep-last.\n  Missing or invalid completed_at timestamps are treated as oldest for keep-last ordering\n  but do NOT match the age filter (safety-first).\n\nExamples:\n  ralph queue prune --dry-run --age 30 --status rejected\n  ralph queue prune --keep-last 100\n  ralph queue prune --age 90\n  ralph queue prune --age 30 --status done --keep-last 50"
    )]
    Prune(QueuePruneArgs),
    /// Print the next todo task (ID by default).
    #[command(after_long_help = "Examples:\n  ralph queue next\n  ralph queue next --with-title")]
    Next(QueueNextArgs),
    /// Print the next available task ID (across queue + done archive).
    #[command(after_long_help = "Example:\n  ralph queue next-id")]
    NextId,
    /// Show a task by ID.
    Show(QueueShowArgs),
    /// List tasks in queue order.
    List(QueueListArgs),
    /// Search tasks by content (title, evidence, plan, notes).
    #[command(
        after_long_help = "Examples:\n  ralph queue search \"authentication\"\n  ralph queue search \"RQ-\\d{4}\" --regex\n  ralph queue search \"TODO\" --match-case\n  ralph queue search \"fix\" --status todo --tag rust"
    )]
    Search(QueueSearchArgs),
    /// Move completed tasks from queue.json to done.json.
    #[command(after_long_help = "Example:\n  ralph queue done")]
    Done,
    /// Complete a task and move it to the done archive.
    #[command(
        after_long_help = "Examples:\n  ralph queue complete RQ-0001 done\n  ralph queue complete RQ-0002 rejected --note \"No longer needed\""
    )]
    Complete(QueueCompleteArgs),
    /// Repair the queue and done files (fix missing fields, duplicates, timestamps).
    #[command(after_long_help = "Example:\n  ralph queue repair\n  ralph queue repair --dry-run")]
    Repair(RepairArgs),
    /// Remove the queue lock file.
    #[command(after_long_help = "Example:\n  ralph queue unlock")]
    Unlock,
    /// Update a task status in the active queue.
    #[command(
        after_long_help = "Example:\n  ralph queue set-status RQ-0001 doing --note \"Starting work\""
    )]
    SetStatus {
        task_id: String,
        status: StatusArg,
        #[arg(long)]
        note: Option<String>,
    },
    /// Set a custom field on a task.
    #[command(
        after_long_help = "Examples:\n  ralph queue set-field RQ-0001 severity high\n  ralph queue set-field RQ-0002 complexity \"O(n log n)\""
    )]
    SetField {
        task_id: String,
        /// Custom field key (must not contain whitespace).
        key: String,
        /// Custom field value.
        value: String,
    },
    /// Sort tasks by priority (reorders the queue file).
    #[command(after_long_help = "Examples:\n  ralph queue sort\n  ralph queue sort --descending")]
    Sort(QueueSortArgs),
    /// Show task statistics (completion rate, avg duration, tag breakdown).
    #[command(
        after_long_help = "Examples:\n  ralph queue stats\n  ralph queue stats --tag rust --tag cli"
    )]
    Stats(QueueStatsArgs),
    /// Show task history timeline (creation/completion events by day).
    #[command(
        after_long_help = "Examples:\n  ralph queue history\n  ralph queue history --days 14"
    )]
    History(QueueHistoryArgs),
    /// Show burndown chart of remaining tasks over time.
    #[command(
        after_long_help = "Examples:\n  ralph queue burndown\n  ralph queue burndown --days 30"
    )]
    Burndown(QueueBurndownArgs),
    /// Print the JSON schema for the queue file.
    #[command(after_long_help = "Example:\n  ralph queue schema")]
    Schema,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum StatusArg {
    /// Task is waiting to be started.
    Todo,
    /// Task is currently being worked on.
    Doing,
    /// Task is complete.
    Done,
    /// Task was rejected (dependents can proceed).
    Rejected,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueShowFormat {
    /// Full JSON representation of the task.
    Json,
    /// Compact tab-separated summary (ID, status, title).
    Compact,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum QueueListFormat {
    /// Compact tab-separated summary (ID, status, title).
    Compact,
    /// Detailed tab-separated format including tags, scope, and timestamps.
    Long,
}

#[derive(Args)]
#[command(after_long_help = "Example:\n  ralph queue next --with-title")]
pub struct QueueNextArgs {
    /// Include the task title after the ID.
    #[arg(long)]
    pub with_title: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue show RQ-0001\n  ralph queue show RQ-0001 --format compact"
)]
pub struct QueueShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Json)]
    pub format: QueueShowFormat,
}

#[derive(Args)]
pub struct QueueCompleteArgs {
    /// Task ID to complete.
    pub task_id: String,

    /// Completion status (done or rejected).
    #[arg(value_enum)]
    pub status: StatusArg,

    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,
}

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

    /// Sort by field (e.g., priority).
    #[arg(long)]
    pub sort_by: Option<String>,

    /// Sort in descending order.
    #[arg(long)]
    pub descending: bool,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph queue sort\n  ralph queue sort --descending")]
pub struct QueueSortArgs {
    /// Sort by field (default: priority).
    #[arg(long, default_value = "priority")]
    pub sort_by: String,

    /// Sort in descending order (highest priority first).
    #[arg(long)]
    pub descending: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue search \"authentication\"\n  ralph queue search \"RQ-\\d{4}\" --regex\n  ralph queue search \"TODO\" --match-case\n  ralph queue search \"fix\" --status todo --tag rust"
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
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue stats\n  ralph queue stats --tag rust --tag cli"
)]
pub struct QueueStatsArgs {
    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    pub tag: Vec<String>,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph queue history\n  ralph queue history --days 14")]
pub struct QueueHistoryArgs {
    /// Number of days to show (default: 7).
    #[arg(long, default_value_t = 7)]
    pub days: u32,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph queue burndown\n  ralph queue burndown --days 30")]
pub struct QueueBurndownArgs {
    /// Number of days to show (default: 7).
    #[arg(long, default_value_t = 7)]
    pub days: u32,
}

#[derive(Args)]
pub struct RepairArgs {
    /// Show what would be changed without writing to disk.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Prune removes old tasks from .ralph/done.json while preserving recent history.\n\nSafety:\n  --keep-last always protects the N most recently completed tasks (by completed_at).\n  If no filters are provided, all tasks are pruned except those protected by --keep-last.\n  Missing or invalid completed_at timestamps are treated as oldest for keep-last ordering\n  but do NOT match the age filter (safety-first).\n\nExamples:\n  ralph queue prune --dry-run --age 30 --status rejected\n  ralph queue prune --keep-last 100\n  ralph queue prune --age 90\n  ralph queue prune --age 30 --status done --keep-last 50"
)]
pub struct QueuePruneArgs {
    /// Only prune tasks completed at least N days ago.
    #[arg(long)]
    pub age: Option<u32>,

    /// Filter by task status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Keep the N most recently completed tasks regardless of filters.
    #[arg(long)]
    pub keep_last: Option<u32>,

    /// Show what would be pruned without writing to disk.
    #[arg(long)]
    pub dry_run: bool,
}

impl From<StatusArg> for contracts::TaskStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Todo => contracts::TaskStatus::Todo,
            StatusArg::Doing => contracts::TaskStatus::Doing,
            StatusArg::Done => contracts::TaskStatus::Done,
            StatusArg::Rejected => contracts::TaskStatus::Rejected,
        }
    }
}
