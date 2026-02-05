//! Productivity CLI commands.
//!
//! Responsibilities:
//! - Provide human- and machine-readable views of productivity stats.
//! - Read from `.ralph/cache/productivity.json` via `crate::productivity`.
//!
//! Not handled here:
//! - Recording completions (handled where tasks are completed).
//! - Queue mutations.
//!
//! Invariants/assumptions:
//! - Stats timestamps are RFC3339.
//! - Missing stats file implies zeroed defaults.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

use crate::config;
use crate::productivity;

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
#[clap(rename_all = "snake_case")]
pub enum ProductivityFormat {
    #[default]
    Text,
    Json,
}

#[derive(Args)]
#[command(about = "View productivity stats (streaks, velocity, milestones)")]
pub struct ProductivityArgs {
    #[command(subcommand)]
    pub command: ProductivityCommand,
}

#[derive(Subcommand)]
pub enum ProductivityCommand {
    /// Summary: total completions, streak, milestones, recent completions.
    #[command(
        after_long_help = "Examples:\n  ralph productivity summary\n  ralph productivity summary --format json\n  ralph productivity summary --recent 10"
    )]
    Summary(ProductivitySummaryArgs),

    /// Velocity: completions per day over windows (7/30 by default).
    #[command(
        after_long_help = "Examples:\n  ralph productivity velocity\n  ralph productivity velocity --format json\n  ralph productivity velocity --days 14"
    )]
    Velocity(ProductivityVelocityArgs),

    /// Streak details: current/longest streak + last completion date.
    #[command(
        after_long_help = "Examples:\n  ralph productivity streak\n  ralph productivity streak --format json"
    )]
    Streak(ProductivityStreakArgs),
}

#[derive(Args)]
pub struct ProductivitySummaryArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = ProductivityFormat::Text)]
    pub format: ProductivityFormat,

    /// Number of recent completions to show.
    #[arg(long, default_value = "5")]
    pub recent: usize,
}

#[derive(Args)]
pub struct ProductivityVelocityArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = ProductivityFormat::Text)]
    pub format: ProductivityFormat,

    /// Window size in days.
    #[arg(long, default_value = "7")]
    pub days: u32,
}

#[derive(Args)]
pub struct ProductivityStreakArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = ProductivityFormat::Text)]
    pub format: ProductivityFormat,
}

pub fn handle(args: ProductivityArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let stats = productivity::load_productivity_stats(&cache_dir)?;

    match args.command {
        ProductivityCommand::Summary(cmd) => {
            let report = productivity::build_summary_report(&stats, cmd.recent);
            match cmd.format {
                ProductivityFormat::Json => {
                    print!("{}", serde_json::to_string_pretty(&report)?);
                }
                ProductivityFormat::Text => {
                    productivity::print_summary_report_text(&report);
                }
            }
        }
        ProductivityCommand::Velocity(cmd) => {
            let report = productivity::build_velocity_report(&stats, cmd.days);
            match cmd.format {
                ProductivityFormat::Json => {
                    print!("{}", serde_json::to_string_pretty(&report)?);
                }
                ProductivityFormat::Text => {
                    productivity::print_velocity_report_text(&report);
                }
            }
        }
        ProductivityCommand::Streak(cmd) => {
            let report = productivity::build_streak_report(&stats);
            match cmd.format {
                ProductivityFormat::Json => {
                    print!("{}", serde_json::to_string_pretty(&report)?);
                }
                ProductivityFormat::Text => {
                    productivity::print_streak_report_text(&report);
                }
            }
        }
    }

    Ok(())
}
