//! Purpose: thin integration-test hub for `ralph::fsutil` contract coverage.
//!
//! Responsibilities:
//! - Re-export shared imports and suite-local synchronization used by fsutil integration tests.
//! - Delegate temp cleanup, atomic write, and tilde expansion coverage to focused companion modules.
//!
//! Scope:
//! - Integration tests for the public `ralph::fsutil` facade only.
//!
//! Usage:
//! - Companion modules use `use super::*;` to access shared imports and `ENV_LOCK`.
//!
//! Invariants/Assumptions:
//! - This root module contains no test functions.
//! - Test names, assertions, and environment-lock semantics remain unchanged from the pre-split suite.

pub(crate) use ralph::fsutil;
pub(crate) use serial_test::serial;
pub(crate) use std::env;
pub(crate) use std::fs;
pub(crate) use std::path::PathBuf;
pub(crate) use std::sync::Mutex;
pub(crate) use std::thread;
pub(crate) use std::time::Duration;
pub(crate) use tempfile::TempDir;

pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

#[path = "fsutil_test/atomic_writes.rs"]
mod atomic_writes;
#[path = "fsutil_test/temp_cleanup.rs"]
mod temp_cleanup;
#[path = "fsutil_test/tilde_expansion.rs"]
mod tilde_expansion;
