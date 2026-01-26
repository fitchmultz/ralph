#![deny(unsafe_op_in_unsafe_fn)]

//! Ralph library surface.
//!
//! The CLI entrypoint is `src/main.rs`. This crate exposes internal modules so
//! integration/stress tests can exercise real behavior without mocks.

// --- Core --------------------------------------------------------------------

pub mod agent;
pub mod config;
pub mod contracts;
pub mod queue;
pub mod reports;

// --- Commands ----------------------------------------------------------------

pub mod cli;
pub mod commands;

// --- UI ----------------------------------------------------------------------

pub mod tui;

// --- Utils -------------------------------------------------------------------

pub mod fsutil;
pub mod gitutil;
pub mod outpututil;
pub mod promptflow;
pub mod prompts;
pub mod redaction;
pub mod runner;
pub mod runutil;
pub mod timeutil;

// --- Internal ----------------------------------------------------------------

// Not used by the binary/tests directly; keep crate-private.
pub(crate) mod completions;
pub(crate) mod debuglog;

// Internal prompt composition helpers.
mod prompts_internal;

#[cfg(test)]
mod runutil_tests;
