//! Productivity CLI commands.
//!
//! Purpose:
//! - Productivity CLI commands.
//!
//! Responsibilities:
//! - Provide human- and machine-readable views of productivity stats.
//! - Read from `.ralph/cache/productivity.jsonc` via `crate::productivity`.
//!
//! Not handled here:
//! - Recording completions (handled where tasks are completed).
//! - Queue mutations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Stats timestamps are RFC3339.
//! - Missing stats file implies zeroed defaults.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

use crate::config;
use crate::productivity;

fn load_done_queue_for_estimation(
    resolved: &config::Resolved,
) -> Result<crate::contracts::QueueFile> {
    crate::queue::load_queue_or_default(&resolved.done_path)
}

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

    /// Estimation accuracy: compare estimated vs actual time for completed tasks.
    #[command(
        after_long_help = "Examples:\n  ralph productivity estimation\n  ralph productivity estimation --format json"
    )]
    Estimation(ProductivityEstimationArgs),
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

#[derive(Args)]
pub struct ProductivityEstimationArgs {
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
        ProductivityCommand::Estimation(cmd) => {
            // Load completed tasks from the resolved done archive path.
            let done_queue = load_done_queue_for_estimation(&resolved)?;
            let report = productivity::build_estimation_report(&done_queue.tasks);
            match cmd.format {
                ProductivityFormat::Json => {
                    print!("{}", serde_json::to_string_pretty(&report)?);
                }
                ProductivityFormat::Text => {
                    productivity::print_estimation_report_text(&report);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, QueueFile, Task, TaskStatus};
    use std::path::PathBuf;

    #[test]
    fn estimation_loads_done_tasks_from_resolved_done_path() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path().to_path_buf();

        let default_done_path = repo_root.join(".ralph/done.jsonc");
        let custom_done_path = repo_root.join("archive/done.jsonc");
        std::fs::create_dir_all(default_done_path.parent().expect("default parent"))?;
        std::fs::create_dir_all(custom_done_path.parent().expect("custom parent"))?;

        let default_done = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-DEFAULT".to_string(),
                status: TaskStatus::Done,
                title: "default".to_string(),
                estimated_minutes: Some(10),
                actual_minutes: Some(10),
                ..Task::default()
            }],
        };
        crate::queue::save_queue(&default_done_path, &default_done)?;

        let custom_done = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-CUSTOM".to_string(),
                status: TaskStatus::Done,
                title: "custom".to_string(),
                estimated_minutes: Some(20),
                actual_minutes: Some(25),
                ..Task::default()
            }],
        };
        crate::queue::save_queue(&custom_done_path, &custom_done)?;

        let resolved = config::Resolved {
            config: Config::default(),
            repo_root,
            queue_path: PathBuf::from("unused-queue"),
            done_path: custom_done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let loaded = load_done_queue_for_estimation(&resolved)?;
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].id, "RQ-CUSTOM");
        Ok(())
    }
}
