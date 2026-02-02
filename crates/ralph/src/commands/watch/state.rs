//! State management for the watch command.
//!
//! Responsibilities:
//! - Track pending files that need processing.
//! - Manage debounce timing to batch rapid file changes.
//!
//! Not handled here:
//! - File watching or event handling (see `event_loop.rs`).
//! - Comment detection (see `comments.rs`).
//! - Task creation (see `tasks.rs`).
//!
//! Invariants/assumptions:
//! - `WatchState` is protected by a mutex in the watch loop.
//! - `debounce_duration` is derived from `WatchOptions.debounce_ms`.
//! - `last_event` is updated when files are added or taken.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Internal state for the file watcher.
pub struct WatchState {
    pub pending_files: HashSet<PathBuf>,
    pub last_event: Instant,
    pub debounce_duration: Duration,
}

impl WatchState {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            pending_files: HashSet::new(),
            last_event: Instant::now(),
            debounce_duration: Duration::from_millis(debounce_ms),
        }
    }

    /// Add a file to pending set. Returns true if debounce window has elapsed.
    pub fn add_file(&mut self, path: PathBuf) -> bool {
        self.pending_files.insert(path);
        let now = Instant::now();
        if now.duration_since(self.last_event) >= self.debounce_duration {
            self.last_event = now;
            true
        } else {
            false
        }
    }

    /// Take all pending files and reset the event timer.
    pub fn take_pending(&mut self) -> Vec<PathBuf> {
        let files: Vec<PathBuf> = self.pending_files.drain().collect();
        self.last_event = Instant::now();
        files
    }
}
