//! Queue continuation document regression coverage.
//!
//! Purpose:
//! - Queue continuation document regression coverage.
//!
//! Responsibilities:
//! - Verify validate/repair/undo machine documents expose stable continuation guidance.
//! - Cover representative ready, stalled, and checkpoint-list states.
//!
//! Non-scope:
//! - CLI routing or JSON printing.
//! - Queue repair internals beyond the document builder contract.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Ready queues should advertise normal continuation commands.
//! - Stalled states should surface operator-recovery blocking metadata.
//! - Undo list documents should reflect whether checkpoints exist.

use super::*;

use crate::cli::machine::args::{MachineQueueRepairArgs, MachineQueueUndoArgs};
use crate::contracts::{BlockingReason, BlockingStatus, Config, QueueFile, Task, TaskStatus};
use crate::queue::save_queue;
use crate::undo::create_undo_snapshot;
use std::collections::HashMap;
use tempfile::TempDir;

fn create_test_resolved(temp_dir: &TempDir) -> crate::config::Resolved {
    let repo_root = temp_dir.path();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).expect("create .ralph dir");

    crate::config::Resolved {
        config: Config::default(),
        repo_root: repo_root.to_path_buf(),
        queue_path: ralph_dir.join("queue.jsonc"),
        done_path: ralph_dir.join("done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(ralph_dir.join("config.jsonc")),
    }
}

fn test_task(id: &str, title: &str) -> Task {
    Task {
        id: id.to_string(),
        status: TaskStatus::Todo,
        title: title.to_string(),
        description: None,
        priority: Default::default(),
        tags: Vec::new(),
        scope: Vec::new(),
        evidence: Vec::new(),
        plan: Vec::new(),
        notes: Vec::new(),
        request: Some("seed request".to_string()),
        agent: None,
        created_at: Some("2026-04-01T00:00:00Z".to_string()),
        updated_at: Some("2026-04-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        estimated_minutes: None,
        actual_minutes: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

#[test]
fn build_validate_document_marks_missing_queue_as_stalled() {
    let temp_dir = TempDir::new().expect("temp dir");
    let resolved = create_test_resolved(&temp_dir);

    let document = build_validate_document(&resolved);

    assert!(!document.valid);
    assert!(document.warnings.is_empty());
    let blocking = document.blocking.expect("blocking state");
    assert_eq!(blocking.status, BlockingStatus::Stalled);
    match blocking.reason {
        BlockingReason::OperatorRecovery {
            scope,
            reason,
            suggested_command,
        } => {
            assert_eq!(scope, "queue_validate");
            assert_eq!(reason, "validation_failed");
            assert_eq!(
                suggested_command.as_deref(),
                Some("ralph machine queue repair --dry-run")
            );
        }
        other => panic!("unexpected blocking reason: {other:?}"),
    }
    assert_eq!(
        document.continuation.headline,
        "Queue continuation is stalled."
    );
    assert_eq!(
        document.continuation.next_steps[0].command,
        "ralph machine queue repair --dry-run"
    );
}

#[test]
fn build_validate_document_ready_queue_offers_resume_and_mutation_preview() {
    let temp_dir = TempDir::new().expect("temp dir");
    let resolved = create_test_resolved(&temp_dir);
    save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", "Ready task")],
        },
    )
    .expect("save queue");

    let document = build_validate_document(&resolved);

    assert!(document.valid);
    assert!(document.blocking.is_none());
    assert!(document.warnings.is_empty());
    assert_eq!(
        document.continuation.headline,
        "Queue continuation is ready."
    );
    assert_eq!(
        document.continuation.next_steps[0].command,
        "ralph machine run one --resume"
    );
    assert_eq!(
        document.continuation.next_steps[1].command,
        "ralph machine task mutate --dry-run --input <PATH>"
    );
}

#[test]
fn build_repair_document_dry_run_reports_recoverable_repairs() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let resolved = create_test_resolved(&temp_dir);
    save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", "Already valid")],
        },
    )?;
    save_queue(&resolved.done_path, &QueueFile::default())?;

    let document =
        build_repair_document(&resolved, false, &MachineQueueRepairArgs { dry_run: true })?;

    assert!(document.dry_run);
    assert!(document.changed);
    let blocking = document.blocking.expect("repair preview blocking state");
    assert_eq!(blocking.status, BlockingStatus::Stalled);
    assert_eq!(document.continuation.headline, "Repair preview is ready.");
    assert_eq!(
        document.continuation.next_steps[0].command,
        "ralph machine queue repair"
    );
    Ok(())
}

#[test]
fn build_undo_document_list_without_snapshots_offers_mutation_checkpoint_guidance()
-> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let resolved = create_test_resolved(&temp_dir);

    let document = build_undo_document(
        &resolved,
        false,
        &MachineQueueUndoArgs {
            id: None,
            list: true,
            dry_run: false,
        },
    )?;

    assert!(document.dry_run);
    assert!(!document.restored);
    assert_eq!(
        document.continuation.headline,
        "No continuation checkpoints are available."
    );
    assert_eq!(
        document.continuation.next_steps[0].command,
        "ralph machine task mutate --dry-run --input <PATH>"
    );
    Ok(())
}

#[test]
fn build_undo_document_list_with_snapshots_offers_preview_and_restore() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let resolved = create_test_resolved(&temp_dir);
    save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", "Checkpointed task")],
        },
    )?;
    save_queue(&resolved.done_path, &QueueFile::default())?;
    create_undo_snapshot(&resolved, "queue repair continuation")?;

    let document = build_undo_document(
        &resolved,
        false,
        &MachineQueueUndoArgs {
            id: None,
            list: true,
            dry_run: false,
        },
    )?;

    assert!(document.dry_run);
    assert!(!document.restored);
    assert_eq!(
        document.continuation.headline,
        "Continuation checkpoints are available."
    );
    assert_eq!(
        document.continuation.next_steps[0].command,
        "ralph machine queue undo --dry-run"
    );
    assert_eq!(
        document.continuation.next_steps[1].command,
        "ralph machine queue undo --id <SNAPSHOT_ID>"
    );
    let result = document.result.expect("snapshot list result");
    assert_eq!(result.as_array().map(Vec::len), Some(1));
    Ok(())
}
