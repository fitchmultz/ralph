//! Completion signal helpers for supervised Phase 3 task completion.

use crate::contracts::TaskStatus;
use crate::fsutil;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionSignal {
    pub task_id: String,
    pub status: TaskStatus,
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_used: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_used: Option<String>,
}

pub fn completion_signal_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph").join("cache").join("completions")
}

pub fn completion_signal_path(repo_root: &Path, task_id: &str) -> Result<PathBuf> {
    let normalized = normalize_task_id(task_id)?;
    Ok(completion_signal_dir(repo_root).join(format!("{normalized}.json")))
}

pub fn write_completion_signal(repo_root: &Path, signal: &CompletionSignal) -> Result<PathBuf> {
    validate_terminal_status(signal.status)?;
    let normalized = normalize_task_id(&signal.task_id)?;
    let path = completion_signal_path(repo_root, &normalized)?;
    let rendered = serde_json::to_string_pretty(signal).context("serialize completion signal")?;
    fsutil::write_atomic(&path, rendered.as_bytes())
        .with_context(|| format!("write completion signal {}", path.display()))?;
    Ok(path)
}

pub fn read_completion_signal(repo_root: &Path, task_id: &str) -> Result<Option<CompletionSignal>> {
    let normalized = normalize_task_id(task_id)?;
    let path = completion_signal_path(repo_root, &normalized)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read completion signal {}", path.display()))?;
    let signal: CompletionSignal =
        serde_json::from_str(&raw).context("parse completion signal JSON")?;
    let signal_id = normalize_task_id(&signal.task_id)?;
    if signal_id != normalized {
        bail!(
            "Completion signal task id mismatch: expected {}, got {}",
            normalized,
            signal_id
        );
    }
    validate_terminal_status(signal.status)?;
    Ok(Some(signal))
}

pub fn take_completion_signal(repo_root: &Path, task_id: &str) -> Result<Option<CompletionSignal>> {
    let normalized = normalize_task_id(task_id)?;
    let path = completion_signal_path(repo_root, &normalized)?;
    if !path.exists() {
        return Ok(None);
    }
    let signal = read_completion_signal(repo_root, &normalized)?;
    if signal.is_some() {
        std::fs::remove_file(&path)
            .with_context(|| format!("remove completion signal {}", path.display()))?;
    }
    Ok(signal)
}

fn normalize_task_id(task_id: &str) -> Result<String> {
    let trimmed = task_id.trim();
    if trimmed.is_empty() {
        bail!("Missing task id for completion signal.");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        bail!(
            "Invalid task id for completion signal (path separators are not allowed): {}",
            trimmed
        );
    }
    Ok(trimmed.to_string())
}

fn validate_terminal_status(status: TaskStatus) -> Result<()> {
    match status {
        TaskStatus::Done | TaskStatus::Rejected => Ok(()),
        _ => bail!(
            "Invalid completion signal status: {:?} (expected done or rejected)",
            status
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::TaskStatus;
    use tempfile::TempDir;

    #[test]
    fn completion_signal_roundtrip_and_take_removes_file() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();

        let signal = CompletionSignal {
            task_id: "RQ-0001".to_string(),
            status: TaskStatus::Done,
            notes: vec!["Reviewed".to_string()],
            runner_used: None,
            model_used: None,
        };

        let path = write_completion_signal(repo_root, &signal)?;
        assert!(path.exists());

        let loaded = read_completion_signal(repo_root, "RQ-0001")?;
        assert_eq!(loaded, Some(signal.clone()));

        let taken = take_completion_signal(repo_root, "RQ-0001")?;
        assert_eq!(taken, Some(signal));
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn read_completion_signal_missing_returns_none() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let loaded = read_completion_signal(repo_root, "RQ-0001")?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[test]
    fn write_completion_signal_rejects_non_terminal_status() {
        let temp = TempDir::new().expect("temp dir");
        let repo_root = temp.path();

        let signal = CompletionSignal {
            task_id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            notes: vec![],
            runner_used: None,
            model_used: None,
        };

        let err = write_completion_signal(repo_root, &signal).unwrap_err();
        assert!(format!("{err}").contains("Invalid completion signal status"));
    }

    #[test]
    fn completion_signal_path_rejects_empty_task_id() {
        let temp = TempDir::new().expect("temp dir");
        let repo_root = temp.path();

        let err = completion_signal_path(repo_root, " ").unwrap_err();
        assert!(format!("{err}").contains("Missing task id"));
    }

    #[test]
    fn completion_signal_path_rejects_path_separators() {
        let temp = TempDir::new().expect("temp dir");
        let repo_root = temp.path();

        let err = completion_signal_path(repo_root, "RQ/0001").unwrap_err();
        assert!(format!("{err}").contains("path separators"));
    }
}
