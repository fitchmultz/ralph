#![deny(unsafe_op_in_unsafe_fn)]

//! Ralph library surface.
//!
//! Responsibilities:
//! - Expose internal modules for tests and CLI integration.
//! - Provide a stable entrypoint for crate-wide utilities.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli`).
//! - Runner execution or queue persistence details.
//!
//! Invariants/assumptions:
//! - Modules remain internal-first; public exports are intentional.

// --- Core --------------------------------------------------------------------

pub mod agent;
pub mod config;
pub mod contracts;
pub mod queue;
pub(crate) mod reports;

// --- Commands ----------------------------------------------------------------

pub mod cli;
pub mod commands;
pub mod migration;
pub mod sanity;

// --- UI ----------------------------------------------------------------------

pub mod tui;

// --- Utils -------------------------------------------------------------------

pub mod celebrations;
pub mod fsutil;
pub mod git;
// DEPRECATED: gitutil is deprecated and will be removed. Use `git` instead.
pub(crate) mod gitutil;
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
pub(crate) mod completions;
pub(crate) mod debuglog;

// Internal prompt composition helpers.
mod prompts_internal;

#[cfg(test)]
mod runutil_tests;

#[cfg(test)]
pub(crate) mod testsupport;
