//! Queue reloading and external change detection for the TUI.
//!
//! Responsibilities:
//! - Reload queue files from disk
//! - Detect external modifications via mtime tracking
//! - Handle completion callbacks for scans and task builder
//!
//! Not handled here:
//! - Actual file I/O (see queue module)
//! - Lock management (see lock module)
//! - Auto-save logic (see app.rs or app_persistence)
//!
//! Invariants/assumptions:
//! - Reload preserves selected task when possible
//! - External changes trigger automatic reload
//! - mtime caching prevents self-triggered reloads

use std::path::Path;

/// Trait for queue reload operations.
pub trait ReloadOperations {
    /// Reload the queue + done archive from disk.
    ///
    /// Preserves the currently selected task if possible.
    fn reload_queues_from_disk(&mut self, queue_path: &Path, done_path: &Path);

    /// Check if queue files have been modified externally and reload if necessary.
    ///
    /// Returns true if external changes were detected and reloaded.
    fn check_external_changes_and_reload(&mut self, queue_path: &Path, done_path: &Path) -> bool;

    /// Update cached mtimes after save operations.
    fn update_cached_mtimes(&mut self, queue_path: &Path, done_path: &Path);

    /// Handle scan completion: reload queue, set status, and return to normal mode.
    fn on_scan_finished(&mut self, queue_path: &Path, done_path: &Path);

    /// Handle task builder completion: reload queue, set status, and return to normal mode.
    fn on_task_builder_finished(&mut self, queue_path: &Path, done_path: &Path);

    /// Handle scan error: set error message and return to normal mode.
    fn on_scan_error(&mut self, msg: &str);

    /// Handle task builder error: set error message and return to normal mode.
    fn on_task_builder_error(&mut self, msg: &str);
}

// Implementation for App is in app.rs to avoid circular dependencies
