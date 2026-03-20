//! Purpose: subprocess self-exec entrypoint for queue-lock contention tests.
//!
//! Responsibilities:
//! - Provide the lock-holder logic invoked by the root-level `lock_holder_process` test wrapper.
//! - Resolve the current test executable path for self-spawn helpers.
//!
//! Scope:
//! - Lock-holder subprocess behavior only.
//!
//! Usage:
//! - The suite hub keeps the `#[test] fn lock_holder_process()` wrapper to preserve the exact libtest filter.
//! - Suite-local helpers call `current_exe()` when spawning the subprocess.
//!
//! Invariants/Assumptions:
//! - `RALPH_TEST_LOCK_HOLD=1` gates the helper so normal suite runs skip it.
//! - Readiness is signaled by printing `LOCK_HELD`.
//! - The subprocess exits only after parent stdin closes.

use super::*;
use std::io::{Read, Write};

pub(super) fn current_exe() -> PathBuf {
    std::env::current_exe().expect("resolve current test executable path")
}

pub(super) fn lock_holder_process() -> Result<()> {
    if std::env::var("RALPH_TEST_LOCK_HOLD").ok().as_deref() != Some("1") {
        return Ok(());
    }

    let repo_root = std::env::var("RALPH_TEST_REPO_ROOT").context("read RALPH_TEST_REPO_ROOT")?;
    let repo_root = PathBuf::from(repo_root);

    std::fs::create_dir_all(repo_root.join(".ralph")).context("create .ralph dir")?;

    let label =
        std::env::var("RALPH_TEST_LOCK_LABEL").unwrap_or_else(|_| "lock holder".to_string());

    let _lock = queue::acquire_queue_lock(&repo_root, &label, false)?;
    println!("LOCK_HELD");
    let _ = std::io::stdout().flush();

    let mut stdin = std::io::stdin();
    let mut buf = [0u8; 1];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    Ok(())
}
