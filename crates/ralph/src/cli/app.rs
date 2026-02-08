//! `ralph app ...` command group for macOS GUI integration.
//!
//! Responsibilities:
//! - Define clap structures for app-related commands (currently `open`).
//! - Route app subcommands to the command implementation layer (`crate::commands::app`).
//!
//! Not handled here:
//! - Building or installing the SwiftUI app bundle (see `apps/RalphMac/`).
//! - Any queue/runner logic (Ralph remains CLI-first; the GUI shells out to the CLI).
//!
//! Invariants/assumptions:
//! - `ralph app open` is macOS-only; non-macOS platforms return a clear error.

use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::commands::app as app_cmd;

pub fn handle_app(cmd: AppCommand) -> Result<()> {
    match cmd {
        AppCommand::Open(args) => app_cmd::open(args),
    }
}

#[derive(Args)]
pub struct AppArgs {
    #[command(subcommand)]
    pub command: AppCommand,
}

#[derive(Subcommand)]
pub enum AppCommand {
    /// Open the macOS Ralph app.
    #[command(
        about = "Open the macOS Ralph app",
        after_long_help = "Examples:\n  ralph app open\n  ralph app open --bundle-id com.mitchfultz.ralph\n  ralph app open --path /Applications/Ralph.app\n"
    )]
    Open(AppOpenArgs),
}

#[derive(Args, Debug, Clone)]
pub struct AppOpenArgs {
    /// Bundle identifier to open (default: com.mitchfultz.ralph).
    ///
    /// Uses: `open -b <bundle_id>`.
    #[arg(long, value_name = "BUNDLE_ID", conflicts_with = "path")]
    pub bundle_id: Option<String>,

    /// Open the app at a specific `.app` bundle path (useful for dev builds).
    ///
    /// Uses: `open <path>`.
    #[arg(long, value_name = "PATH", conflicts_with = "bundle_id")]
    pub path: Option<PathBuf>,
}
