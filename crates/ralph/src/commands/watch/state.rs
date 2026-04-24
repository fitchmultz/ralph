//! State management for the watch command.
//!
//! Purpose:
//! - State management for the watch command.
//!
//! Responsibilities:
//! - Track pending files that need processing.
//! - Manage debounce timing to batch rapid file changes.
//!
//! Not handled here:
//! - File watching or event handling (see `event_loop/mod.rs`).
//! - Comment detection (see `comments.rs`).
//! - Task creation (see `tasks.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_file(&mut self, path: PathBuf) -> bool {
        self.add_file_at(path, Instant::now())
    }

    /// Add a file to pending set using an injected timestamp.
    pub fn add_file_at(&mut self, path: PathBuf, now: Instant) -> bool {
        self.pending_files.insert(path);
        if now.duration_since(self.last_event) >= self.debounce_duration {
            self.last_event = now;
            true
        } else {
            false
        }
    }

    /// Take all pending files and reset the event timer.
    pub fn take_pending(&mut self) -> Vec<PathBuf> {
        self.take_pending_at(Instant::now())
    }

    /// Take all pending files and reset the event timer using an injected timestamp.
    pub fn take_pending_at(&mut self, now: Instant) -> Vec<PathBuf> {
        let files: Vec<PathBuf> = self.pending_files.drain().collect();
        self.last_event = now;
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_state_new_initializes_empty() {
        let state = WatchState::new(100);
        assert!(state.pending_files.is_empty());
        assert_eq!(state.debounce_duration, Duration::from_millis(100));
    }

    #[test]
    fn add_file_adds_to_pending() {
        let mut state = WatchState::new(100);
        let path = PathBuf::from("/test/file.rs");

        state.add_file(path.clone());

        assert!(state.pending_files.contains(&path));
    }

    #[test]
    fn add_file_returns_true_when_debounce_elapsed() {
        let mut state = WatchState::new(50); // 50ms debounce
        let path = PathBuf::from("/test/file.rs");
        let start = state.last_event;

        // First add sets last_event to now
        state.add_file_at(path.clone(), start);

        // Second add should return true (debounce elapsed)
        let should_process = state.add_file_at(
            PathBuf::from("/test/other.rs"),
            start + Duration::from_millis(60),
        );
        assert!(should_process);
    }

    #[test]
    fn add_file_returns_false_within_debounce() {
        let mut state = WatchState::new(1000); // 1 second debounce
        let path = PathBuf::from("/test/file.rs");

        // First add
        state.add_file(path.clone());

        // Immediate second add should return false
        let should_process = state.add_file(PathBuf::from("/test/other.rs"));
        assert!(!should_process);
    }

    #[test]
    fn take_pending_returns_all_files() {
        let mut state = WatchState::new(100);
        let path1 = PathBuf::from("/test/file1.rs");
        let path2 = PathBuf::from("/test/file2.rs");

        state.add_file(path1.clone());
        state.add_file(path2.clone());

        let pending = state.take_pending();

        assert_eq!(pending.len(), 2);
        assert!(pending.contains(&path1) || pending.contains(&path2));
    }

    #[test]
    fn take_pending_clears_pending_files() {
        let mut state = WatchState::new(100);
        state.add_file(PathBuf::from("/test/file.rs"));

        state.take_pending();

        assert!(state.pending_files.is_empty());
    }

    #[test]
    fn take_pending_resets_last_event() {
        let mut state = WatchState::new(100);
        let before = state.last_event;

        let now = before + Duration::from_millis(10);
        state.add_file_at(PathBuf::from("/test/file.rs"), now);
        state.take_pending_at(now);

        assert!(state.last_event > before);
    }

    #[test]
    fn add_file_deduplicates_paths() {
        let mut state = WatchState::new(100);
        let path = PathBuf::from("/test/file.rs");

        state.add_file(path.clone());
        state.add_file(path.clone()); // Duplicate

        let pending = state.take_pending();
        assert_eq!(pending.len(), 1);
    }
}
