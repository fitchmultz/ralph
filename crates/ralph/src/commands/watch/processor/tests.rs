//! Watch processor regression coverage.
//!
//! Responsibilities:
//! - Verify pending-file processing handles debounce, missing files, and read failures.
//! - Cover stale-history cleanup and state-poison resilience.
//!
//! Does not handle:
//! - File watcher event-loop orchestration.
//! - Low-level comment parsing beyond the processor contract.
//!
//! Assumptions/invariants:
//! - Missing files count as processed for reconciliation.
//! - Existing-path read failures should not be recorded as successful processing.
//! - Old `last_processed` entries should be pruned during normal processing.

use super::*;

use crate::commands::watch::comments::build_comment_regex;
use crate::commands::watch::types::CommentType;
use crate::contracts::{Config, QueueFile};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::{NamedTempFile, TempDir};

fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    let queue_json = serde_json::to_string_pretty(&QueueFile::default()).unwrap();
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

fn watch_opts(
    comment_types: Vec<CommentType>,
    debounce_ms: u64,
    close_removed: bool,
) -> WatchOptions {
    WatchOptions {
        patterns: vec!["*.rs".to_string()],
        debounce_ms,
        auto_queue: false,
        notify: false,
        ignore_patterns: vec![],
        comment_types,
        paths: vec![PathBuf::from(".")],
        force: false,
        close_removed,
    }
}

#[test]
fn process_pending_files_handles_state_mutex_poison() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let opts = watch_opts(vec![CommentType::Todo], 100, false);
    let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

    let state_clone = state.clone();
    let poison_handle = std::thread::spawn(move || {
        let _guard = state_clone.lock().unwrap();
        panic!("Intentional panic to poison state mutex");
    });
    let _ = poison_handle.join();

    let result = process_pending_files(
        &resolved,
        &state,
        &comment_regex,
        &opts,
        &mut last_processed,
    );

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

    let mut temp_file = NamedTempFile::new_in(temp_dir.path()).unwrap();
    writeln!(temp_file, "// TODO: test task").unwrap();
    temp_file.flush().unwrap();

    let file_path = temp_file.path().to_path_buf();
    state.lock().unwrap().add_file(file_path.clone());

    let opts = watch_opts(vec![CommentType::Todo], 100, false);
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
    assert!(last_processed.contains_key(&file_path));
    assert!(state.lock().unwrap().pending_files.is_empty());
}

#[test]
fn process_pending_files_empty_pending_does_nothing() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let opts = watch_opts(vec![CommentType::Todo], 100, false);
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
    assert!(last_processed.is_empty());
}

#[test]
fn process_pending_files_skips_recently_processed() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let mut temp_file = NamedTempFile::new_in(temp_dir.path()).unwrap();
    writeln!(temp_file, "// TODO: test task").unwrap();
    temp_file.flush().unwrap();

    let file_path = temp_file.path().to_path_buf();
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
    last_processed.insert(file_path.clone(), Instant::now());
    state.lock().unwrap().add_file(file_path.clone());

    let opts = watch_opts(vec![CommentType::Todo], 1000, false);
    let comment_regex = build_comment_regex(&opts.comment_types).unwrap();

    let result = process_pending_files(
        &resolved,
        &state,
        &comment_regex,
        &opts,
        &mut last_processed,
    );

    assert!(result.is_ok());
    assert!(last_processed.contains_key(&file_path));
}

#[test]
fn process_pending_files_handles_read_error_gracefully() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let nonexistent_path = temp_dir.path().join("does_not_exist.rs");
    state.lock().unwrap().add_file(nonexistent_path);

    let opts = watch_opts(vec![CommentType::Todo], 100, false);
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
}

#[test]
fn process_pending_files_records_missing_file_for_reconciliation() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let missing_path = temp_dir.path().join("does_not_exist.rs");
    state.lock().unwrap().add_file(missing_path.clone());

    let opts = watch_opts(vec![CommentType::Todo], 100, true);
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
    assert!(last_processed.contains_key(&missing_path));
}

#[test]
fn process_pending_files_processes_multiple_files() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

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

    let opts = watch_opts(vec![CommentType::All], 100, false);
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
    assert!(last_processed.contains_key(&path1));
    assert!(last_processed.contains_key(&path2));
}

#[test]
fn process_pending_files_does_not_record_existing_path_on_detect_error() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let unreadable_path = temp_dir.path().join("directory.rs");
    std::fs::create_dir_all(&unreadable_path).unwrap();
    state.lock().unwrap().add_file(unreadable_path.clone());

    let opts = watch_opts(vec![CommentType::Todo], 100, false);
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
    assert!(
        !last_processed.contains_key(&unreadable_path),
        "existing-path read failures should not be recorded as processed"
    );
}

#[test]
fn process_pending_files_prunes_stale_history_entries_after_processing() {
    let temp_dir = TempDir::new().unwrap();
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(100)));

    let mut temp_file = NamedTempFile::new_in(temp_dir.path()).unwrap();
    writeln!(temp_file, "// TODO: fresh task").unwrap();
    temp_file.flush().unwrap();

    let file_path = temp_file.path().to_path_buf();
    let stale_path = temp_dir.path().join("stale.rs");
    state.lock().unwrap().add_file(file_path.clone());

    let opts = watch_opts(vec![CommentType::Todo], 100, false);
    let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
    last_processed.insert(
        stale_path.clone(),
        Instant::now() - Duration::from_millis(1_500),
    );

    let result = process_pending_files(
        &resolved,
        &state,
        &comment_regex,
        &opts,
        &mut last_processed,
    );

    assert!(result.is_ok());
    assert!(last_processed.contains_key(&file_path));
    assert!(
        !last_processed.contains_key(&stale_path),
        "stale history entries should be pruned during processing"
    );
}
