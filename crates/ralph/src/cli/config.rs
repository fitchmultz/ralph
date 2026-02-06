//! `ralph config ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

use crate::{config, contracts};

/// Output format for `config show` command.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum ConfigShowFormat {
    /// YAML output (human-readable, default).
    #[default]
    #[value(alias = "text", alias = "yml")]
    Yaml,

    /// JSON output for scripting and tooling.
    Json,
}

/// Arguments for the `ralph config show` command.
#[derive(Args, Debug, Clone, Copy)]
pub struct ConfigShowArgs {
    /// Output format.
    #[arg(long, value_enum, default_value = "yaml")]
    pub format: ConfigShowFormat,
}

pub fn handle_config(cmd: ConfigCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        ConfigCommand::Show(args) => match args.format {
            ConfigShowFormat::Json => {
                let rendered = serde_json::to_string_pretty(&resolved.config)?;
                println!("{rendered}");
            }
            ConfigShowFormat::Yaml => {
                let rendered = serde_yaml::to_string(&resolved.config)?;
                print!("{rendered}");
            }
        },
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
    after_long_help = "Examples:\n  ralph config show\n  ralph config show --format json\n  ralph config paths\n  ralph config schema"
)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show the resolved Ralph configuration.
    #[command(
        after_long_help = "Examples:\n  ralph config show\n  ralph config show --format json\n  ralph config show --format yaml"
    )]
    Show(ConfigShowArgs),
    /// Print paths to the queue, done archive, and config files.
    #[command(after_long_help = "Example:\n  ralph config paths")]
    Paths,
    /// Print the JSON schema for the configuration.
    #[command(after_long_help = "Example:\n  ralph config schema")]
    Schema,
}
