//! Queue loader entrypoints for plain reads and parse repair.
//!
//! Purpose:
//! - Queue loader entrypoints for plain reads and parse repair.
//!
//! Responsibilities:
//! - Load queue files with plain JSONC parsing or in-memory parse repair.
//! - Coordinate queue/done loading for read-only validation flows.
//! - Keep semantic repair writes out of read paths.
//!
//! Not handled here:
//! - Timestamp normalization details (see `queue::repair`).
//! - Validation rule definitions (see `queue::validation`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Read-only flows never write queue or done files.
//! - Semantic repair application is handled by undo-backed queue repair APIs.

use super::validation::validate_loaded_queues;
use crate::config::Resolved;
use crate::contracts::QueueFile;
use crate::queue::json_repair::attempt_json_repair;
use crate::queue::validation::{self, ValidationWarning};
use anyhow::{Context, Result};
use std::path::Path;

/// Load queue from path, returning default if file doesn't exist.
pub fn load_queue_or_default(path: &Path) -> Result<QueueFile> {
    if !path.exists() {
        return Ok(QueueFile::default());
    }
    load_queue(path)
}

/// Load queue from path with standard JSONC parsing.
pub fn load_queue(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    let queue = crate::jsonc::parse_jsonc::<QueueFile>(&raw, &format!("queue {}", path.display()))?;
    Ok(queue)
}

/// Load queue with automatic repair for common JSON errors.
/// Attempts to fix trailing commas and other common agent-induced mistakes.
pub fn load_queue_with_repair(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;

    match crate::jsonc::parse_jsonc::<QueueFile>(&raw, &format!("queue {}", path.display())) {
        Ok(queue) => Ok(queue),
        Err(parse_err) => {
            log::warn!("Queue JSON parse error, attempting repair: {}", parse_err);

            if let Some(repaired) = attempt_json_repair(&raw) {
                match crate::jsonc::parse_jsonc::<QueueFile>(
                    &repaired,
                    &format!("repaired queue {}", path.display()),
                ) {
                    Ok(queue) => {
                        log::info!("Successfully repaired queue JSON");
                        Ok(queue)
                    }
                    Err(repair_err) => Err(parse_err).with_context(|| {
                        format!(
                            "parse queue {} as JSON/JSONC (repair also failed: {})",
                            path.display(),
                            repair_err
                        )
                    })?,
                }
            } else {
                Err(parse_err)
            }
        }
    }
}

/// Load queue with JSON repair and semantic validation.
///
/// This API is pure with respect to the filesystem: it may repair parseable JSON
/// mistakes in memory, but it never rewrites the queue file on disk.
///
/// Returns the queue file and any validation warnings (non-blocking issues).
pub fn load_queue_with_repair_and_validate(
    path: &Path,
    done: Option<&crate::contracts::QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<(QueueFile, Vec<ValidationWarning>)> {
    let queue = load_queue_with_repair(path)?;

    let warnings = if let Some(d) = done {
        validation::validate_queue_set(&queue, Some(d), id_prefix, id_width, max_dependency_depth)
            .with_context(|| format!("validate repaired queue {}", path.display()))?
    } else {
        validation::validate_queue(&queue, id_prefix, id_width)
            .with_context(|| format!("validate repaired queue {}", path.display()))?;
        Vec::new()
    };

    Ok((queue, warnings))
}

fn load_queue_set_with_repair(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, QueueFile, bool)> {
    let queue_file = load_queue_with_repair(&resolved.queue_path)?;
    let done_path_exists = resolved.done_path.exists();
    let done_file = if done_path_exists {
        load_queue_with_repair(&resolved.done_path)?
    } else {
        QueueFile::default()
    };

    let done_file = if include_done || done_path_exists {
        done_file
    } else {
        QueueFile::default()
    };

    Ok((queue_file, done_file, done_path_exists))
}

/// Load the active queue and optionally the done queue, validating both.
///
/// This API is pure with respect to the filesystem: it may repair parseable JSON
/// in memory, but it never rewrites queue/done files during the read.
pub fn load_and_validate_queues(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let (queue_file, done_for_validation, _done_path_exists) =
        load_queue_set_with_repair(resolved, include_done)?;
    validate_loaded_queues(resolved, &queue_file, &done_for_validation)?;

    let done_file = if include_done {
        Some(done_for_validation)
    } else {
        None
    };

    Ok((queue_file, done_file))
}
