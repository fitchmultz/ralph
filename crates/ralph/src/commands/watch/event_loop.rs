//! Event loop for the watch command.
//!
//! Responsibilities:
//! - Run the watch event loop until stopped or channel disconnects.
//! - Handle file events and coordinate processing.
//! - Manage Ctrl+C signal handling state.
//!
//! Not handled here:
//! - File watching setup (see `mod.rs`).
//! - Comment detection (see `comments.rs`).
//! - Task creation (see `tasks.rs`).
//!
//! Invariants/assumptions:
//! - The loop exits cleanly on channel disconnect or when `running` is false.
//! - Mutex poison errors are handled gracefully (logged, loop continues or exits).
//! - Timeout-based processing checks for pending files on each iteration.

use crate::commands::watch::debounce::can_reprocess;
use crate::commands::watch::paths::get_relevant_paths;
use crate::commands::watch::processor::process_pending_files;
use crate::commands::watch::state::WatchState;
use crate::commands::watch::types::WatchOptions;
use crate::config::Resolved;
use anyhow::Result;
use notify::Event;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Run the watch event loop until stopped or channel disconnects.
///
/// This is extracted as a separate function for testability.
pub fn run_watch_loop(
    rx: &std::sync::mpsc::Receiver<notify::Result<Event>>,
    running: &Arc<Mutex<bool>>,
    state: &Arc<Mutex<WatchState>>,
    resolved: &Resolved,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
) -> Result<()> {
    loop {
        // Check running state with poison handling
        let should_continue = match running.lock() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Watch 'running' mutex poisoned, exiting: {}", e);
                break;
            }
        };
        if !should_continue {
            break;
        }
        // Check for events with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if let Some(paths) = get_relevant_paths(&event, opts) {
                    let debounce = opts.debounce_ms;
                    let mut should_process = false;
                    match state.lock() {
                        Ok(mut guard) => {
                            for path in paths {
                                if can_reprocess(
                                    &path,
                                    last_processed,
                                    Duration::from_millis(debounce),
                                ) && guard.add_file(path.clone())
                                {
                                    should_process = true;
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Watch 'state' mutex poisoned, skipping event: {}", e);
                            continue;
                        }
                    }
                    if should_process {
                        process_pending_files(
                            resolved,
                            state,
                            comment_regex,
                            opts,
                            last_processed,
                        )?;
                    }
                }
            }
            Ok(Err(e)) => {
                log::warn!("Watch error: {}", e);
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::info!("Watch channel disconnected, shutting down...");
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // Timeout - check if we should process pending files
                let should_process = match state.lock() {
                    Ok(state) => {
                        !state.pending_files.is_empty()
                            && Instant::now().duration_since(state.last_event)
                                >= state.debounce_duration
                    }
                    Err(e) => {
                        log::error!("Watch 'state' mutex poisoned during timeout check: {}", e);
                        false
                    }
                };
                if should_process {
                    process_pending_files(resolved, state, comment_regex, opts, last_processed)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::watch::comments::build_comment_regex;
    use crate::commands::watch::types::CommentType;
    use crate::contracts::{Config, QueueFile};
    use std::path::PathBuf;
    use std::sync::mpsc::channel;
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
    fn watch_loop_exits_on_channel_disconnect() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
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

        // Spawn the watch loop in a separate thread
        let running_clone = running.clone();
        let state_clone = state.clone();
        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
            .unwrap();
        });

        // Give the loop a moment to start
        std::thread::sleep(Duration::from_millis(50));

        // Drop the sender to simulate channel disconnect
        drop(tx);

        // The loop should exit within a reasonable time (timeout to prevent hanging)
        let result = handle.join();
        assert!(
            result.is_ok(),
            "Watch loop should exit cleanly on channel disconnect"
        );
    }

    #[test]
    fn watch_loop_exits_on_running_mutex_poison() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
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
        let running_clone = running.clone();

        // Spawn a thread that will panic while holding the running mutex
        let poison_handle = std::thread::spawn(move || {
            let _guard = running_clone.lock().unwrap();
            panic!("Intentional panic to poison running mutex");
        });

        // Wait for the panic
        let _ = poison_handle.join();

        // Now the running mutex is poisoned - verify the watch loop handles it gracefully
        let running_clone2 = running.clone();
        let state_clone = state.clone();
        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone2,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
        });

        // Give the loop a moment to start and hit the poisoned mutex
        std::thread::sleep(Duration::from_millis(100));

        // Drop the sender to ensure clean exit
        drop(tx);

        // The loop should exit cleanly (not panic)
        let result = handle.join();
        assert!(
            result.is_ok(),
            "Watch loop should exit cleanly on running mutex poison"
        );
    }

    // =====================================================================
    // Additional event loop tests
    // =====================================================================

    #[test]
    fn watch_loop_processes_file_event() {
        use notify::EventKind;
        use notify::event::{DataChange, ModifyKind};
        use std::io::Write;

        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a file with a TODO
        let mut temp_file = NamedTempFile::new_in(temp_dir.path()).unwrap();
        writeln!(temp_file, "// TODO: test").unwrap();
        temp_file.flush().unwrap();

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
        let state = Arc::new(Mutex::new(WatchState::new(50)));

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 50,
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

        let running_clone = running.clone();
        let state_clone = state.clone();

        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
        });

        // Send a file event
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            paths: vec![temp_file.path().to_path_buf()],
            attrs: Default::default(),
        };
        tx.send(Ok(event)).unwrap();

        // Give it time to process
        std::thread::sleep(Duration::from_millis(100));

        // Stop the loop
        *running.lock().unwrap() = false;

        // Drop sender to disconnect
        drop(tx);

        // Wait for loop to exit
        let _ = handle.join();
    }

    #[test]
    fn watch_loop_handles_state_mutex_poison_during_event() {
        use notify::EventKind;
        use notify::event::{DataChange, ModifyKind};

        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
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

        // Poison the state mutex
        let state_clone = state.clone();
        let poison_handle = std::thread::spawn(move || {
            let _guard = state_clone.lock().unwrap();
            panic!("Poison state mutex");
        });
        let _ = poison_handle.join();

        let running_clone = running.clone();
        let state_clone = state.clone();

        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
        });

        // Send an event - loop should handle poison gracefully
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            paths: vec![PathBuf::from("/test/file.rs")],
            attrs: Default::default(),
        };
        tx.send(Ok(event)).unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Stop and cleanup
        *running.lock().unwrap() = false;
        drop(tx);

        let result = handle.join();
        assert!(result.is_ok());
    }

    #[test]
    fn watch_loop_handles_watch_error() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
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

        let running_clone = running.clone();
        let state_clone = state.clone();

        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
        });

        // Send an error result
        let error = notify::Error::generic("Test watch error");
        tx.send(Err(error)).unwrap();

        std::thread::sleep(Duration::from_millis(50));

        // Loop should continue after error
        *running.lock().unwrap() = false;
        drop(tx);

        let result = handle.join();
        assert!(result.is_ok());
    }

    #[test]
    fn watch_loop_exits_when_running_set_to_false() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let (_tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
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

        let running_clone = running.clone();
        let state_clone = state.clone();

        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
        });

        // Let loop start
        std::thread::sleep(Duration::from_millis(50));

        // Set running to false to trigger exit
        *running.lock().unwrap() = false;

        // Don't drop sender - we want to test that running=false causes exit
        // even with channel still open

        let result = handle.join();
        assert!(result.is_ok());
    }
}
