//! Queue file saving functionality.
//!
//! Purpose:
//! - Queue file saving functionality.
//!
//! Responsibilities:
//! - Serialize queue files to JSON with pretty formatting.
//! - Write queue files atomically to prevent corruption.
//!
//! Not handled here:
//! - Queue file loading or backup creation.
//! - Lock acquisition (assumed to be held by caller).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue files are written atomically using `write_atomic`.
//! - Serialization should never fail for valid QueueFile structures.

use crate::contracts::QueueFile;
use crate::fsutil;
use anyhow::{Context, Result};
use std::path::Path;

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(())
}
