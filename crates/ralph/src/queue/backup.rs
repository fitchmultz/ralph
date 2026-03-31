//! Queue file backup and cleanup functionality.
//!
//! Responsibilities:
//! - Create timestamped backups of queue files before modifications.
//! - Prune old backups to prevent unbounded growth (respects MAX_QUEUE_BACKUP_FILES).
//!
//! Not handled here:
//! - Actual file modification (callers handle that after backup).
//! - Lock acquisition (assumed to be held by caller).
//!
//! Invariants/assumptions:
//! - Backup directory is writable; failures are logged but not fatal.
//! - Backup file names follow the pattern: `queue.json.backup.<timestamp>`.

use crate::constants::limits::MAX_QUEUE_BACKUP_FILES;
use anyhow::{Context, Result};
use std::path::Path;
use std::path::PathBuf;

const QUEUE_BACKUP_PREFIX: &str = "queue.json.backup.";

/// Create a backup of the queue file before modification.
/// Returns the path to the backup file.
pub fn backup_queue(path: &Path, backup_dir: &Path) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(backup_dir)?;
    let timestamp = crate::timeutil::now_utc_rfc3339_or_fallback().replace([':', '.'], "-");
    let backup_name = format!("{QUEUE_BACKUP_PREFIX}{timestamp}");
    let backup_path = backup_dir.join(backup_name);

    std::fs::copy(path, &backup_path)
        .with_context(|| format!("backup queue to {}", backup_path.display()))?;

    match cleanup_queue_backups(backup_dir, MAX_QUEUE_BACKUP_FILES) {
        Ok(removed) if removed > 0 => {
            log::debug!(
                "pruned {} stale queue backup(s); retaining latest {}",
                removed,
                MAX_QUEUE_BACKUP_FILES
            );
        }
        Ok(_) => {
            // The backup set already fit within the retention cap, so no files were removed.
        }
        Err(err) => {
            log::warn!(
                "failed to prune queue backups in {}: {:#}",
                backup_dir.display(),
                err
            );
        }
    }

    Ok(backup_path)
}

pub(crate) fn cleanup_queue_backups(backup_dir: &Path, max_backups: usize) -> Result<usize> {
    if max_backups == 0 || !backup_dir.exists() {
        return Ok(0);
    }

    let mut backup_paths: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(backup_dir)
        .with_context(|| format!("read backup directory {}", backup_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("read backup directory entry in {}", backup_dir.display()))?;

        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with(QUEUE_BACKUP_PREFIX) {
            backup_paths.push(entry.path());
        }
    }

    if backup_paths.len() <= max_backups {
        return Ok(0);
    }

    backup_paths.sort_unstable_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });

    let mut removed = 0usize;
    let to_remove = backup_paths.len().saturating_sub(max_backups);
    for backup_path in backup_paths.into_iter().take(to_remove) {
        std::fs::remove_file(&backup_path)
            .with_context(|| format!("remove queue backup {}", backup_path.display()))?;
        removed += 1;
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use crate::fsutil;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["code".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }
    }

    fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
        let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
        fsutil::write_atomic(path, rendered.as_bytes())
            .with_context(|| format!("write queue JSON {}", path.display()))?;
        Ok(())
    }

    #[test]
    fn backup_queue_creates_backup_file() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let backup_dir = temp.path().join("backups");

        // Create initial queue
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;

        // Create backup
        let backup_path = backup_queue(&queue_path, &backup_dir)?;

        // Verify backup exists and contains valid JSON
        assert!(backup_path.exists());
        let backup_queue: QueueFile =
            serde_json::from_str(&std::fs::read_to_string(&backup_path)?)?;
        assert_eq!(backup_queue.tasks.len(), 1);
        assert_eq!(backup_queue.tasks[0].id, "RQ-0001");

        Ok(())
    }

    #[test]
    fn cleanup_queue_backups_removes_oldest_files() -> Result<()> {
        let temp = TempDir::new()?;
        let backup_dir = temp.path().join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        for suffix in ["0001", "0002", "0003"] {
            let backup_path = backup_dir.join(format!("{QUEUE_BACKUP_PREFIX}{suffix}"));
            std::fs::write(backup_path, "{}")?;
        }

        let removed = cleanup_queue_backups(&backup_dir, 2)?;
        assert_eq!(removed, 1);
        assert!(
            !backup_dir
                .join(format!("{QUEUE_BACKUP_PREFIX}0001"))
                .exists()
        );
        assert!(
            backup_dir
                .join(format!("{QUEUE_BACKUP_PREFIX}0002"))
                .exists()
        );
        assert!(
            backup_dir
                .join(format!("{QUEUE_BACKUP_PREFIX}0003"))
                .exists()
        );

        Ok(())
    }

    #[test]
    fn backup_queue_prunes_backups_to_retention_limit() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let backup_dir = temp.path().join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;

        for idx in 0..(MAX_QUEUE_BACKUP_FILES + 2) {
            let backup_path = backup_dir.join(format!("{QUEUE_BACKUP_PREFIX}0000-{idx:04}"));
            std::fs::write(backup_path, "{}")?;
        }

        let _backup_path = backup_queue(&queue_path, &backup_dir)?;

        let backup_count = std::fs::read_dir(&backup_dir)?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(QUEUE_BACKUP_PREFIX))
            .count();

        assert_eq!(backup_count, MAX_QUEUE_BACKUP_FILES);

        Ok(())
    }
}
