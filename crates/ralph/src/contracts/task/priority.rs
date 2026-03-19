//! Purpose: Define task-priority ordering, formatting, and parsing helpers.
//!
//! Responsibilities:
//! - Define `TaskPriority`.
//! - Provide ordering/weight/cycle helpers.
//! - Provide `Display` and `FromStr` support.
//!
//! Scope:
//! - Priority behavior only; task/task-agent data models and serde/schema
//!   helpers live in sibling modules.
//!
//! Usage:
//! - Used by task contracts and queue sorting through `crate::contracts`.
//!
//! Invariants/Assumptions:
//! - Ordering remains critical > high > medium > low.
//! - String parsing stays case-insensitive with canonical lowercase output.

use std::str::FromStr;

use anyhow::{Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Task priority used for queue ordering and display.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Critical,
    High,
    #[default]
    Medium,
    Low,
}

impl PartialOrd for TaskPriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaskPriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.weight().cmp(&other.weight())
    }
}

impl TaskPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskPriority::Critical => "critical",
            TaskPriority::High => "high",
            TaskPriority::Medium => "medium",
            TaskPriority::Low => "low",
        }
    }

    pub fn weight(self) -> u8 {
        match self {
            TaskPriority::Critical => 3,
            TaskPriority::High => 2,
            TaskPriority::Medium => 1,
            TaskPriority::Low => 0,
        }
    }

    /// Cycle to the next priority in ascending order, wrapping after Critical.
    pub fn cycle(self) -> Self {
        match self {
            TaskPriority::Low => TaskPriority::Medium,
            TaskPriority::Medium => TaskPriority::High,
            TaskPriority::High => TaskPriority::Critical,
            TaskPriority::Critical => TaskPriority::Low,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskPriority {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let token = value.trim();

        if token.eq_ignore_ascii_case("critical") {
            return Ok(TaskPriority::Critical);
        }
        if token.eq_ignore_ascii_case("high") {
            return Ok(TaskPriority::High);
        }
        if token.eq_ignore_ascii_case("medium") {
            return Ok(TaskPriority::Medium);
        }
        if token.eq_ignore_ascii_case("low") {
            return Ok(TaskPriority::Low);
        }

        bail!(
            "Invalid priority: '{}'. Expected one of: critical, high, medium, low.",
            token
        )
    }
}
