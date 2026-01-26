#![deny(unsafe_op_in_unsafe_fn)]

//! Ralph library surface.
//!
//! The CLI entrypoint is `src/main.rs`. This crate exposes internal modules so
//! integration/stress tests can exercise real behavior without mocks.

pub mod agent;
pub mod cli;
pub mod commands;
pub mod completions;
pub mod config;
pub mod contracts;
pub mod debuglog;
pub mod fsutil;
pub mod gitutil;
pub mod outpututil;
pub mod promptflow;
pub mod prompts;
mod prompts_internal;
pub mod queue;
pub mod redaction;
pub mod reports;
pub mod runner;
pub mod runutil;
#[cfg(test)]
mod runutil_tests;
pub mod timeutil;
pub mod tui;
