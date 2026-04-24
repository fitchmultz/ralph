//! Queue file contracts for Ralph.
//!
//! Purpose:
//! - Queue file contracts for Ralph.
//!
//! Responsibilities:
//! - Define the queue file payload structure and defaults.
//!
//! Not handled here:
//! - Queue persistence or scheduling logic (see `crate::queue`).
//! - Task field definitions (see `super::task`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `version` is the queue schema version.
//! - Tasks are serialized/deserialized with strict field validation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Task;

/* --------------------------- QueueFile (JSON) ---------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueFile {
    pub version: u32,

    #[serde(default)]
    pub tasks: Vec<Task>,
}

/* ------------------------------ Defaults -------------------------------- */

impl Default for QueueFile {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: Vec::new(),
        }
    }
}
