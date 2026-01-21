#![deny(unsafe_op_in_unsafe_fn)]

//! Ralph library surface.
//!
//! The CLI entrypoint is `src/main.rs`. This crate exposes internal modules so
//! integration/stress tests can exercise real behavior without mocks.

pub mod agent;
pub mod cli;
pub mod completions;
pub mod config;
pub mod contracts;
pub mod doctor_cmd;
pub mod fsutil;
pub mod gitutil;
pub mod init_cmd;
pub mod outpututil;
pub mod prompt_cmd;
pub mod promptflow;
pub mod prompts;
pub mod queue;
pub mod redaction;
pub mod reports;
pub mod run_cmd;
pub mod runner;
pub mod runutil;
pub mod scan_cmd;
pub mod task_cmd;
pub mod timeutil;
pub mod tui;
