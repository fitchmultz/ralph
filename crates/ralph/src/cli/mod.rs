//! Ralph CLI facade.
//!
//! Responsibilities:
//! - Expose the top-level Clap surface for the `ralph` binary.
//! - Re-export shared CLI helpers used across subcommand modules.
//! - Keep the root CLI module thin and navigation-friendly.
//!
//! Not handled here:
//! - Command execution logic beyond delegated helper entrypoints.
//! - Queue persistence or runner execution internals.
//! - Large parse-regression suites beyond delegated test modules.
//!
//! Invariants/assumptions:
//! - Top-level clap types remain stable re-exports for the rest of the crate.
//! - Shared queue/list helper behavior stays centralized in `helpers.rs`.

mod args;
mod helpers;

pub mod app;
pub mod app_parity;
pub mod cleanup;
pub mod color;
pub mod completions;
pub mod config;
pub mod context;
pub mod daemon;
pub mod doctor;
pub mod init;
pub mod machine;
pub mod migrate;
pub mod plugin;
pub mod prd;
pub mod productivity;
pub mod prompt;
pub mod queue;
pub mod run;
pub mod runner;
pub mod scan;
pub mod task;
pub mod tutorial;
pub mod undo;
pub mod version;
pub mod watch;
pub mod webhook;

pub use args::{Cli, CliSpecArgs, CliSpecFormatArg, Command};
pub use color::ColorArg;
pub use helpers::{handle_cli_spec, handle_help_all};
pub(crate) use helpers::{load_and_validate_queues_read_only, resolve_list_limit};

#[cfg(test)]
mod tests;
