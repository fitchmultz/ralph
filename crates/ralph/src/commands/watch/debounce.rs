//! Debounce helpers for the watch command.
//!
//! Responsibilities:
//! - Determine if a file can be reprocessed based on last processing time.
//! - Clean up old entries from the processing history to prevent memory leaks.
//!
//! Not handled here:
//! - File watching or event handling (see `event_loop.rs`).
//! - State management (see `state.rs`).
//!
//! Invariants/assumptions:
//! - `can_reprocess` returns true if file has never been processed or if
//!   the debounce duration has elapsed since last processing.
//! - `cleanup_old_entries` removes entries older than 10x the debounce duration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Check if a file can be reprocessed based on when it was last processed.
///
/// A file can be reprocessed if:
/// - It has never been processed before, OR
/// - The time since last processing is >= the debounce duration
pub fn can_reprocess(
    path: &Path,
    last_processed: &HashMap<PathBuf, Instant>,
    debounce: Duration,
) -> bool {
    match last_processed.get(path) {
        Some(last_time) => Instant::now().duration_since(*last_time) >= debounce,
        None => true,
    }
}

/// Clean up old entries from the last_processed map to prevent unbounded growth.
///
/// Removes entries older than 10x the debounce duration.
pub fn cleanup_old_entries(last_processed: &mut HashMap<PathBuf, Instant>, debounce: Duration) {
    let cutoff = Instant::now() - debounce * 10;
    last_processed.retain(|_, timestamp| *timestamp >= cutoff);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_reprocess_new_file() {
        let last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path = Path::new("/test/file.rs");

        // New file should be reprocessable
        assert!(can_reprocess(
            path,
            &last_processed,
            Duration::from_millis(100)
        ));
    }

    #[test]
    fn can_reprocess_after_debounce() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path = PathBuf::from("/test/file.rs");

        // Insert a timestamp from the past (older than debounce)
        last_processed.insert(path.clone(), Instant::now() - Duration::from_millis(200));

        // Should be reprocessable after debounce period
        assert!(can_reprocess(
            &path,
            &last_processed,
            Duration::from_millis(100)
        ));
    }

    #[test]
    fn cannot_reprocess_within_debounce() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path = PathBuf::from("/test/file.rs");

        // Insert current timestamp
        last_processed.insert(path.clone(), Instant::now());

        // Should NOT be reprocessable within debounce period
        assert!(!can_reprocess(
            &path,
            &last_processed,
            Duration::from_millis(100)
        ));
    }

    #[test]
    fn cleanup_old_entries_removes_stale_entries() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let old_path = PathBuf::from("/test/old.rs");
        let recent_path = PathBuf::from("/test/recent.rs");

        // Insert an old entry (older than 10x debounce)
        last_processed.insert(
            old_path.clone(),
            Instant::now() - Duration::from_millis(1500),
        );
        // Insert a recent entry
        last_processed.insert(
            recent_path.clone(),
            Instant::now() - Duration::from_millis(50),
        );

        let debounce = Duration::from_millis(100);
        cleanup_old_entries(&mut last_processed, debounce);

        // Old entry should be removed
        assert!(!last_processed.contains_key(&old_path));
        // Recent entry should remain
        assert!(last_processed.contains_key(&recent_path));
    }

    #[test]
    fn cleanup_old_entries_preserves_recent_entries() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path1 = PathBuf::from("/test/file1.rs");
        let path2 = PathBuf::from("/test/file2.rs");

        // Insert entries within the cleanup window
        last_processed.insert(path1.clone(), Instant::now() - Duration::from_millis(500));
        last_processed.insert(path2.clone(), Instant::now() - Duration::from_millis(300));

        let debounce = Duration::from_millis(100);
        cleanup_old_entries(&mut last_processed, debounce);

        // Both entries should remain (both within 10x debounce = 1000ms)
        assert!(last_processed.contains_key(&path1));
        assert!(last_processed.contains_key(&path2));
    }
}
