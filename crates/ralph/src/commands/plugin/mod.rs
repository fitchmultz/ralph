//! Plugin command facade.
//!
//! Purpose:
//! - Plugin command facade.
//!
//! Responsibilities:
//! - Route plugin subcommands to focused handlers.
//! - Keep plugin command entrypoints thin and behavior-grouped.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli::plugin`).
//! - Plugin discovery/registry internals (see `crate::plugins`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Subcommand behavior stays delegated to focused helper modules.
//! - This facade preserves the existing plugin CLI contracts.

mod common;
mod init;
mod install;
mod list;
mod templates;
mod validate;

use anyhow::Result;

use crate::cli::plugin::{PluginArgs, PluginCommand};
use crate::config::Resolved;

pub fn run(args: &PluginArgs, resolved: &Resolved) -> Result<()> {
    match &args.command {
        PluginCommand::List { json } => list::run_list(resolved, *json),
        PluginCommand::Validate { id } => validate::run_validate(resolved, id.as_deref()),
        PluginCommand::Install { source, scope } => install::run_install(resolved, source, *scope),
        PluginCommand::Uninstall { id, scope } => install::run_uninstall(resolved, id, *scope),
        PluginCommand::Init(init_args) => init::run_init(resolved, init_args),
    }
}
