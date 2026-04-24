//! Tests for `followups.rs` queue-growth proposal operations.
//!
//! Purpose:
//! - Tests for `followups.rs` queue-growth proposal operations.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::path::Path;

use super::*;
use crate::config;
use crate::contracts::Config;

fn proposal_json(source_task_id: &str) -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "source_task_id": source_task_id,
        "tasks": [
            {
                "key": "docs-hierarchy",
                "title": "Rework docs hierarchy",
                "description": "Split oversized documentation into a navigable hierarchy.",
                "priority": "high",
                "tags": ["docs"],
                "scope": ["docs/"],
                "evidence": ["Audit found oversized docs."],
                "plan": ["Design hierarchy.", "Move content."],
                "depends_on_keys": [],
                "independence_rationale": "Independent remediation discovered by the audit."
            },
            {
                "key": "cli-flags",
                "title": "Document CLI flags",
                "description": "Add deeper coverage for CLI flags discovered during audit.",
                "priority": "medium",
                "tags": ["docs", "cli"],
                "scope": ["docs/cli.md"],
                "evidence": ["Audit found shallow CLI coverage."],
                "plan": ["Inventory flags.", "Expand docs."],
                "depends_on_keys": ["docs-hierarchy"],
                "independence_rationale": "Depends on the docs hierarchy but is separate from the audit."
            }
        ]
    })
}

fn parse_proposal(value: serde_json::Value) -> FollowupProposalDocument {
    serde_json::from_value(value).expect("valid proposal")
}

fn resolved_for(root: &Path) -> config::Resolved {
    config::Resolved {
        config: Config::default(),
        repo_root: root.to_path_buf(),
        queue_path: root.join(".ralph/queue.jsonc"),
        done_path: root.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(root.join(".ralph/config.jsonc")),
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    crate::fsutil::write_atomic(path, serde_json::to_string_pretty(value)?.as_bytes())
}

fn queue_snapshot(queue: &QueueFile) -> serde_json::Value {
    serde_json::to_value(queue).expect("queue snapshot")
}

#[test]
fn apply_followups_materializes_tasks_after_active_doing_task() -> anyhow::Result<()> {
    let mut source = task_with("RQ-0001", TaskStatus::Doing, vec!["docs".to_string()]);
    source.title = "Audit docs".to_string();
    source.request = Some("Find documentation gaps".to_string());
    let mut existing = task("RQ-0002");
    existing.title = "Existing queued work".to_string();
    let mut active = QueueFile {
        version: 1,
        tasks: vec![source, existing],
    };
    let proposal = parse_proposal(proposal_json("RQ-0001"));

    let report = apply_followups_in_memory(
        &mut active,
        None,
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )?;

    assert_eq!(report.created_tasks.len(), 2);
    assert_eq!(active.tasks[0].id, "RQ-0001");
    assert_eq!(active.tasks[1].id, "RQ-0003");
    assert_eq!(active.tasks[2].id, "RQ-0004");
    assert_eq!(active.tasks[3].id, "RQ-0002");

    let first = &active.tasks[1];
    assert_eq!(first.title, "Rework docs hierarchy");
    assert_eq!(first.priority, TaskPriority::High);
    assert_eq!(first.status, TaskStatus::Todo);
    assert_eq!(first.request.as_deref(), Some("Find documentation gaps"));
    assert_eq!(first.relates_to, vec!["RQ-0001".to_string()]);
    assert_eq!(first.created_at.as_deref(), Some("2026-04-23T20:00:00Z"));

    let second = &active.tasks[2];
    assert_eq!(second.depends_on, vec!["RQ-0003".to_string()]);
    assert_eq!(report.created_tasks[1].depends_on, vec!["RQ-0003"]);

    Ok(())
}

#[test]
fn apply_followups_can_reference_source_task_in_done_archive() -> anyhow::Result<()> {
    let active = &mut QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["docs".to_string()]);
    done_task.completed_at = Some("2026-04-23T19:00:00Z".to_string());
    done_task.request = Some("Original audit request".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    let proposal = parse_proposal(proposal_json("RQ-0001"));

    apply_followups_in_memory(
        active,
        Some(&done),
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )?;

    assert_eq!(active.tasks[0].id, "RQ-0003");
    assert_eq!(active.tasks[0].relates_to, vec!["RQ-0001".to_string()]);
    assert_eq!(
        active.tasks[0].request.as_deref(),
        Some("Original audit request")
    );

    Ok(())
}

#[test]
fn apply_followups_rejects_unknown_dependency_key_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = queue_snapshot(&active);
    let mut value = proposal_json("RQ-0001");
    value["tasks"][0]["depends_on_keys"] = serde_json::json!(["missing"]);
    let proposal = parse_proposal(value);

    let err = apply_followups_in_memory(
        &mut active,
        None,
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("unknown follow-up dependency key: missing"));
    assert_eq!(queue_snapshot(&active), before);
}

#[test]
fn apply_followups_rejects_duplicate_keys_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = queue_snapshot(&active);
    let mut value = proposal_json("RQ-0001");
    value["tasks"][1]["key"] = serde_json::json!("docs-hierarchy");
    let proposal = parse_proposal(value);

    let err = apply_followups_in_memory(
        &mut active,
        None,
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("duplicate follow-up proposal key"));
    assert_eq!(queue_snapshot(&active), before);
}

#[test]
fn apply_followups_rejects_empty_independence_rationale_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = queue_snapshot(&active);
    let mut value = proposal_json("RQ-0001");
    value["tasks"][0]["independence_rationale"] = serde_json::json!(" ");
    let proposal = parse_proposal(value);

    let err = apply_followups_in_memory(
        &mut active,
        None,
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("follow-up independence_rationale must be non-empty"));
    assert_eq!(queue_snapshot(&active), before);
}

#[test]
fn apply_followups_rejects_queue_validation_failure_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = queue_snapshot(&active);
    let mut value = proposal_json("RQ-0001");
    value["tasks"][0]["depends_on_keys"] = serde_json::json!(["cli-flags"]);
    value["tasks"][1]["depends_on_keys"] = serde_json::json!(["docs-hierarchy"]);
    let proposal = parse_proposal(value);

    let err = apply_followups_in_memory(
        &mut active,
        None,
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("validate queue after applying follow-up proposal"));
    assert_eq!(queue_snapshot(&active), before);
}

#[test]
fn apply_followups_rejects_wrong_source_task_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = queue_snapshot(&active);
    let proposal = parse_proposal(proposal_json("RQ-9999"));

    let err = apply_followups_in_memory(
        &mut active,
        None,
        &proposal,
        "RQ-0001",
        Path::new(".ralph/cache/followups/RQ-0001.json"),
        "2026-04-23T20:00:00Z",
        "RQ",
        4,
        10,
        false,
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("does not match --task RQ-0001"));
    assert_eq!(queue_snapshot(&active), before);
}

#[test]
fn apply_followups_file_creates_undo_and_removes_proposal() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let resolved = resolved_for(temp.path());
    crate::queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        },
    )?;
    crate::queue::save_queue(&resolved.done_path, &QueueFile::default())?;
    let proposal_path = default_followups_path(&resolved.repo_root, "RQ-0001");
    write_json(&proposal_path, &proposal_json("RQ-0001"))?;

    let report = apply_followups_file(
        &resolved,
        &FollowupApplyOptions {
            task_id: "RQ-0001",
            input_path: None,
            dry_run: false,
            create_undo: true,
            remove_proposal: true,
        },
    )?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(report.created_tasks.len(), 2);
    assert_eq!(queue.tasks.len(), 3);
    assert!(!proposal_path.exists());
    assert!(resolved.repo_root.join(".ralph/cache/undo").exists());

    Ok(())
}

#[test]
fn apply_followups_file_dry_run_leaves_queue_and_proposal_unchanged() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let resolved = resolved_for(temp.path());
    let initial = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial)?;
    crate::queue::save_queue(&resolved.done_path, &QueueFile::default())?;
    let proposal_path = default_followups_path(&resolved.repo_root, "RQ-0001");
    write_json(&proposal_path, &proposal_json("RQ-0001"))?;

    let report = apply_followups_file(
        &resolved,
        &FollowupApplyOptions {
            task_id: "RQ-0001",
            input_path: None,
            dry_run: true,
            create_undo: true,
            remove_proposal: true,
        },
    )?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert!(report.dry_run);
    assert_eq!(queue_snapshot(&queue), queue_snapshot(&initial));
    assert!(proposal_path.exists());
    assert!(!resolved.repo_root.join(".ralph/cache/undo").exists());

    Ok(())
}

#[test]
fn apply_followups_file_rejects_invalid_priority_without_writing() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let resolved = resolved_for(temp.path());
    let initial = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    crate::queue::save_queue(&resolved.queue_path, &initial)?;
    crate::queue::save_queue(&resolved.done_path, &QueueFile::default())?;
    let proposal_path = default_followups_path(&resolved.repo_root, "RQ-0001");
    let mut value = proposal_json("RQ-0001");
    value["tasks"][0]["priority"] = serde_json::json!("urgent");
    write_json(&proposal_path, &value)?;

    let err = apply_followups_file(
        &resolved,
        &FollowupApplyOptions {
            task_id: "RQ-0001",
            input_path: None,
            dry_run: false,
            create_undo: true,
            remove_proposal: true,
        },
    )
    .unwrap_err();

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert!(format!("{err:#}").contains("unknown variant `urgent`"));
    assert_eq!(queue_snapshot(&queue), queue_snapshot(&initial));
    assert!(proposal_path.exists());
    assert!(!resolved.repo_root.join(".ralph/cache/undo").exists());

    Ok(())
}
