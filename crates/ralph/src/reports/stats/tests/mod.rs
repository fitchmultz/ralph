//! Stats report unit tests grouped by concern.
//!
//! Purpose:
//! - Stats report unit tests grouped by concern.
//!
//! Responsibilities:
//! - Provide focused regression coverage for stats summary and breakdown helpers.
//! - Keep production stats modules free of large inline test blocks.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::collections::HashMap;

use time::Duration;

use crate::constants::custom_fields::RUNNER_USED;
use crate::contracts::{QueueFile, Task, TaskStatus};

fn task_with_status(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        description: None,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
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
    }
}

mod breakdown_tests;
mod summary_tests;
