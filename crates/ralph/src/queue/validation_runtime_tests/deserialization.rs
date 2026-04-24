//! Queue deserialization validation tests.
//!
//! Purpose:
//! - Queue deserialization validation tests.
//!
//! Responsibilities:
//! - Verify malformed JSON field types fail with useful diagnostics.
//! - Verify missing required task fields fail during deserialization.
//! - Keep serde-focused checks separate from runtime validation behavior.
//!
//! Not handled here:
//! - Queue graph or relationship validation.
//! - Queue load/repair read-path semantics.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - These tests assert raw serde errors, not post-deserialization validation.
//! - JSON fixtures intentionally violate schema expectations.

use serde::Deserialize;

use crate::contracts::QueueFile;

#[test]
fn deserialize_rejects_invalid_field_types() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct TestTask {
        id: String,
        status: String,
        title: String,
        tags: Vec<String>,
        scope: Vec<String>,
        evidence: Vec<String>,
        plan: Vec<String>,
        created_at: String,
        updated_at: String,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct TestQueueFile {
        version: u32,
        tasks: Vec<TestTask>,
    }

    let json = r#"{
        "version": 1,
        "tasks": [{
            "id": "RQ-0001",
            "status": "todo",
            "title": "Test",
            "tags": "not-an-array",
            "scope": ["file"],
            "evidence": ["observed"],
            "plan": ["do thing"],
            "created_at": "2026-01-18T00:00:00Z",
            "updated_at": "2026-01-18T00:00:00Z"
        }]
    }"#;

    let err = serde_json::from_str::<TestQueueFile>(json)
        .expect_err("Should fail when tags is not an array");
    assert!(
        err.to_string().contains("tags") || err.to_string().contains("array"),
        "Error should mention tags field: {err}"
    );
}

#[test]
fn deserialize_rejects_missing_task_id() {
    let json = r#"{
        "version": 1,
        "tasks": [{
            "status": "todo",
            "title": "Test",
            "tags": [],
            "scope": [],
            "evidence": [],
            "plan": [],
            "created_at": "2026-01-18T00:00:00Z",
            "updated_at": "2026-01-18T00:00:00Z"
        }]
    }"#;

    let err = serde_json::from_str::<QueueFile>(json).expect_err("Should fail without id");
    assert!(
        err.to_string().contains("missing field") && err.to_string().contains("id"),
        "Error should mention missing id field: {err}"
    );
}

#[test]
fn deserialize_rejects_missing_task_title() {
    let json = r#"{
        "version": 1,
        "tasks": [{
            "id": "RQ-0001",
            "status": "todo",
            "tags": [],
            "scope": [],
            "evidence": [],
            "plan": [],
            "created_at": "2026-01-18T00:00:00Z",
            "updated_at": "2026-01-18T00:00:00Z"
        }]
    }"#;

    let err = serde_json::from_str::<QueueFile>(json).expect_err("Should fail without title");
    assert!(
        err.to_string().contains("missing field") && err.to_string().contains("title"),
        "Error should mention missing title field: {err}"
    );
}
