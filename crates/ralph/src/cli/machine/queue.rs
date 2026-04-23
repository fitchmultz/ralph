//! Queue-oriented machine command handlers.
//!
//! Purpose:
//! - Implement `ralph machine queue ...` dispatch and queue snapshot/document routing.
//!
//! Responsibilities:
//! - Render queue snapshots, graph documents, and dashboard documents.
//! - Delegate validation/repair/undo continuation documents to `queue_docs.rs`.
//! - Keep machine queue formatting deterministic and versioned.
//!
//! Scope:
//! - Queue command routing and read-side document assembly.
//! - Does not own queue continuation builders or Clap definitions.
//!
//! Usage:
//! - Called from `cli::machine::handle_machine`.
//! - Imports continuation builders from `queue_docs.rs` via this module.
//!
//! Invariants/assumptions:
//! - Queue reads preserve existing read-only semantics.
//! - Graph output keeps task/dependency ordering deterministic.
//! - Mutating repair/undo flows hold the queue lock and create undo checkpoints before writes.

use anyhow::Result;
use serde_json::{Value as JsonValue, json};

use super::queue_docs::{build_repair_document, build_undo_document, build_validate_document};
use crate::cli::machine::args::{MachineQueueArgs, MachineQueueCommand};
use crate::cli::machine::common::{build_queue_read_document, done_queue_ref};
use crate::cli::machine::io::print_json;
use crate::contracts::{
    MACHINE_DASHBOARD_READ_VERSION, MACHINE_GRAPH_READ_VERSION,
    MACHINE_QUEUE_UNLOCK_INSPECT_VERSION, MachineDashboardReadDocument,
    MachineGraphReadDocument, MachineQueueUnlockCondition, MachineQueueUnlockInspectDocument,
};
use crate::queue::graph::{
    build_graph, find_critical_paths, get_blocked_tasks, get_runnable_tasks,
};

pub(crate) fn handle_queue(args: MachineQueueArgs, force: bool) -> Result<()> {
    let resolved = crate::config::resolve_from_cwd()?;
    match args.command {
        MachineQueueCommand::Read => print_json(&build_queue_read_document(&resolved)?),
        MachineQueueCommand::Graph => {
            let (active, done) = crate::cli::load_and_validate_queues_read_only(&resolved, true)?;
            let done_ref = done
                .as_ref()
                .and_then(|done| done_queue_ref(done, &resolved.done_path));
            let graph = build_graph(&active, done_ref);
            let critical = find_critical_paths(&graph);
            print_json(&MachineGraphReadDocument {
                version: MACHINE_GRAPH_READ_VERSION,
                graph: build_graph_json(&graph, &critical)?,
            })
        }
        MachineQueueCommand::Dashboard(args) => {
            let (active, done) = crate::cli::load_and_validate_queues_read_only(&resolved, true)?;
            let done_ref = done
                .as_ref()
                .and_then(|done| done_queue_ref(done, &resolved.done_path));
            let cache_dir = resolved.repo_root.join(".ralph/cache");
            let productivity = crate::productivity::load_productivity_stats(&cache_dir).ok();
            let dashboard = crate::reports::build_dashboard_report(
                &active,
                done_ref,
                productivity.as_ref(),
                args.days,
            );
            print_json(&MachineDashboardReadDocument {
                version: MACHINE_DASHBOARD_READ_VERSION,
                dashboard: serde_json::to_value(dashboard)?,
            })
        }
        MachineQueueCommand::Validate => print_json(&build_validate_document(&resolved)),
        MachineQueueCommand::Repair(args) => {
            print_json(&build_repair_document(&resolved, force, &args)?)
        }
        MachineQueueCommand::Undo(args) => {
            print_json(&build_undo_document(&resolved, force, &args)?)
        }
        MachineQueueCommand::UnlockInspect => {
            let inspection = crate::commands::run::inspect_queue_lock(&resolved.repo_root);
            let (condition, blocking, unlock_allowed, headline, detail) = match inspection {
                None => (
                    MachineQueueUnlockCondition::Clear,
                    None,
                    false,
                    "Queue lock is clear".to_string(),
                    "No queue lock is present for this repository.".to_string(),
                ),
                Some(inspection) => match inspection.condition {
                    crate::commands::run::QueueLockCondition::Live => (
                        MachineQueueUnlockCondition::Live,
                        Some(inspection.blocking_state),
                        false,
                        "Queue lock is held by an active Ralph process".to_string(),
                        "Wait for the owning Ralph process to finish or inspect the current lock owner before retrying.".to_string(),
                    ),
                    crate::commands::run::QueueLockCondition::Stale => (
                        MachineQueueUnlockCondition::Stale,
                        Some(inspection.blocking_state),
                        true,
                        "Queue lock is stale".to_string(),
                        "The recorded lock owner is no longer running, so it is safe to clear the lock.".to_string(),
                    ),
                    crate::commands::run::QueueLockCondition::OwnerMissing => (
                        MachineQueueUnlockCondition::OwnerMissing,
                        Some(inspection.blocking_state),
                        false,
                        "Queue lock owner metadata is missing".to_string(),
                        "Confirm no other Ralph process is active before clearing the broken lock record.".to_string(),
                    ),
                    crate::commands::run::QueueLockCondition::OwnerUnreadable => (
                        MachineQueueUnlockCondition::OwnerUnreadable,
                        Some(inspection.blocking_state),
                        false,
                        "Queue lock owner metadata is unreadable".to_string(),
                        "Confirm no other Ralph process is active before clearing the broken lock record.".to_string(),
                    ),
                },
            };
            print_json(&MachineQueueUnlockInspectDocument {
                version: MACHINE_QUEUE_UNLOCK_INSPECT_VERSION,
                condition,
                blocking,
                unlock_allowed: unlock_allowed,
                continuation: crate::contracts::MachineContinuationSummary {
                    headline,
                    detail,
                    blocking: None,
                    next_steps: vec![crate::contracts::MachineContinuationAction {
                        title: "Inspect queue lock".to_string(),
                        command: "ralph machine queue unlock-inspect".to_string(),
                        detail: "Refresh the structured queue-lock state for this repository.".to_string(),
                    }],
                },
            })
        }
    }
}

fn build_graph_json(
    graph: &crate::queue::graph::DependencyGraph,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
) -> Result<JsonValue> {
    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);
    let mut tasks = graph
        .task_ids()
        .filter_map(|id| graph.get(id))
        .map(|node| {
            let mut dependencies = node.dependencies.clone();
            dependencies.sort_unstable();
            let mut dependents = node.dependents.clone();
            dependents.sort_unstable();
            json!({
                "id": node.task.id,
                "title": node.task.title,
                "status": node.task.status.as_str(),
                "dependencies": dependencies,
                "dependents": dependents,
                "critical": graph.is_on_critical_path(&node.task.id, critical_paths),
            })
        })
        .collect::<Vec<_>>();
    tasks.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));

    let mut critical_paths_json = critical_paths
        .iter()
        .map(|path| {
            json!({
                "path": path.path,
                "length": path.length,
                "blocked": path.is_blocked,
            })
        })
        .collect::<Vec<_>>();
    critical_paths_json
        .sort_by(|left, right| left["path"].to_string().cmp(&right["path"].to_string()));

    Ok(json!({
        "summary": {
            "total_tasks": graph.len(),
            "runnable_tasks": runnable.len(),
            "blocked_tasks": blocked.len(),
        },
        "critical_paths": critical_paths_json,
        "tasks": tasks,
    }))
}
