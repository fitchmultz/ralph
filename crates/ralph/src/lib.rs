//! Ralph library surface.
//!
//! Purpose:
//! - Ralph library surface.
//!
//! Responsibilities:
//! - Expose internal modules for tests and CLI integration.
//! - Provide a stable entrypoint for crate-wide utilities.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli`).
//! - Runner execution or queue persistence details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Modules remain internal-first; public exports are intentional.

#![deny(unsafe_op_in_unsafe_fn)]

// --- Core --------------------------------------------------------------------

pub mod agent;
pub mod config;
pub mod constants;
pub mod contracts;
pub mod queue;
pub(crate) mod reports;

// --- Commands ----------------------------------------------------------------

pub mod cli;
#[path = "cli/spec.rs"]
pub mod cli_spec;
pub mod commands;
pub mod migration;
pub mod plugins;
pub mod sanity;
pub mod undo;

// --- Utils -------------------------------------------------------------------

pub mod celebrations;
pub mod error_messages;
pub mod eta_calculator;
pub mod execution_history;
pub mod fsutil;
pub mod git;
pub mod jsonc;
pub mod lock;
pub mod notification;
pub mod webhook;
// Internal-only output modules
pub(crate) mod output;
pub(crate) mod outpututil;
pub mod productivity;
pub mod progress;
pub mod promptflow;
pub mod prompts;
pub mod redaction;
pub mod runner;
pub mod runutil;
pub mod session;
pub mod signal;
pub mod template;
pub mod timeutil;

// --- Internal ----------------------------------------------------------------

// Not used by the binary/tests directly; keep crate-private.
pub(crate) mod debuglog;

// Internal prompt composition helpers.
mod prompts_internal;

#[cfg(test)]
pub(crate) mod testsupport;

/// Test synchronization utilities for managing global state across tests.
///
/// This module provides synchronization primitives to prevent test flakiness
/// caused by global state interference between parallel tests.
#[cfg(test)]
pub(crate) mod test_sync {
    use std::sync::{Mutex, OnceLock};

    /// Global mutex to synchronize tests that modify the Ctrl+C interrupt flag.
    ///
    /// Tests that set the interrupt flag should acquire this mutex.
    /// Tests that use the runner should reset the flag before starting.
    pub static INTERRUPT_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    /// Reset the global Ctrl+C interrupted flag to false.
    ///
    /// This should be called by tests that use the runner to ensure they
    /// don't fail due to stale interrupt flags from other tests.
    pub fn reset_ctrlc_interrupt_flag() {
        use std::sync::atomic::Ordering;
        if let Ok(ctrlc) = crate::runner::ctrlc_state() {
            ctrlc.interrupted.store(false, Ordering::SeqCst);
        }
    }
}
