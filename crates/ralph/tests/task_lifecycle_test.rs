//! Purpose: thin integration-test hub for task lifecycle coverage.
//!
//! Responsibilities:
//! - Re-export shared imports and suite-local helpers for lifecycle integration tests.
//! - Delegate happy-path, state-verification, transition, multi-task, and runner execution coverage to focused companion modules.
//!
//! Scope:
//! - Test-suite wiring only; this root module contains no test functions.
//!
//! Usage:
//! - Companion modules use `use super::*;` to access shared types, helpers, and fixtures.
//!
//! Invariants/assumptions callers must respect:
//! - `support.rs` remains the suite-local fixture bootstrap and must be preserved.
//! - Test names, assertions, and CLI-driven behavior stay unchanged from the original monolith.

#[path = "task_lifecycle_test/support.rs"]
mod support;
mod test_support;

use anyhow::Result;
use ralph::contracts::{Task, TaskPriority, TaskStatus};
use support::{LifecycleRepo, draft_task, terminal_task};

/// Helper to find a task by ID in a queue slice.
fn find_task<'a>(tasks: &'a [Task], id: &str) -> Option<&'a Task> {
    tasks.iter().find(|task| task.id == id)
}

#[path = "task_lifecycle_test/happy_path.rs"]
mod happy_path;
#[path = "task_lifecycle_test/multi_task.rs"]
mod multi_task;
#[path = "task_lifecycle_test/runner_execution.rs"]
mod runner_execution;
#[path = "task_lifecycle_test/state_verification.rs"]
mod state_verification;
#[path = "task_lifecycle_test/transitions.rs"]
mod transitions;
