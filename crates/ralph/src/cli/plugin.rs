//! Plugin CLI surface.
//!
//! Responsibilities:
//! - Define Clap args for plugin management.
//!
//! Not handled here:
//! - Filesystem operations (see `crate::commands::plugin`).
//!
//! Invariants/assumptions:
//! - `install` copies a plugin directory containing `plugin.json` into the chosen scope.
//! - Installing does NOT auto-enable the plugin (security).

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
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
        #[arg(long, default_value = "project")]
        scope: String,
    },

    /// Uninstall a plugin by id from the chosen scope.
    #[command(
        after_long_help = "Examples:\n  ralph plugin uninstall acme.super_runner --scope project\n"
    )]
    Uninstall {
        id: String,

        #[arg(long, default_value = "project")]
        scope: String,
    },
}
