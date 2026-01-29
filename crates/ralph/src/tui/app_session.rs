//! TUI session management and queue persistence.
//!
//! Responsibilities:
//! - Queue persistence operations (save/load).
//! - External change detection and reload.
//! - Session lifecycle callbacks (on_scan_finished, etc.).
//! - Auto-save functionality.
//!
//! Not handled here:
//! - Task mutation operations (see app_tasks module).
//! - Terminal setup/teardown (see app module).
//! - Event loop handling (see app module).
//!
//! Invariants/assumptions:
//! - Queue paths are valid and accessible.
//! - Modification times are cached after successful operations.
//! - External changes are detected by comparing mtimes.

use crate::config::ConfigLayer;
use crate::contracts::QueueFile;
use crate::{config as crate_config, queue};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// State for TUI session management.
#[derive(Debug, Default)]
pub struct SessionState {
    /// Cached modification time for queue.json (for detecting external changes).
    queue_mtime: Option<std::time::SystemTime>,
    /// Cached modification time for done.json (for detecting external changes).
    done_mtime: Option<std::time::SystemTime>,
    /// Last auto-save error message, if any.
    pub save_error: Option<String>,
    /// Whether the queue has unsaved changes.
    pub dirty: bool,
    /// Whether the done archive has unsaved changes.
    pub dirty_done: bool,
    /// Whether the project config has unsaved changes.
    pub dirty_config: bool,
}

impl SessionState {
    /// Create a new session state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize cached mtimes from the given paths.
    pub fn initialize_cached_mtimes(&mut self, queue_path: &Path, done_path: &Path) {
        self.queue_mtime = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.done_mtime = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());
    }

    /// Update cached mtimes after save operations.
    pub fn update_cached_mtimes(&mut self, queue_path: &Path, done_path: &Path) {
        self.queue_mtime = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.done_mtime = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());
    }

    /// Check if queue files have been modified externally.
    ///
    /// Returns true if external changes were detected.
    pub fn check_external_changes(&self, queue_path: &Path, done_path: &Path) -> bool {
        let queue_current = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        let done_current = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());

        let queue_changed = queue_current != self.queue_mtime;
        let done_changed = done_current != self.done_mtime;

        queue_changed || done_changed
    }

    /// Get the cached queue mtime.
    pub fn queue_mtime(&self) -> Option<std::time::SystemTime> {
        self.queue_mtime
    }

    /// Get the cached done mtime.
    pub fn done_mtime(&self) -> Option<std::time::SystemTime> {
        self.done_mtime
    }

    /// Check if there are any unsaved changes.
    pub fn has_unsaved_changes(&self) -> bool {
        self.dirty || self.dirty_done || self.dirty_config
    }

    /// Mark all as saved and clear errors.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.dirty_done = false;
        self.dirty_config = false;
        self.save_error = None;
    }
}

/// Reload the queue and done archive from disk.
///
/// Returns the loaded queues. The caller should handle updating
/// the app state and selection.
pub fn reload_queues_from_disk(
    queue_path: &Path,
    done_path: &Path,
) -> Result<(QueueFile, QueueFile)> {
    let queue = queue::load_queue(queue_path)?;
    let done = queue::load_queue_or_default(done_path)?;
    Ok((queue, done))
}

/// Auto-save dirty data to disk.
///
/// Saves the queue, done archive, and project config if they are dirty.
/// Updates cached mtimes on successful save.
#[allow(clippy::too_many_arguments)]
pub fn auto_save_if_dirty(
    queue: &QueueFile,
    done: &QueueFile,
    project_config: &ConfigLayer,
    session: &mut SessionState,
    queue_path: &Path,
    done_path: &Path,
    project_config_path: Option<&Path>,
) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    if session.dirty {
        match queue::save_queue(queue_path, queue) {
            Ok(()) => {
                session.dirty = false;
            }
            Err(e) => {
                errors.push(format!("ERROR saving queue: {}", e));
            }
        }
    }

    if session.dirty_done {
        match queue::save_queue(done_path, done) {
            Ok(()) => {
                session.dirty_done = false;
            }
            Err(e) => {
                errors.push(format!("ERROR saving done: {}", e));
            }
        }
    }

    if session.dirty_config {
        match project_config_path {
            Some(path) => match crate_config::save_layer(path, project_config) {
                Ok(()) => {
                    session.dirty_config = false;
                }
                Err(e) => {
                    errors.push(format!("ERROR saving config: {}", e));
                }
            },
            None => {
                errors.push("ERROR saving config: missing project config path".to_string());
            }
        }
    }

    if errors.is_empty() {
        session.save_error = None;
        // Update cached mtimes to avoid triggering external change detection
        // for our own saves
        session.update_cached_mtimes(queue_path, done_path);
        Ok(())
    } else {
        let message = errors.join(" | ");
        let should_log = session.save_error.as_deref() != Some(message.as_str());
        session.save_error = Some(message.clone());
        if should_log {
            // Note: Caller should log this if needed
        }
        Err(errors)
    }
}

/// Result of a session save operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveResult {
    /// All dirty data was saved successfully.
    Success,
    /// No data was dirty, nothing to save.
    NothingToSave,
    /// Some or all saves failed.
    Failed(Vec<String>),
}

/// Session manager that coordinates persistence operations.
pub struct SessionManager {
    /// Queue file path.
    queue_path: PathBuf,
    /// Done archive path.
    done_path: PathBuf,
    /// Project config path (optional).
    project_config_path: Option<PathBuf>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(
        queue_path: PathBuf,
        done_path: PathBuf,
        project_config_path: Option<PathBuf>,
    ) -> Self {
        Self {
            queue_path,
            done_path,
            project_config_path,
        }
    }

    /// Get the queue path.
    pub fn queue_path(&self) -> &Path {
        &self.queue_path
    }

    /// Get the done path.
    pub fn done_path(&self) -> &Path {
        &self.done_path
    }

    /// Get the project config path.
    pub fn project_config_path(&self) -> Option<&Path> {
        self.project_config_path.as_deref()
    }

    /// Check for external changes and reload if necessary.
    ///
    /// Returns true if external changes were detected and reloaded.
    pub fn check_and_reload(
        &self,
        session: &mut SessionState,
        current_queue: &mut QueueFile,
        current_done: &mut QueueFile,
    ) -> bool {
        if session.check_external_changes(&self.queue_path, &self.done_path) {
            match reload_queues_from_disk(&self.queue_path, &self.done_path) {
                Ok((new_queue, new_done)) => {
                    *current_queue = new_queue;
                    *current_done = new_done;
                    session.mark_saved();
                    session.initialize_cached_mtimes(&self.queue_path, &self.done_path);
                    true
                }
                Err(_) => {
                    // If reload fails, don't report external changes
                    false
                }
            }
        } else {
            false
        }
    }

    /// Save all dirty data.
    ///
    /// Returns the result of the save operation.
    pub fn save(
        &self,
        session: &mut SessionState,
        queue: &QueueFile,
        done: &QueueFile,
        project_config: &ConfigLayer,
    ) -> SaveResult {
        if !session.has_unsaved_changes() {
            return SaveResult::NothingToSave;
        }

        match auto_save_if_dirty(
            queue,
            done,
            project_config,
            session,
            &self.queue_path,
            &self.done_path,
            self.project_config_path.as_deref(),
        ) {
            Ok(()) => SaveResult::Success,
            Err(errors) => SaveResult::Failed(errors),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_state_default() {
        let state = SessionState::new();
        assert!(!state.dirty);
        assert!(!state.dirty_done);
        assert!(!state.dirty_config);
        assert!(state.save_error.is_none());
    }

    #[test]
    fn test_has_unsaved_changes() {
        let mut state = SessionState::new();
        assert!(!state.has_unsaved_changes());

        state.dirty = true;
        assert!(state.has_unsaved_changes());

        state.dirty = false;
        state.dirty_done = true;
        assert!(state.has_unsaved_changes());

        state.dirty_done = false;
        state.dirty_config = true;
        assert!(state.has_unsaved_changes());
    }

    #[test]
    fn test_mark_saved() {
        let mut state = SessionState::new();
        state.dirty = true;
        state.dirty_done = true;
        state.dirty_config = true;
        state.save_error = Some("error".to_string());

        state.mark_saved();

        assert!(!state.dirty);
        assert!(!state.dirty_done);
        assert!(!state.dirty_config);
        assert!(state.save_error.is_none());
    }

    #[test]
    fn test_check_external_changes() {
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        // Create initial files
        std::fs::write(&queue_path, "{\"tasks\":[]}").unwrap();
        std::fs::write(&done_path, "{\"tasks\":[]}").unwrap();

        let mut state = SessionState::new();
        state.initialize_cached_mtimes(&queue_path, &done_path);

        // No changes initially
        assert!(!state.check_external_changes(&queue_path, &done_path));

        // Modify queue file
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&queue_path, "{\"tasks\":[{\"id\":\"RQ-0001\"}]}").unwrap();

        // Should detect change
        assert!(state.check_external_changes(&queue_path, &done_path));
    }

    #[test]
    fn test_save_result() {
        assert_eq!(SaveResult::Success, SaveResult::Success);
        assert_eq!(
            SaveResult::Failed(vec!["error".to_string()]),
            SaveResult::Failed(vec!["error".to_string()])
        );
        assert_ne!(SaveResult::Success, SaveResult::NothingToSave);
    }
}
