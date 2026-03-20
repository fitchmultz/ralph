//! Purpose: thin integration-test hub for queue-lock contention coverage.
//!
//! Responsibilities:
//! - Re-export shared imports and suite-local modules for lock contention integration tests.
//! - Preserve the exact top-level `lock_holder_process` subprocess entrypoint required by self-exec contention tests.
//! - Delegate contention and run-loop abort scenarios to focused companion modules.
//!
//! Scope:
//! - Test-suite wiring plus the thin lock-holder wrapper only.
//!
//! Usage:
//! - Companion modules use `use super::*;` for shared imports and helpers.
//! - Contention tests spawn this binary with `--exact lock_holder_process --nocapture`.
//!
//! Invariants/Assumptions:
//! - The root-level `lock_holder_process` test name must remain unchanged so subprocess filtering continues to work.
//! - Assertions, env vars, serial semantics, and coverage stay identical to the pre-split suite.

mod test_support;

#[path = "lock_contention_integration_test/lock_holder.rs"]
mod lock_holder;
#[path = "lock_contention_integration_test/support.rs"]
mod support;

use anyhow::{Context, Result};
use ralph::{lock, queue};
use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

#[test]
#[serial]
fn lock_holder_process() -> Result<()> {
    lock_holder::lock_holder_process()
}

#[path = "lock_contention_integration_test/contention.rs"]
mod contention;
#[path = "lock_contention_integration_test/run_loop_abort.rs"]
mod run_loop_abort;
