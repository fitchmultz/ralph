//! `ralph queue graph` subcommand for dependency visualization.
//!
//! Responsibilities:
//! - Display task dependency graphs in various formats (tree, DOT, JSON, list)
//! - Highlight critical paths and blocking relationships
//! - Support filtering by task and status
//!
//! Not handled here:
//! - Graph algorithm computation (handled by queue::graph)
//! - File I/O (handled by queue module)
//! - TUI rendering
//!
//! Invariants/assumptions:
//! - Queue files are valid and already validated by the loader
//! - Task IDs are matched after trimming whitespace
//! - Output is written to stdout

use anyhow::{anyhow, Result};
use clap::{Args, ValueEnum};

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::contracts::TaskStatus;
use crate::queue::find_task_across;
use crate::queue::graph::{
    build_graph, find_critical_path_from, find_critical_paths, get_blocked_tasks,
    get_runnable_tasks, GraphFormat,
};

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
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Build the dependency graph
    let graph = build_graph(&queue_file, done_ref);

    if graph.is_empty() {
        println!("No tasks found in queue.");
        return Ok(());
    }

    // Find critical paths for highlighting
    let critical_paths = if args.critical || matches!(args.format, GraphFormatArg::Tree) {
        find_critical_paths(&graph)
    } else {
        Vec::new()
    };

    // If a specific task is specified, focus on it
    if let Some(task_id) = args.task {
        let task_id = task_id.trim();
        let task = find_task_across(&queue_file, done_ref, task_id)
            .ok_or_else(|| anyhow!("task not found: {}", task_id))?;

        match args.format {
            GraphFormatArg::Tree => {
                render_task_tree(
                    &graph,
                    task_id,
                    &critical_paths,
                    args.reverse,
                    args.include_done,
                )?;
            }
            GraphFormatArg::Dot => {
                render_task_dot(&graph, Some(task_id), args.reverse, args.include_done)?;
            }
            GraphFormatArg::Json => {
                render_task_json(
                    &graph,
                    task,
                    &critical_paths,
                    args.reverse,
                    args.include_done,
                )?;
            }
            GraphFormatArg::List => {
                render_task_list(
                    &graph,
                    task_id,
                    &critical_paths,
                    args.reverse,
                    args.include_done,
                )?;
            }
        }
    } else {
        // Show full graph
        match args.format {
            GraphFormatArg::Tree => {
                render_full_tree(&graph, &critical_paths, args.include_done)?;
            }
            GraphFormatArg::Dot => {
                render_task_dot(&graph, None, args.reverse, args.include_done)?;
            }
            GraphFormatArg::Json => {
                render_full_json(&graph, &critical_paths, args.include_done)?;
            }
            GraphFormatArg::List => {
                render_full_list(&graph, &critical_paths, args.include_done)?;
            }
        }
    }

    Ok(())
}

/// Render a tree view for a specific task.
fn render_task_tree(
    graph: &crate::queue::graph::DependencyGraph,
    task_id: &str,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let task = graph
        .get(task_id)
        .ok_or_else(|| anyhow!("task not found: {}", task_id))?;

    println!("Dependency tree for {}: {}", task_id, task.task.title);

    if reverse {
        println!("\nTasks blocked by this task (downstream):");
        render_dependents_tree(graph, task_id, "", include_done, critical_paths)?;
    } else {
        println!("\nTasks this task depends on (upstream):");
        render_dependencies_tree(graph, task_id, "", include_done, critical_paths, true)?;
    }

    // Show critical path info
    if let Some(cp) = find_critical_path_from(graph, task_id) {
        if cp.length > 1 {
            println!("\nCritical path from this task: {} tasks", cp.length);
            if cp.is_blocked {
                println!("  Status: BLOCKED (incomplete dependencies)");
            } else {
                println!("  Status: Unblocked");
            }
        }
    }

    Ok(())
}

/// Render dependencies recursively (upstream).
fn render_dependencies_tree(
    graph: &crate::queue::graph::DependencyGraph,
    task_id: &str,
    prefix: &str,
    include_done: bool,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    is_last: bool,
) -> Result<()> {
    let node = match graph.get(task_id) {
        Some(n) => n,
        None => return Ok(()),
    };

    let is_done = matches!(node.task.status, TaskStatus::Done | TaskStatus::Rejected);
    if is_done && !include_done {
        return Ok(());
    }

    // Determine prefix characters
    let branch = if prefix.is_empty() {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };

    let is_critical = graph.is_on_critical_path(task_id, critical_paths);
    let critical_marker = if is_critical { "* " } else { "  " };
    let status_marker = status_to_emoji(node.task.status);

    println!(
        "{}{}{}{}: {} [{}]",
        prefix, branch, critical_marker, task_id, node.task.title, status_marker
    );

    // Recurse into dependencies
    let deps: Vec<_> = node
        .dependencies
        .iter()
        .filter(|id| include_done || !graph.is_task_completed(id))
        .collect();

    let new_prefix = if prefix.is_empty() {
        (if is_last { "   " } else { "│  " }).to_string()
    } else {
        format!("{}{}", prefix, if is_last { "   " } else { "│  " })
    };

    for (i, dep_id) in deps.iter().enumerate() {
        let last = i == deps.len() - 1;
        render_dependencies_tree(
            graph,
            dep_id,
            &new_prefix,
            include_done,
            critical_paths,
            last,
        )?;
    }

    Ok(())
}

/// Render dependents recursively (downstream).
fn render_dependents_tree(
    graph: &crate::queue::graph::DependencyGraph,
    task_id: &str,
    prefix: &str,
    include_done: bool,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
) -> Result<()> {
    let node = match graph.get(task_id) {
        Some(n) => n,
        None => return Ok(()),
    };

    let is_done = matches!(node.task.status, TaskStatus::Done | TaskStatus::Rejected);
    if is_done && !include_done && !prefix.is_empty() {
        return Ok(());
    }

    let _is_critical = graph.is_on_critical_path(task_id, critical_paths);
    let status_marker = status_to_emoji(node.task.status);

    if prefix.is_empty() {
        println!("* {}: {} [{}]", task_id, node.task.title, status_marker);
    }

    // Recurse into dependents
    let deps: Vec<_> = node
        .dependents
        .iter()
        .filter(|id| include_done || !graph.is_task_completed(id))
        .collect();

    for (i, dep_id) in deps.iter().enumerate() {
        let dep_node = match graph.get(dep_id) {
            Some(n) => n,
            None => continue,
        };

        let is_last = i == deps.len() - 1;
        let branch = if is_last { "└─ " } else { "├─ " };
        let dep_is_done = matches!(
            dep_node.task.status,
            TaskStatus::Done | TaskStatus::Rejected
        );

        if dep_is_done && !include_done {
            continue;
        }

        let dep_is_critical = graph.is_on_critical_path(dep_id, critical_paths);
        let dep_critical_marker = if dep_is_critical { "* " } else { "  " };
        let dep_status_marker = status_to_emoji(dep_node.task.status);

        println!(
            "{}{}{}{}: {} [{}]",
            prefix, branch, dep_critical_marker, dep_id, dep_node.task.title, dep_status_marker
        );

        let new_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
        render_dependents_tree(graph, dep_id, &new_prefix, include_done, critical_paths)?;
    }

    Ok(())
}

/// Render full dependency tree (all roots).
fn render_full_tree(
    graph: &crate::queue::graph::DependencyGraph,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    include_done: bool,
) -> Result<()> {
    println!("Task Dependency Graph\n");

    // Show statistics
    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);

    println!("Summary:");
    println!("  Total tasks: {}", graph.len());
    println!("  Ready to run: {}", runnable.len());
    println!("  Blocked: {}", blocked.len());

    if !critical_paths.is_empty() {
        println!("  Critical path length: {}", critical_paths[0].length);
    }

    println!("\nDependency Chains:\n");

    // Show trees from each root
    for (i, root_id) in graph.roots().iter().enumerate() {
        if let Some(node) = graph.get(root_id) {
            let is_done = matches!(node.task.status, TaskStatus::Done | TaskStatus::Rejected);
            if is_done && !include_done {
                continue;
            }

            render_dependencies_tree(graph, root_id, "", include_done, critical_paths, true)?;

            if i < graph.roots().len() - 1 {
                println!();
            }
        }
    }

    // Show legend
    println!("\nLegend:");
    println!("  * = on critical path");
    println!("  ⏳ = todo, 🔄 = doing, ✅ = done, ❌ = rejected");

    Ok(())
}

/// Render Graphviz DOT format.
fn render_task_dot(
    graph: &crate::queue::graph::DependencyGraph,
    focus_task: Option<&str>,
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let critical_paths = find_critical_paths(graph);

    println!("digraph dependencies {{");
    println!("  rankdir=TB;");
    println!("  node [shape=box, style=rounded];");
    println!();

    // Collect tasks to include
    let mut included_tasks: Vec<String> = Vec::new();

    if let Some(task_id) = focus_task {
        // Include only the focus task and its related tasks
        included_tasks.push(task_id.to_string());

        if reverse {
            // Include dependents
            let chain = graph.get_blocked_chain(task_id);
            included_tasks.extend(chain);
        } else {
            // Include dependencies
            let chain = graph.get_blocking_chain(task_id);
            included_tasks.extend(chain);
        }
    } else {
        // Include all tasks
        included_tasks.extend(graph.task_ids().cloned());
    }

    // Filter by status if needed
    if !include_done {
        included_tasks.retain(|id| {
            graph
                .get(id)
                .map(|n| !matches!(n.task.status, TaskStatus::Done | TaskStatus::Rejected))
                .unwrap_or(false)
        });
    }

    // Define nodes
    for task_id in &included_tasks {
        if let Some(node) = graph.get(task_id) {
            let is_critical = graph.is_on_critical_path(task_id, &critical_paths);
            let color = match node.task.status {
                TaskStatus::Done => "green",
                TaskStatus::Rejected => "gray",
                TaskStatus::Doing => "orange",
                TaskStatus::Draft => "yellow",
                TaskStatus::Todo => {
                    if is_critical {
                        "red"
                    } else {
                        "lightblue"
                    }
                }
            };

            let style = if is_critical {
                ", style=filled, fillcolor=red, fontcolor=white"
            } else {
                ""
            };
            let label = format!("{}\\n{}", task_id, escape_label(&node.task.title));

            println!(
                "  \"{}\" [label=\"{}\", color={}{}];",
                task_id, label, color, style
            );
        }
    }

    println!();

    // Define edges
    for task_id in &included_tasks {
        if let Some(node) = graph.get(task_id) {
            for dep_id in &node.dependencies {
                if included_tasks.contains(dep_id) {
                    let is_critical = graph.is_on_critical_path(task_id, &critical_paths)
                        && graph.is_on_critical_path(dep_id, &critical_paths);
                    let edge_style = if is_critical {
                        " [color=red, penwidth=2]"
                    } else {
                        ""
                    };
                    println!("  \"{}\" -> \"{}\"{};", task_id, dep_id, edge_style);
                }
            }
        }
    }

    println!("}}");

    Ok(())
}

/// Render JSON output for a specific task.
fn render_task_json(
    graph: &crate::queue::graph::DependencyGraph,
    task: &crate::contracts::Task,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    use serde_json::json;

    let task_id = &task.id;

    let (related_tasks, relationship_type) = if reverse {
        (graph.get_blocked_chain(task_id), "blocked_by")
    } else {
        (graph.get_blocking_chain(task_id), "depends_on")
    };

    let is_critical = graph.is_on_critical_path(task_id, critical_paths);

    let related: Vec<_> = related_tasks
        .iter()
        .filter_map(|id| graph.get(id))
        .filter(|n| {
            include_done || !matches!(n.task.status, TaskStatus::Done | TaskStatus::Rejected)
        })
        .map(|n| {
            json!({
                "id": n.task.id,
                "title": n.task.title,
                "status": format!("{:?}", n.task.status).to_lowercase(),
                "critical": graph.is_on_critical_path(&n.task.id, critical_paths),
            })
        })
        .collect();

    let output = json!({
        "task": task_id,
        "title": task.title,
        "status": format!("{:?}", task.status).to_lowercase(),
        "critical": is_critical,
        "relationship": relationship_type,
        "related_tasks": related,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Render JSON output for full graph.
fn render_full_json(
    graph: &crate::queue::graph::DependencyGraph,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    include_done: bool,
) -> Result<()> {
    use serde_json::json;

    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);

    let tasks: Vec<_> = graph
        .task_ids()
        .filter_map(|id| graph.get(id))
        .filter(|n| {
            include_done || !matches!(n.task.status, TaskStatus::Done | TaskStatus::Rejected)
        })
        .map(|n| {
            json!({
                "id": n.task.id,
                "title": n.task.title,
                "status": format!("{:?}", n.task.status).to_lowercase(),
                "dependencies": n.dependencies,
                "dependents": n.dependents,
                "critical": graph.is_on_critical_path(&n.task.id, critical_paths),
            })
        })
        .collect();

    let critical_path_json: Vec<_> = critical_paths
        .iter()
        .map(|cp| {
            json!({
                "path": cp.path,
                "length": cp.length,
                "blocked": cp.is_blocked,
            })
        })
        .collect();

    let output = json!({
        "summary": {
            "total_tasks": graph.len(),
            "runnable_tasks": runnable.len(),
            "blocked_tasks": blocked.len(),
        },
        "critical_paths": critical_path_json,
        "tasks": tasks,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Render flat list view for a specific task.
fn render_task_list(
    graph: &crate::queue::graph::DependencyGraph,
    task_id: &str,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let task = graph
        .get(task_id)
        .ok_or_else(|| anyhow!("task not found: {}", task_id))?;

    println!("{}: {}", task_id, task.task.title);
    println!("Status: {:?}", task.task.status);

    if graph.is_on_critical_path(task_id, critical_paths) {
        println!("CRITICAL PATH TASK");
    }

    if reverse {
        println!("\nBlocked tasks (downstream):");
        let chain = graph.get_blocked_chain(task_id);
        for (i, id) in chain.iter().enumerate() {
            if let Some(node) = graph.get(id) {
                let is_done = matches!(node.task.status, TaskStatus::Done | TaskStatus::Rejected);
                if is_done && !include_done {
                    continue;
                }
                let indent = "  ".repeat(i + 1);
                let critical = if graph.is_on_critical_path(id, critical_paths) {
                    " *"
                } else {
                    ""
                };
                println!(
                    "{}{}: {} [{:?}]{}",
                    indent, id, node.task.title, node.task.status, critical
                );
            }
        }
    } else {
        println!("\nDependencies (upstream):");
        let chain = graph.get_blocking_chain(task_id);
        for (i, id) in chain.iter().enumerate() {
            if let Some(node) = graph.get(id) {
                let is_done = matches!(node.task.status, TaskStatus::Done | TaskStatus::Rejected);
                if is_done && !include_done {
                    continue;
                }
                let indent = "  ".repeat(i + 1);
                let critical = if graph.is_on_critical_path(id, critical_paths) {
                    " *"
                } else {
                    ""
                };
                println!(
                    "{}{}: {} [{:?}]{}",
                    indent, id, node.task.title, node.task.status, critical
                );
            }
        }
    }

    Ok(())
}

/// Render flat list view for full graph.
fn render_full_list(
    graph: &crate::queue::graph::DependencyGraph,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
    include_done: bool,
) -> Result<()> {
    println!("Task Dependency List\n");

    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);

    println!(
        "Summary: {} total, {} ready, {} blocked\n",
        graph.len(),
        runnable.len(),
        blocked.len()
    );

    // Group by status
    let mut by_status: std::collections::HashMap<TaskStatus, Vec<&str>> =
        std::collections::HashMap::new();

    for task_id in graph.task_ids() {
        if let Some(node) = graph.get(task_id) {
            let is_done = matches!(node.task.status, TaskStatus::Done | TaskStatus::Rejected);
            if is_done && !include_done {
                continue;
            }
            by_status.entry(node.task.status).or_default().push(task_id);
        }
    }

    // Print by status
    for status in [
        TaskStatus::Doing,
        TaskStatus::Todo,
        TaskStatus::Draft,
        TaskStatus::Done,
        TaskStatus::Rejected,
    ] {
        if let Some(tasks) = by_status.get(&status) {
            println!("{:?}:", status);
            for task_id in tasks {
                if let Some(node) = graph.get(task_id) {
                    let critical = if graph.is_on_critical_path(task_id, critical_paths) {
                        " *"
                    } else {
                        ""
                    };
                    let deps = if node.dependencies.is_empty() {
                        "none".to_string()
                    } else {
                        node.dependencies.join(", ")
                    };
                    println!(
                        "  {}: {} (depends on: {}){}",
                        task_id, node.task.title, deps, critical
                    );
                }
            }
            println!();
        }
    }

    Ok(())
}

/// Convert status to emoji.
fn status_to_emoji(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "⏳",
        TaskStatus::Doing => "🔄",
        TaskStatus::Done => "✅",
        TaskStatus::Rejected => "❌",
        TaskStatus::Draft => "📝",
    }
}

/// Escape a string for use in DOT label.
fn escape_label(s: &str) -> String {
    s.replace('"', "\\\"").replace('\n', "\\n")
}
