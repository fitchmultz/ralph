//! `ralph config ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{config, contracts};

pub fn handle_config(cmd: ConfigCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        ConfigCommand::Show => {
            let rendered = serde_json::to_string_pretty(&resolved.config)?;
            print!("{rendered}");
        }
        ConfigCommand::Paths => {
            println!("repo_root: {}", resolved.repo_root.display());
            println!("queue: {}", resolved.queue_path.display());
            println!("done: {}", resolved.done_path.display());
            if let Some(path) = resolved.global_config_path.as_ref() {
                println!("global_config: {}", path.display());
            } else {
                println!("global_config: (unavailable)");
            }
            if let Some(path) = resolved.project_config_path.as_ref() {
                println!("project_config: {}", path.display());
            } else {
                println!("project_config: (unavailable)");
            }
        }
        ConfigCommand::Schema => {
            let schema = schemars::schema_for!(contracts::Config);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
    }
    Ok(())
}

#[derive(Args)]
#[command(
    about = "Inspect and manage Ralph configuration",
    after_long_help = "Examples:\n  ralph config show\n  ralph config paths"
)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show the resolved Ralph configuration (YAML).
    #[command(after_long_help = "Example:\n  ralph config show")]
    Show,
    /// Print paths to the queue, done archive, and config files.
    #[command(after_long_help = "Example:\n  ralph config paths")]
    Paths,
    /// Print the JSON schema for the configuration.
    #[command(after_long_help = "Example:\n  ralph config schema")]
    Schema,
}
