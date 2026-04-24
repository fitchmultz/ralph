//! Persistence helpers for integration remediation state.
//!
//! Purpose:
//! - Persistence helpers for integration remediation state.
//!
//! Responsibilities:
//! - Read/write blocked-push marker files.
//! - Persist remediation handoff packets for later operator recovery.
//!
//! Non-scope:
//! - Deciding when a task is blocked.
//! - Compliance evaluation or prompt rendering.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::timeutil;

use super::types::{BlockedPushMarker, RemediationHandoff};

fn blocked_push_marker_path(workspace_path: &Path) -> PathBuf {
    workspace_path.join(super::super::BLOCKED_PUSH_MARKER_FILE)
}

pub(super) fn write_blocked_push_marker(
    workspace_path: &Path,
    task_id: &str,
    reason: &str,
    attempt: u32,
    max_attempts: u32,
) -> Result<()> {
    let marker = BlockedPushMarker {
        task_id: task_id.trim().to_string(),
        reason: reason.to_string(),
        attempt,
        max_attempts,
        generated_at: timeutil::now_utc_rfc3339_or_fallback(),
    };
    let marker_path = blocked_push_marker_path(workspace_path);
    if let Some(parent) = marker_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create blocked marker directory {}", parent.display()))?;
    }
    let rendered = serde_json::to_string_pretty(&marker).context("serialize blocked marker")?;
    crate::fsutil::write_atomic(&marker_path, rendered.as_bytes())
        .with_context(|| format!("write blocked marker {}", marker_path.display()))?;
    Ok(())
}

pub(super) fn clear_blocked_push_marker(workspace_path: &Path) {
    let marker_path = blocked_push_marker_path(workspace_path);
    if !marker_path.exists() {
        return;
    }
    if let Err(err) = std::fs::remove_file(&marker_path) {
        log::warn!(
            "Failed to clear blocked marker at {}: {}",
            marker_path.display(),
            err
        );
    }
}

pub(crate) fn read_blocked_push_marker(workspace_path: &Path) -> Result<Option<BlockedPushMarker>> {
    let marker_path = blocked_push_marker_path(workspace_path);
    if !marker_path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&marker_path)
        .with_context(|| format!("read blocked marker {}", marker_path.display()))?;
    let marker =
        serde_json::from_str::<BlockedPushMarker>(&raw).context("parse blocked marker json")?;
    Ok(Some(marker))
}

pub fn write_handoff_packet(
    workspace_path: &Path,
    task_id: &str,
    attempt: u32,
    handoff: &RemediationHandoff,
) -> Result<PathBuf> {
    let handoff_dir = workspace_path
        .join(".ralph/cache/parallel/handoffs")
        .join(task_id);
    std::fs::create_dir_all(&handoff_dir)
        .with_context(|| format!("create handoff directory {}", handoff_dir.display()))?;

    let path = handoff_dir.join(format!("attempt_{}.json", attempt));
    let content = serde_json::to_string_pretty(handoff).context("serialize handoff packet")?;
    crate::fsutil::write_atomic(&path, content.as_bytes())
        .with_context(|| format!("write handoff packet to {}", path.display()))?;
    Ok(path)
}
