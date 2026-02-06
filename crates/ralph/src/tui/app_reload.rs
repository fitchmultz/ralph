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

use crate::queue;
use crate::tui::app::App;
use crate::tui::events::AppMode;

// Implementation for App
impl ReloadOperations for App {
    fn reload_queues_from_disk(&mut self, queue_path: &Path, done_path: &Path) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());

        match queue::load_queue(queue_path) {
            Ok(new_queue) => {
                self.queue = new_queue;
            }
            Err(e) => {
                self.set_status_message(format!("Reload error: {}", e));
                return;
            }
        }

        match queue::load_queue_or_default(done_path) {
            Ok(new_done) => {
                self.done = new_done;
            }
            Err(e) => {
                self.set_status_message(format!("Reload error (done): {}", e));
                return;
            }
        }

        self.bump_queue_rev();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
        self.dirty = false;
        self.dirty_done = false;
        self.save_error = None;
    }

    fn check_external_changes_and_reload(&mut self, queue_path: &Path, done_path: &Path) -> bool {
        let queue_current = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        let done_current = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());

        let queue_changed = queue_current != self.queue_mtime;
        let done_changed = done_current != self.done_mtime;

        if queue_changed || done_changed {
            self.reload_queues_from_disk(queue_path, done_path);
            self.queue_mtime = queue_current;
            self.done_mtime = done_current;
            self.set_status_message("External changes detected - reloaded".to_string());
            true
        } else {
            false
        }
    }

    fn update_cached_mtimes(&mut self, queue_path: &Path, done_path: &Path) {
        self.queue_mtime = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.done_mtime = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());
    }

    fn on_scan_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Scan completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    fn on_task_builder_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Task builder completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    fn on_scan_error(&mut self, msg: &str) {
        self.set_status_message(format!("Scan error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    fn on_task_builder_error(&mut self, msg: &str) {
        self.set_status_message(format!("Task builder error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }
}
