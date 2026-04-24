//! Plugin CLI surface.
//!
//! Purpose:
//! - Plugin CLI surface.
//!
//! Responsibilities:
//! - Define Clap args for plugin management.
//!
//! Not handled here:
//! - Filesystem operations (see `crate::commands::plugin`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `install` copies a plugin directory containing `plugin.json` into the chosen scope.
//! - Installing does NOT auto-enable the plugin (security).

use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum PluginScopeArg {
    Project,
    Global,
}

#[derive(Args, Debug, Clone)]
#[command(
    after_long_help = "Examples:\n  ralph plugin init acme.super_runner\n  ralph plugin init acme.super_runner --with-runner\n  ralph plugin init acme.super_runner --with-processor\n  ralph plugin init acme.super_runner --scope global\n  ralph plugin init acme.super_runner --dry-run\n"
)]
pub struct PluginInitArgs {
    /// Plugin ID (used as directory name in default layout).
    #[arg(value_name = "PLUGIN_ID")]
    pub id: String,

    /// Where to scaffold the plugin (ignored when --path is provided).
    #[arg(long, value_enum, default_value = "project")]
    pub scope: PluginScopeArg,

    /// Target plugin directory (overrides --scope). Relative paths are resolved from repo root.
    #[arg(long, value_name = "DIR")]
    pub path: Option<PathBuf>,

    /// Manifest name (default: derived from id).
    #[arg(long)]
    pub name: Option<String>,

    /// Manifest version (SemVer string).
    #[arg(long, default_value = "0.1.0")]
    pub version: String,

    /// Optional manifest description.
    #[arg(long)]
    pub description: Option<String>,

    /// Include runner stub + runner manifest section.
    #[arg(long)]
    pub with_runner: bool,

    /// Include processor stub + processors manifest section.
    #[arg(long)]
    pub with_processor: bool,

    /// Preview what would be written without creating files.
    #[arg(long)]
    pub dry_run: bool,

    /// Overwrite scaffolded files if the directory already exists.
    #[arg(long)]
    pub force: bool,
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// List discovered plugins (global + project) and whether they are enabled.
    #[command(after_long_help = "Examples:\n  ralph plugin list\n  ralph plugin list --json\n")]
    List {
        /// Output JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Validate discovered plugin manifests and referenced executables.
    #[command(
        after_long_help = "Examples:\n  ralph plugin validate\n  ralph plugin validate --id acme.super_runner\n"
    )]
    Validate {
        /// Validate only a single plugin id.
        #[arg(long)]
        id: Option<String>,
    },

    /// Install a plugin from a local directory (must contain plugin.json).
    #[command(
        after_long_help = "Examples:\n  ralph plugin install ./my-plugin --scope project\n  ralph plugin install ./my-plugin --scope global\n\nNotes:\n  - Install does not enable the plugin. Enable via config.plugins.plugins.<id>.enabled=true\n"
    )]
    Install {
        /// Source directory containing plugin.json
        source: String,

        /// Install scope: project or global
        #[arg(long, value_enum, default_value = "project")]
        scope: PluginScopeArg,
    },

    /// Uninstall a plugin by id from the chosen scope.
    #[command(
        after_long_help = "Examples:\n  ralph plugin uninstall acme.super_runner --scope project\n"
    )]
    Uninstall {
        id: String,

        #[arg(long, value_enum, default_value = "project")]
        scope: PluginScopeArg,
    },

    /// Scaffold a new plugin directory with plugin.json and optional scripts.
    #[command(
        after_long_help = "Examples:\n  ralph plugin init acme.super_runner\n  ralph plugin init acme.super_runner --with-runner\n  ralph plugin init acme.super_runner --with-processor\n  ralph plugin init acme.super_runner --scope global\n  ralph plugin init acme.super_runner --dry-run\n"
    )]
    Init(PluginInitArgs),
}
