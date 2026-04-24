//! Worker lifecycle tests for parallel execution helpers.
//!
//! Purpose:
//! - Worker lifecycle tests for parallel execution helpers.
//!
//! Responsibilities:
//! - Verify worker command construction, task selection, and exclusion rules.
//! - Keep the heavy scenario coverage out of the production worker facade.
//!
//! Non-scope:
//! - Production worker orchestration logic.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;
use crate::agent::AgentOverrides;
use crate::commands::run::parallel::state;
use crate::config;
use crate::git::WorkspaceSpec;
use crate::queue;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tempfile::TempDir;

#[path = "worker_tests/command.rs"]
mod command;
#[path = "worker_tests/exclusion_and_locking.rs"]
mod exclusion_and_locking;
#[path = "worker_tests/ordering_and_blocking.rs"]
mod ordering_and_blocking;
#[path = "worker_tests/repair_and_validation.rs"]
mod repair_and_validation;
