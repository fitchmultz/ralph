//! Integration tests for shared integration-test helpers in `test_support`.
//!
//! Purpose:
//! - Integration tests for shared integration-test helpers in `test_support`.
//!
//! Responsibilities:
//! - Verify helper behavior that affects cross-suite determinism.
//! - Assert `git_init` creates a usable initial commit in isolated repos.
//! - Assert wait helpers cover success and timeout cases deterministically.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled:
//! - End-to-end CLI behavior (covered by command-specific integration tests).
//! - Filesystem lock behavior (covered by lock-specific tests).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Git is available on PATH.
//! - Tests use isolated temp directories and do not mutate repository state.

mod test_support;

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn git_init_creates_initial_commit_head() {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path()).expect("git init helper should succeed");

    let output = Command::new("git")
        .current_dir(dir.path())
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .expect("run git rev-parse");
    assert!(
        output.status.success(),
        "expected git HEAD after git_init; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wait_until_returns_true_when_condition_becomes_true() {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = Arc::new(test_support::Signal::new());
    let writer_flag = Arc::clone(&flag);
    let writer_started = Arc::clone(&started);
    let writer = thread::spawn(move || {
        writer_started.notify(());
        writer_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    assert_eq!(started.wait(Duration::from_secs(1)), Some(()));

    let ok = test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
        flag.load(std::sync::atomic::Ordering::SeqCst)
    });
    writer.join().expect("join writer thread");
    assert!(ok, "wait_until should succeed once condition flips true");
}

#[test]
fn wait_until_returns_false_on_timeout() {
    let ok = test_support::wait_until(
        Duration::from_millis(150),
        Duration::from_millis(25),
        || false,
    );
    assert!(
        !ok,
        "wait_until should return false when condition never becomes true"
    );
}

#[test]
fn wait_for_mutex_value_returns_some_when_populated() {
    let shared = Arc::new(Mutex::new(None::<u8>));
    let started = Arc::new(test_support::Signal::new());
    let writer_shared = Arc::clone(&shared);
    let writer_started = Arc::clone(&started);
    let writer = thread::spawn(move || {
        writer_started.notify(());
        *writer_shared.lock().expect("lock shared value") = Some(7);
    });

    assert_eq!(started.wait(Duration::from_secs(1)), Some(()));

    let value = test_support::wait_for_mutex_value(
        &shared,
        Duration::from_secs(5),
        Duration::from_millis(25),
    );
    writer.join().expect("join writer thread");
    assert_eq!(value, Some(7));
}

#[test]
fn wait_for_mutex_value_returns_none_on_timeout() {
    let shared = Arc::new(Mutex::new(None::<u8>));
    let value = test_support::wait_for_mutex_value(
        &shared,
        Duration::from_millis(150),
        Duration::from_millis(25),
    );
    assert!(
        value.is_none(),
        "wait_for_mutex_value should return None when value stays empty"
    );
}
