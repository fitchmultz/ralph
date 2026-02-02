//! `ralph queue ...` command group: Clap types and handler facade.
//!
//! Responsibilities:
//! - Define clap structures for queue-related subcommands.
//! - Route queue subcommands to their specific handlers.
//! - Re-export argument types used by queue commands.
//!
//! Not handled here:
//! - Queue persistence and locking semantics (see `crate::queue` and `crate::lock`).
//! - Task execution or runner behavior.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Queue state changes occur within the subcommand handlers.

mod archive;
mod burndown;
mod export;
mod graph;
mod history;
mod list;
mod next;
mod next_id;
mod prune;
mod repair;
mod schema;
mod search;
mod shared;
mod show;
mod sort;
mod stats;
mod stop;
mod unlock;
mod validate;

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config;

pub use burndown::QueueBurndownArgs;
pub use export::QueueExportArgs;
pub use graph::QueueGraphArgs;
pub use history::QueueHistoryArgs;
pub use list::QueueListArgs;
pub use next::QueueNextArgs;
pub use next_id::QueueNextIdArgs;
pub use prune::QueuePruneArgs;
pub use repair::RepairArgs;
pub use search::QueueSearchArgs;
pub use shared::{
    QueueExportFormat, QueueListFormat, QueueReportFormat, QueueShowFormat, QueueSortBy,
    QueueSortOrder, StatusArg,
};
pub use show::QueueShowArgs;
pub(crate) use show::show_task;
pub use sort::QueueSortArgs;
pub use stats::QueueStatsArgs;

pub fn handle_queue(cmd: QueueCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        QueueCommand::Validate => validate::handle(&resolved),
        QueueCommand::Next(args) => next::handle(&resolved, args),
        QueueCommand::NextId(args) => next_id::handle(&resolved, args),
        QueueCommand::Show(args) => show::handle(&resolved, args),
        QueueCommand::List(args) => list::handle(&resolved, args),
        QueueCommand::Search(args) => search::handle(&resolved, args),
        QueueCommand::Archive => archive::handle(&resolved, force),
        QueueCommand::Repair(args) => repair::handle(&resolved, force, args),
        QueueCommand::Unlock => unlock::handle(&resolved),
        QueueCommand::Sort(args) => sort::handle(&resolved, force, args),
        QueueCommand::Stats(args) => stats::handle(&resolved, args),
        QueueCommand::History(args) => history::handle(&resolved, args),
        QueueCommand::Burndown(args) => burndown::handle(&resolved, args),
        QueueCommand::Schema => schema::handle(),
        QueueCommand::Prune(args) => prune::handle(&resolved, force, args),
        QueueCommand::Graph(args) => graph::handle(&resolved, args),
        QueueCommand::Export(args) => export::handle(&resolved, args),
        QueueCommand::Stop => stop::handle(&resolved),
    }
}

#[derive(Args)]
#[command(
    about = "Inspect and manage the task queue",
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph queue next-id\n  ralph queue archive"
)]
pub struct QueueArgs {
    #[command(subcommand)]
    pub command: QueueCommand,
}

#[derive(Subcommand)]
pub enum QueueCommand {
    /// Validate the active queue (and done archive if present).
    #[command(
        after_long_help = "Examples:\n ralph queue validate\n ralph --verbose queue validate"
    )]
    Validate,

    /// Prune tasks from the done archive based on age, status, or keep-last rules.
    #[command(
        after_long_help = "Prune removes old tasks from .ralph/done.json while preserving recent history.\n\nSafety:\n --keep-last always protects the N most recently completed tasks (by completed_at).\n If no filters are provided, all tasks are pruned except those protected by --keep-last.\n Missing or invalid completed_at timestamps are treated as oldest for keep-last ordering\n but do NOT match the age filter (safety-first).\n\nExamples:\n ralph queue prune --dry-run --age 30 --status rejected\n ralph queue prune --keep-last 100\n ralph queue prune --age 90\n ralph queue prune --age 30 --status done --keep-last 50"
    )]
    Prune(QueuePruneArgs),

    /// Print the next todo task (ID by default).
    #[command(after_long_help = "Examples:\n ralph queue next\n ralph queue next --with-title")]
    Next(QueueNextArgs),

    /// Print the next available task ID (across queue + done archive).
    #[command(
        after_long_help = "Examples:\n ralph queue next-id\n ralph queue next-id --count 5\n ralph queue next-id -n 3\n ralph --verbose queue next-id"
    )]
    NextId(QueueNextIdArgs),

    /// Show a task by ID.
    Show(QueueShowArgs),

    /// List tasks in queue order.
    List(QueueListArgs),

    /// Search tasks by content (title, evidence, plan, notes, request, tags, scope, custom fields).
    #[command(
        after_long_help = "Examples:\n ralph queue search \"authentication\"\n ralph queue search \"RQ-\\d{4}\" --regex\n ralph queue search \"TODO\" --match-case\n ralph queue search \"fix\" --status todo --tag rust\n ralph queue search \"refactor\" --scope crates/ralph --tag rust\n ralph queue search \"auth bug\" --fuzzy\n ralph queue search \"fuzzy search\" --fuzzy --match-case"
    )]
    Search(QueueSearchArgs),

    /// Move completed tasks from queue.json to done.json.
    #[command(after_long_help = "Example:\n ralph queue archive")]
    Archive,

    /// Repair the queue and done files (fix missing fields, duplicates, timestamps).
    #[command(after_long_help = "Example:\n ralph queue repair\n ralph queue repair --dry-run")]
    Repair(RepairArgs),

    /// Remove the queue lock file.
    #[command(after_long_help = "Example:\n ralph queue unlock")]
    Unlock,

    /// Sort tasks by priority (reorders the queue file).
    #[command(
        after_long_help = "Examples:\n ralph queue sort\n ralph queue sort --order descending\n ralph queue sort --order ascending"
    )]
    Sort(QueueSortArgs),

    /// Show task statistics (completion rate, avg duration, tag breakdown).
    #[command(
        after_long_help = "Examples:\n ralph queue stats\n ralph queue stats --tag rust --tag cli"
    )]
    Stats(QueueStatsArgs),

    /// Show task history timeline (creation/completion events by day).
    #[command(after_long_help = "Examples:\n ralph queue history\n ralph queue history --days 14")]
    History(QueueHistoryArgs),

    /// Show burndown chart of remaining tasks over time.
    #[command(
        after_long_help = "Examples:\n ralph queue burndown\n ralph queue burndown --days 30"
    )]
    Burndown(QueueBurndownArgs),

    /// Print the JSON schema for the queue file.
    #[command(after_long_help = "Example:\n ralph queue schema")]
    Schema,

    /// Visualize task dependencies as a graph.
    #[command(
        after_long_help = "Examples:\n ralph queue graph\n ralph queue graph --task RQ-0001\n ralph queue graph --format dot\n ralph queue graph --critical\n ralph queue graph --reverse --task RQ-0001"
    )]
    Graph(QueueGraphArgs),

    /// Export task data to CSV, TSV, or JSON format.
    #[command(
        after_long_help = "Examples:\n ralph queue export\n ralph queue export --format csv --output tasks.csv\n ralph queue export --format json --status done\n ralph queue export --format tsv --tag rust --tag cli\n ralph queue export --include-archive --format csv\n ralph queue export --format csv --created-after 2026-01-01"
    )]
    Export(QueueExportArgs),

    /// Request graceful stop of a running loop after current task completes.
    #[command(
        after_long_help = "Examples:\n ralph queue stop\n\nNotes:\n - This creates a stop signal file that the run loop checks between tasks.\n - The current in-flight task will complete normally.\n - To force immediate termination, use Ctrl+C in the running loop."
    )]
    Stop,
}

#[cfg(test)]
mod tests;
