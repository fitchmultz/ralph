//! Lock test helpers shared across integration tests.
//!
//! Responsibilities:
//! - Provide short-lived process utilities for lock-related tests.
//!
//! Not handled here:
//! - Lock acquisition logic or filesystem operations.
//! - Assertions about lock behavior.
//!
//! Invariants/assumptions:
//! - Spawns the current test binary with `--help` and waits for exit.
//! - Returned PID should be treated as stale immediately after the process exits.

use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn spawn_exited_pid() -> u32 {
    let mut child = Command::new(current_exe())
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn helper process");
    let pid = child.id();
    let _ = child.wait();
    pid
}

fn current_exe() -> PathBuf {
    std::env::current_exe().expect("resolve current test executable path")
}
