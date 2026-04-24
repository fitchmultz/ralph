//! `ralph queue graph` subcommand facade.
//!
//! Purpose:
//! - `ralph queue graph` subcommand facade.
//!
//! Responsibilities:
//! - Define CLI arguments for graph rendering.
//! - Load validated queue data and dispatch to format-specific renderers.
//! - Keep command-level behavior stable while renderer internals evolve.
//!
//! Not handled here:
//! - Dependency graph construction (see `crate::queue::graph`).
//! - Format-specific rendering details (see sibling renderer modules).
//! - Queue file mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue files are already validated before rendering.
//! - Focus task IDs are trimmed before lookup.
//! - Rendering writes directly to stdout.

mod dot;
mod json;
mod list;
mod shared;
mod tree;

use anyhow::{Result, anyhow};
use clap::{Args, ValueEnum};

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;
use crate::queue::find_task_across;
use crate::queue::graph::{GraphFormat, build_graph, find_critical_paths};

/// Arguments for `ralph queue graph`.
#[derive(Args)]
#[command(
    about = "Visualize task dependencies as a graph",
    after_long_help = "Examples:\n  ralph queue graph\n  ralph queue graph --task RQ-0001\n  ralph queue graph --format dot\n  ralph queue graph --critical\n  ralph queue graph --reverse --task RQ-0001"
)]
pub struct QueueGraphArgs {
    /// Focus on a specific task (show its dependency tree).
    #[arg(long, short)]
    pub task: Option<String>,

    /// Output format.
    #[arg(long, short, value_enum, default_value_t = GraphFormatArg::Tree)]
    pub format: GraphFormatArg,

    /// Include completed tasks in output.
    #[arg(long)]
    pub include_done: bool,

    /// Show only critical path.
    #[arg(long, short)]
    pub critical: bool,

    /// Show reverse dependencies (what this task blocks).
    #[arg(long, short)]
    pub reverse: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum GraphFormatArg {
    /// ASCII art tree (default).
    Tree,
    /// Graphviz DOT format.
    Dot,
    /// JSON for external tools.
    Json,
    /// Flat list with indentation.
    List,
}

impl From<GraphFormatArg> for GraphFormat {
    fn from(arg: GraphFormatArg) -> Self {
        match arg {
            GraphFormatArg::Tree => GraphFormat::Tree,
            GraphFormatArg::Dot => GraphFormat::Dot,
            GraphFormatArg::Json => GraphFormat::Json,
            GraphFormatArg::List => GraphFormat::List,
        }
    }
}

pub(crate) fn handle(resolved: &Resolved, args: QueueGraphArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues_read_only(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    let graph = build_graph(&queue_file, done_ref);
    if graph.is_empty() {
        println!("No tasks found in queue.");
        return Ok(());
    }

    let critical_paths = if args.critical || matches!(args.format, GraphFormatArg::Tree) {
        find_critical_paths(&graph)
    } else {
        Vec::new()
    };

    match args
        .task
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(task_id) => {
            let task = find_task_across(&queue_file, done_ref, task_id)
                .ok_or_else(|| anyhow!("{}", crate::error_messages::task_not_found(task_id)))?;
            render_focused_graph(&graph, task, task_id, &critical_paths, &args)
        }
        None => render_full_graph(&graph, &critical_paths, &args),
    }
}

fn render_focused_graph(
    graph: &crate::queue::graph::DependencyGraph,
    task: &crate::contracts::Task,
    task_id: &str,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    args: &QueueGraphArgs,
) -> Result<()> {
    match args.format {
        GraphFormatArg::Tree => tree::render_task_tree(
            graph,
            task_id,
            critical_paths,
            args.reverse,
            args.include_done,
        ),
        GraphFormatArg::Dot => {
            dot::render_task_dot(graph, Some(task_id), args.reverse, args.include_done)
        }
        GraphFormatArg::Json => {
            json::render_task_json(graph, task, critical_paths, args.reverse, args.include_done)
        }
        GraphFormatArg::List => list::render_task_list(
            graph,
            task_id,
            critical_paths,
            args.reverse,
            args.include_done,
        ),
    }
}

fn render_full_graph(
    graph: &crate::queue::graph::DependencyGraph,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    args: &QueueGraphArgs,
) -> Result<()> {
    match args.format {
        GraphFormatArg::Tree => tree::render_full_tree(graph, critical_paths, args.include_done),
        GraphFormatArg::Dot => dot::render_task_dot(graph, None, args.reverse, args.include_done),
        GraphFormatArg::Json => json::render_full_json(graph, critical_paths, args.include_done),
        GraphFormatArg::List => list::render_full_list(graph, critical_paths, args.include_done),
    }
}
