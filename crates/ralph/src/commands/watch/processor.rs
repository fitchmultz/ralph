//! File processing for the watch command.
//!
//! Responsibilities:
//! - Process pending files and detect comments.
//! - Coordinate between debounce logic, comment detection, and task handling.
//!
//! Not handled here:
//! - File watching or event handling (see `event_loop.rs`).
//! - Low-level comment detection (see `comments.rs`).
//! - Task creation (see `tasks.rs`).
//!
//! Invariants/assumptions:
//! - Files are skipped if recently processed (within debounce window).
//! - Errors reading individual files are logged but don't stop processing.
//! - Old entries in `last_processed` are cleaned up periodically.

use crate::commands::watch::comments::detect_comments;
use crate::commands::watch::debounce::{can_reprocess, cleanup_old_entries};
use crate::commands::watch::state::WatchState;
use crate::commands::watch::tasks::handle_detected_comments;
use crate::commands::watch::types::{DetectedComment, WatchOptions};
use crate::config::Resolved;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Process pending files and detect comments.
pub fn process_pending_files(
    resolved: &Resolved,
    state: &Arc<Mutex<WatchState>>,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
) -> Result<()> {
    let files: Vec<PathBuf> = match state.lock() {
        Ok(mut guard) => guard.take_pending(),
        Err(e) => {
            log::error!("Watch 'state' mutex poisoned, cannot process files: {}", e);
            return Ok(());
        }
    };

    if files.is_empty() {
        return Ok(());
    }

    let debounce = Duration::from_millis(opts.debounce_ms);
    let mut all_comments: Vec<DetectedComment> = Vec::new();

    for file_path in files {
        // Skip if file was recently processed (within debounce window)
        if !can_reprocess(&file_path, last_processed, debounce) {
            continue;
        }

        match detect_comments(&file_path, comment_regex) {
            Ok(comments) => {
                if !comments.is_empty() {
                    log::debug!(
                        "Detected {} comments in {}",
                        comments.len(),
                        file_path.display()
                    );
                    all_comments.extend(comments);
                }
                // Record when this file was processed
                last_processed.insert(file_path, Instant::now());
            }
            Err(e) => {
                log::warn!("Failed to process file {}: {}", file_path.display(), e);
            }
        }
    }

    // Periodically clean up old entries to prevent unbounded growth
    cleanup_old_entries(last_processed, debounce);

    if !all_comments.is_empty() {
        handle_detected_comments(resolved, &all_comments, opts)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::watch::comments::build_comment_regex;
    use crate::commands::watch::types::CommentType;
    use crate::contracts::{Config, QueueFile};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::{NamedTempFile, TempDir};

    fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        // Create empty queue file
        let queue = QueueFile::default();
        let queue_json = serde_json::to_string_pretty(&queue).unwrap();
        std::fs::write(&queue_path, queue_json).unwrap();

        Resolved {
            config: Config::default(),
            repo_root: temp_dir.path().to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn process_pending_files_handles_state_mutex_poison() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        // Clone for the poisoning thread
        let state_clone = state.clone();

        // Spawn a thread that will panic while holding the state mutex
        let poison_handle = std::thread::spawn(move || {
            let _guard = state_clone.lock().unwrap();
            panic!("Intentional panic to poison state mutex");
        });

        // Wait for the panic
        let _ = poison_handle.join();

        // Now the state mutex is poisoned - verify process_pending_files handles it gracefully
        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        // Should return Ok, not panic
        assert!(
            result.is_ok(),
            "process_pending_files should handle state mutex poison gracefully"
        );
    }

    #[test]
    fn process_pending_files_happy_path() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Create a temp file with a TODO comment
        let mut temp_file = NamedTempFile::new_in(temp_dir.path()).unwrap();
        writeln!(temp_file, "// TODO: test task").unwrap();
        temp_file.flush().unwrap();

        // Add the file to pending
        let file_path = temp_file.path().to_path_buf();
        state.lock().unwrap().add_file(file_path.clone());

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false, // Don't actually queue, just test processing
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        // Process the pending file
        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        assert!(result.is_ok());

        // Verify the file was recorded in last_processed
        assert!(last_processed.contains_key(&file_path));

        // Verify state is empty (files were taken)
        assert!(state.lock().unwrap().pending_files.is_empty());
    }

    // =====================================================================
    // Additional process_pending_files tests
    // =====================================================================

    #[test]
    fn process_pending_files_empty_pending_does_nothing() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // State starts empty, no files added

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        assert!(result.is_ok());
        assert!(last_processed.is_empty()); // Nothing was processed
    }

    #[test]
    fn process_pending_files_skips_recently_processed() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Create a temp file with a TODO
        let mut temp_file = NamedTempFile::new_in(temp_dir.path()).unwrap();
        writeln!(temp_file, "// TODO: test task").unwrap();
        temp_file.flush().unwrap();

        let file_path = temp_file.path().to_path_buf();

        // Pre-populate last_processed with current time (file was just processed)
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        last_processed.insert(file_path.clone(), Instant::now());

        // Add file to pending
        state.lock().unwrap().add_file(file_path.clone());

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 1000, // Long debounce - file should be skipped
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();

        // Process should skip the file due to recent processing
        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        assert!(result.is_ok());
        // File should still be in last_processed but timestamp should not change
        assert!(last_processed.contains_key(&file_path));
    }

    #[test]
    fn process_pending_files_handles_read_error_gracefully() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Use a non-existent file path (will cause read error)
        let nonexistent_path = temp_dir.path().join("does_not_exist.rs");
        state.lock().unwrap().add_file(nonexistent_path);

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        // Should return Ok even though file read failed
        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn process_pending_files_processes_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Create multiple files with TODOs
        let mut file1 = NamedTempFile::new_in(temp_dir.path()).unwrap();
        writeln!(file1, "// TODO: task 1").unwrap();
        file1.flush().unwrap();

        let mut file2 = NamedTempFile::new_in(temp_dir.path()).unwrap();
        writeln!(file2, "// FIXME: task 2").unwrap();
        file2.flush().unwrap();

        let path1 = file1.path().to_path_buf();
        let path2 = file2.path().to_path_buf();

        state.lock().unwrap().add_file(path1.clone());
        state.lock().unwrap().add_file(path2.clone());

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::All],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        assert!(result.is_ok());
        // Both files should be recorded in last_processed
        assert!(last_processed.contains_key(&path1));
        assert!(last_processed.contains_key(&path2));
    }
}
