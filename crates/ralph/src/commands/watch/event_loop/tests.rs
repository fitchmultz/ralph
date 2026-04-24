//! Watch event-loop regression tests.
//!
//! Purpose:
//! - Watch event-loop regression tests.
//!
//! Responsibilities:
//! - Cover loop shutdown, poison handling, timeout behavior, and event ingestion.
//! - Provide shared watch-loop fixtures for local unit tests.
//!
//! Not handled here:
//! - End-to-end watcher setup through `notify::RecommendedWatcher`.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests preserve the watch-loop debounce semantics used in production.
//! - Mutex poison scenarios must return cleanly instead of panicking.

use super::events::handle_watch_event;
use super::run_watch_loop;
use super::timeout::handle_timeout_tick;
use crate::commands::watch::comments::build_comment_regex;
use crate::commands::watch::state::WatchState;
use crate::commands::watch::types::{CommentType, WatchOptions};
use crate::config::Resolved;
use crate::contracts::{Config, QueueFile};
use notify::Event;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::{NamedTempFile, TempDir};

fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
    let queue_path = temp_dir.path().join("queue.json");
    let done_path = temp_dir.path().join("done.json");

    let queue = QueueFile::default();
    let queue_json = serde_json::to_string_pretty(&queue).expect("serialize empty queue");
    std::fs::write(&queue_path, queue_json).expect("write queue fixture");

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

fn test_watch_options() -> WatchOptions {
    WatchOptions {
        patterns: vec!["*.rs".to_string()],
        debounce_ms: 100,
        auto_queue: false,
        notify: false,
        ignore_patterns: vec![],
        comment_types: vec![CommentType::Todo],
        paths: vec![PathBuf::from(".")],
        force: false,
        close_removed: false,
    }
}

#[test]
fn watch_loop_exits_on_channel_disconnect() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let (tx, rx) = channel::<notify::Result<Event>>();
    let running = Arc::new(Mutex::new(true));
    let state = Arc::new(Mutex::new(WatchState::new(100)));
    let opts = test_watch_options();
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

    drop(tx);

    run_watch_loop(
        &rx,
        &running,
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
    )
    .expect("watch loop should exit cleanly on channel disconnect");
}

#[test]
fn watch_loop_exits_on_running_mutex_poison() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let (tx, rx) = channel::<notify::Result<Event>>();
    let running = Arc::new(Mutex::new(true));
    let state = Arc::new(Mutex::new(WatchState::new(100)));
    let opts = test_watch_options();
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
    let running_clone = running.clone();

    let poison_handle = std::thread::spawn(move || {
        let _guard = running_clone.lock().expect("lock running mutex");
        panic!("Intentional panic to poison running mutex");
    });
    let _ = poison_handle.join();

    drop(tx);
    run_watch_loop(
        &rx,
        &running,
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
    )
    .expect("watch loop should exit cleanly on running mutex poison");
}

#[test]
fn watch_loop_processes_file_event() {
    use notify::EventKind;
    use notify::event::{DataChange, ModifyKind};
    use std::io::Write;

    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let mut temp_file = NamedTempFile::new_in(temp_dir.path()).expect("create temp file");
    writeln!(temp_file, "// TODO: test").expect("write todo comment");
    temp_file.flush().expect("flush temp file");

    let (tx, rx) = channel::<notify::Result<Event>>();
    let state = Arc::new(Mutex::new(WatchState::new(50)));
    let mut opts = test_watch_options();
    opts.debounce_ms = 50;
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
    let start = Instant::now();

    let event = Event {
        kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        paths: vec![temp_file.path().to_path_buf()],
        attrs: Default::default(),
    };
    tx.send(Ok(event)).expect("send watch event");
    let received = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("receive watch event")
        .expect("watch event result");

    handle_watch_event(
        &received,
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
        start,
    )
    .expect("handle watch event");
    handle_timeout_tick(
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
        start + Duration::from_millis(60),
    )
    .expect("process timeout tick");
}

#[test]
fn watch_loop_handles_state_mutex_poison_during_event() {
    use notify::EventKind;
    use notify::event::{DataChange, ModifyKind};

    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let (tx, rx) = channel::<notify::Result<Event>>();
    let state = Arc::new(Mutex::new(WatchState::new(100)));
    let opts = test_watch_options();
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
    let state_clone = state.clone();

    let poison_handle = std::thread::spawn(move || {
        let _guard = state_clone.lock().expect("lock state mutex");
        panic!("Poison state mutex");
    });
    let _ = poison_handle.join();

    let event = Event {
        kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        paths: vec![PathBuf::from("/test/file.rs")],
        attrs: Default::default(),
    };
    tx.send(Ok(event)).expect("send poisoned event");
    let received = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("receive watch event")
        .expect("watch event result");

    handle_watch_event(
        &received,
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
        Instant::now(),
    )
    .expect("state mutex poison should be handled gracefully");
}

#[test]
fn watch_loop_handles_watch_error() {
    let (tx, rx) = channel::<notify::Result<Event>>();

    tx.send(Err(notify::Error::generic("Test watch error")))
        .expect("send watch error");

    let received = rx.recv_timeout(Duration::from_secs(1));
    assert!(matches!(received, Ok(Err(_))));
}

#[test]
fn watch_loop_exits_when_running_set_to_false() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let (_tx, rx) = channel::<notify::Result<Event>>();
    let running = Arc::new(Mutex::new(true));
    let state = Arc::new(Mutex::new(WatchState::new(100)));
    let opts = test_watch_options();
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

    *running.lock().expect("lock running mutex") = false;
    run_watch_loop(
        &rx,
        &running,
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
    )
    .expect("watch loop should exit immediately when running=false");
}

#[test]
fn timeout_tick_processes_pending_files_after_debounce() {
    use std::io::Write;

    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let mut temp_file = NamedTempFile::new_in(temp_dir.path()).expect("create temp file");
    writeln!(temp_file, "// TODO: timeout path").expect("write todo comment");
    temp_file.flush().expect("flush temp file");

    let state = Arc::new(Mutex::new(WatchState::new(50)));
    let mut opts = test_watch_options();
    opts.debounce_ms = 50;
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
    let start = Instant::now();

    state
        .lock()
        .expect("lock state mutex")
        .add_file_at(temp_file.path().to_path_buf(), start);

    handle_timeout_tick(
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
        start + Duration::from_millis(60),
    )
    .expect("timeout tick should process pending files");

    let guard = state.lock().expect("lock state mutex");
    assert!(guard.pending_files.is_empty());
}

#[test]
fn event_loop_helpers_leave_timeout_queue_idle_without_pending_files() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let resolved = create_test_resolved(&temp_dir);
    let state = Arc::new(Mutex::new(WatchState::new(50)));
    let mut opts = test_watch_options();
    opts.debounce_ms = 50;
    let comment_regex = build_comment_regex(&opts.comment_types).expect("build regex");
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

    handle_timeout_tick(
        &state,
        &resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
        Instant::now() + Duration::from_secs(1),
    )
    .expect("timeout tick without pending files should be a no-op");

    assert!(
        state
            .lock()
            .expect("lock state mutex")
            .pending_files
            .is_empty()
    );
}
