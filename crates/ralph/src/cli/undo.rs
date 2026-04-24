//! CLI handler for `ralph undo` command.
//!
//! Purpose:
//! - CLI handler for `ralph undo` command.
//!
//! Responsibilities:
//! - List, preview, or restore continuation checkpoints.
//! - Keep undo visible as a normal queue continuation workflow.
//! - Align wording with the canonical blocked/waiting/stalled vocabulary.
//!
//! Not handled here:
//! - Core undo logic (see `crate::undo`).
//! - Queue lock management details beyond invoking the shared machine builders.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::cli::machine::MachineQueueUndoArgs;
use crate::config;
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct UndoArgs {
    /// Snapshot ID to restore (defaults to most recent).
    #[arg(long, short)]
    pub id: Option<String>,

    /// List available snapshots instead of restoring.
    #[arg(long)]
    pub list: bool,

    /// Preview restore without modifying files.
    #[arg(long)]
    pub dry_run: bool,

    /// Show verbose output.
    #[arg(long, short)]
    pub verbose: bool,
}

/// Handle the `ralph undo` command.
pub fn handle(args: UndoArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let document = crate::cli::machine::build_queue_undo_document(
        &resolved,
        force,
        &MachineQueueUndoArgs {
            id: args.id.clone(),
            list: args.list,
            dry_run: args.dry_run,
        },
    )?;

    println!("{}", document.continuation.headline);
    println!("{}", document.continuation.detail);

    if let Some(blocking) = document
        .blocking
        .as_ref()
        .or(document.continuation.blocking.as_ref())
    {
        println!();
        println!(
            "Operator state: {}",
            format!("{:?}", blocking.status).to_lowercase()
        );
        println!("{}", blocking.message);
        if !blocking.detail.is_empty() {
            println!("{}", blocking.detail);
        }
    }

    if let Some(result) = &document.result {
        println!();
        if args.list {
            let snapshots =
                serde_json::from_value::<Vec<crate::undo::UndoSnapshotMeta>>(result.clone())?;
            if snapshots.is_empty() {
                println!(
                    "Ralph will create new checkpoints automatically before future queue writes."
                );
            } else {
                println!("Available continuation checkpoints (newest first):");
                println!();
                for (index, snapshot) in snapshots.iter().enumerate() {
                    println!(
                        "  {}. {} [{}]",
                        index + 1,
                        snapshot.operation,
                        snapshot.timestamp
                    );
                    println!("     ID: {}", snapshot.id);
                }
            }
        } else {
            let restore = serde_json::from_value::<crate::undo::RestoreResult>(result.clone())?;
            println!("Checkpoint: {}", restore.snapshot_id);
            println!("Operation: {}", restore.operation);
            println!("Timestamp: {}", restore.timestamp);
            println!("Tasks affected: {}", restore.tasks_affected);
            if args.verbose && !args.dry_run {
                println!();
                println!("Run `ralph queue list` to inspect the restored queue state in detail.");
            }
        }
    }

    if !document.continuation.next_steps.is_empty() {
        println!();
        println!("Next:");
        for (index, step) in document.continuation.next_steps.iter().enumerate() {
            println!("  {}. {} — {}", index + 1, step.command, step.detail);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_resolved(temp_dir: &TempDir) -> config::Resolved {
        let repo_root = temp_dir.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let queue = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-0001".to_string(),
                title: "Test task".to_string(),
                status: TaskStatus::Todo,
                description: None,
                priority: Default::default(),
                tags: vec!["test".to_string()],
                scope: vec!["crates/ralph".to_string()],
                evidence: vec!["observed".to_string()],
                plan: vec!["do thing".to_string()],
                notes: vec![],
                request: Some("test request".to_string()),
                agent: None,
                created_at: Some("2026-01-18T00:00:00Z".to_string()),
                updated_at: Some("2026-01-18T00:00:00Z".to_string()),
                completed_at: None,
                started_at: None,
                scheduled_start: None,
                estimated_minutes: None,
                actual_minutes: None,
                depends_on: vec![],
                blocks: vec![],
                relates_to: vec![],
                duplicates: None,
                custom_fields: HashMap::new(),
                parent_id: None,
            }],
        };

        crate::queue::save_queue(&queue_path, &queue).unwrap();

        config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn build_undo_list_document_shows_snapshots() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        crate::undo::create_undo_snapshot(&resolved, "test operation").unwrap();

        let document = crate::cli::machine::build_queue_undo_document(
            &resolved,
            false,
            &MachineQueueUndoArgs {
                id: None,
                list: true,
                dry_run: false,
            },
        )
        .expect("undo list document");
        assert!(document.result.is_some());
    }

    #[test]
    fn build_undo_list_document_handles_empty_snapshots() {
        let temp = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp);

        let document = crate::cli::machine::build_queue_undo_document(
            &resolved,
            false,
            &MachineQueueUndoArgs {
                id: None,
                list: true,
                dry_run: false,
            },
        )
        .expect("undo list document");
        assert!(document.result.is_some());
    }
}
