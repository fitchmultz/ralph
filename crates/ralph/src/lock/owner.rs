//! Lock owner metadata.
//!
//! Purpose:
//! - Lock owner metadata.
//!
//! Responsibilities:
//! - Define lock owner metadata and parse/render helpers.
//! - Read and write owner files for lock directories.
//! - Identify task sidecar owner files and supervising labels.
//!
//! Not handled here:
//! - Lock acquisition policy or stale-lock decisions.
//! - PID liveness detection.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Owner files are small text blobs with one `key: value` pair per line.
//! - Task owner sidecars use the `owner_task_` filename prefix.
//! - `started_at` is advisory age metadata for lock review; it is not a
//!   process identity proof.

use crate::fsutil::sync_dir_best_effort;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::io::Write;
use std::path::Path;

pub(crate) const OWNER_FILE_NAME: &str = "owner";
pub const TASK_OWNER_PREFIX: &str = "owner_task_";

/// Lock owner metadata parsed from the owner file.
#[derive(Debug, Clone)]
pub struct LockOwner {
    pub pid: u32,
    pub started_at: String,
    pub command: String,
    pub label: String,
}

impl LockOwner {
    pub(crate) fn render(&self) -> String {
        format!(
            "pid: {}\nstarted_at: {}\ncommand: {}\nlabel: {}\n",
            self.pid, self.started_at, self.command, self.label
        )
    }
}

pub fn read_lock_owner(lock_dir: &Path) -> Result<Option<LockOwner>> {
    let owner_path = lock_dir.join(OWNER_FILE_NAME);
    let raw = match fs::read_to_string(&owner_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("read lock owner {}", owner_path.display()));
        }
    };
    Ok(parse_lock_owner(&raw))
}

pub(crate) fn write_lock_owner(owner_path: &Path, owner: &LockOwner) -> Result<()> {
    log::debug!("writing lock owner: {}", owner_path.display());
    let mut file = fs::File::create(owner_path)
        .with_context(|| format!("create lock owner {}", owner_path.display()))?;
    file.write_all(owner.render().as_bytes())
        .context("write lock owner")?;
    file.flush().context("flush lock owner")?;
    file.sync_all().context("sync lock owner")?;
    if let Some(parent) = owner_path.parent() {
        sync_dir_best_effort(parent);
    }
    Ok(())
}

pub(crate) fn parse_lock_owner(raw: &str) -> Option<LockOwner> {
    let mut pid = None;
    let mut started_at = None;
    let mut command = None;
    let mut label = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim().to_string();
            match key.trim() {
                "pid" => {
                    pid = value
                        .parse::<u32>()
                        .inspect_err(|error| {
                            log::debug!("Lock file has invalid pid '{}': {}", value, error)
                        })
                        .ok()
                }
                "started_at" => started_at = Some(value),
                "command" => command = Some(value),
                "label" => label = Some(value),
                _ => {}
            }
        }
    }

    let pid = pid?;
    Some(LockOwner {
        pid,
        started_at: started_at.unwrap_or_else(|| "unknown".to_string()),
        command: command.unwrap_or_else(|| "unknown".to_string()),
        label: label.unwrap_or_else(|| "unknown".to_string()),
    })
}

pub(crate) fn command_line() -> String {
    let joined = std::env::args().collect::<Vec<_>>().join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn is_supervising_label(label: &str) -> bool {
    matches!(label, "run one" | "run loop")
}

pub fn is_task_owner_file(name: &str) -> bool {
    name.starts_with(TASK_OWNER_PREFIX)
}

pub(crate) fn is_task_sidecar_owner(owner_path: &Path) -> bool {
    owner_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(is_task_owner_file)
        .unwrap_or(false)
}
