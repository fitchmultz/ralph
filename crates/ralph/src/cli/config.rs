//! `ralph config ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

use crate::{agent, config, contracts};

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
    match cmd {
        ConfigCommand::Show(args) => {
            let resolved = config::resolve_from_cwd()?;
            match args.format {
                ConfigShowFormat::Json => {
                    let rendered = serde_json::to_string_pretty(&resolved.config)?;
                    println!("{rendered}");
                }
                ConfigShowFormat::Yaml => {
                    let rendered = serde_yaml::to_string(&resolved.config)?;
                    print!("{rendered}");
                }
            }
        }
        ConfigCommand::Paths => {
            let resolved = config::resolve_from_cwd()?;
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
        ConfigCommand::Profiles(profiles_args) => {
            handle_profiles(profiles_args)?;
        }
    }
    Ok(())
}

fn handle_profiles(args: ConfigProfilesArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        ConfigProfilesCommand::List => {
            let names = agent::all_profile_names(resolved.config.profiles.as_ref());

            if names.is_empty() {
                println!("No profiles configured.");
                println!(
                    "Define profiles under the `profiles` key in .ralph/config.jsonc or ~/.config/ralph/config.jsonc."
                );
                return Ok(());
            }

            println!("Available profiles:");
            for name in names {
                if let Some(patch) =
                    agent::resolve_profile_patch(&name, resolved.config.profiles.as_ref())
                {
                    let details = format_profile_summary(&patch);
                    println!("  {} - {}", name, details);
                } else {
                    println!("  {}", name);
                }
            }
        }
        ConfigProfilesCommand::Show { name } => {
            let name = name.trim();
            if name.is_empty() {
                anyhow::bail!("Profile name cannot be empty");
            }

            match agent::resolve_profile_patch(name, resolved.config.profiles.as_ref()) {
                Some(patch) => {
                    println!("Profile: {}", name);
                    if resolved
                        .config
                        .profiles
                        .as_ref()
                        .is_some_and(|p| p.contains_key(name))
                    {
                        println!("Source: config");
                    }
                    println!();
                    let rendered = serde_yaml::to_string(&patch)?;
                    print!("{}", rendered);
                }
                None => {
                    let names = agent::all_profile_names(resolved.config.profiles.as_ref());
                    if names.is_empty() {
                        anyhow::bail!(
                            "Unknown profile: {name:?}. No profiles are configured. Define profiles under the `profiles` key in .ralph/config.jsonc or ~/.config/ralph/config.jsonc."
                        );
                    }
                    anyhow::bail!(
                        "Unknown profile: {name:?}. Available configured profiles: {}",
                        names.into_iter().collect::<Vec<_>>().join(", ")
                    );
                }
            }
        }
    }
    Ok(())
}

/// Format a profile patch as a summary string.
fn format_profile_summary(patch: &contracts::AgentConfig) -> String {
    let mut parts = Vec::new();

    if let Some(runner) = &patch.runner {
        parts.push(format!("runner={}", runner.as_str()));
    }
    if let Some(model) = &patch.model {
        parts.push(format!("model={}", model.as_str()));
    }
    if let Some(phases) = patch.phases {
        parts.push(format!("phases={}", phases));
    }
    if let Some(effort) = &patch.reasoning_effort {
        parts.push(format!("effort={}", format_reasoning_effort(*effort)));
    }

    if parts.is_empty() {
        "no overrides".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_reasoning_effort(effort: contracts::ReasoningEffort) -> &'static str {
    match effort {
        contracts::ReasoningEffort::Low => "low",
        contracts::ReasoningEffort::Medium => "medium",
        contracts::ReasoningEffort::High => "high",
        contracts::ReasoningEffort::XHigh => "xhigh",
    }
}

#[derive(Args)]
#[command(
    about = "Inspect and manage Ralph configuration",
    after_long_help = "Examples:\n  ralph config show\n  ralph config show --format json\n  ralph config paths\n  ralph config schema\n  ralph config profiles list\n  ralph config profiles show fast-local"
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
    /// List and inspect configuration profiles.
    #[command(
        after_long_help = "Examples:\n  ralph config profiles list\n  ralph config profiles show fast-local\n  ralph config profiles show deep-review"
    )]
    Profiles(ConfigProfilesArgs),
}

/// Arguments for the `ralph config profiles` command.
#[derive(Args)]
pub struct ConfigProfilesArgs {
    #[command(subcommand)]
    pub command: ConfigProfilesCommand,
}

/// Subcommands for `ralph config profiles`.
#[derive(Subcommand)]
pub enum ConfigProfilesCommand {
    /// List available configured profiles.
    List,
    /// Show one configured profile (effective patch).
    Show { name: String },
}
