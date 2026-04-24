//! Merge-focused queue import tests.
//!
//! Purpose:
//! - Merge-focused queue import tests.
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

use super::super::merge::merge_imported_tasks;
use super::{ImportReport, OnDuplicate};
use crate::contracts::{QueueFile, Task, TaskStatus};

#[test]
fn import_report_summary_format() {
    let report = ImportReport {
        parsed: 5,
        imported: 3,
        skipped_duplicates: 1,
        renamed: 1,
        rename_mappings: vec![("OLD-001".to_string(), "RQ-0001".to_string())],
    };
    let summary = report.summary();
    assert!(summary.contains("parsed 5"));
    assert!(summary.contains("imported 3"));
    assert!(summary.contains("skipped 1"));
    assert!(summary.contains("renamed 1"));
    assert!(summary.contains("OLD-001 -> RQ-0001"));
}

#[test]
fn merge_imported_tasks_rename_records_mapping() {
    let mut queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            title: "Existing".to_string(),
            description: None,
            status: TaskStatus::Todo,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            ..Default::default()
        }],
    };

    let imported = vec![Task {
        id: "RQ-0001".to_string(),
        title: "Duplicate".to_string(),
        description: None,
        status: TaskStatus::Todo,
        created_at: Some("2026-01-02T00:00:00Z".to_string()),
        updated_at: Some("2026-01-02T00:00:00Z".to_string()),
        ..Default::default()
    }];

    let report = merge_imported_tasks(
        &mut queue,
        None,
        imported,
        "RQ",
        4,
        10,
        "2026-01-03T00:00:00Z",
        OnDuplicate::Rename,
    )
    .unwrap();

    assert_eq!(report.renamed, 1);
    assert_eq!(report.rename_mappings.len(), 1);
    assert_eq!(report.rename_mappings[0].0, "RQ-0001");
    assert!(report.rename_mappings[0].1.starts_with("RQ-"));
    assert_eq!(queue.tasks.len(), 2);
    assert!(queue.tasks.iter().any(|task| task.id == "RQ-0001"));
    let duplicate = queue
        .tasks
        .iter()
        .find(|task| task.title == "Duplicate")
        .expect("imported duplicate task");
    assert_ne!(duplicate.id, "RQ-0001");
}
