//! `ralph prd ...` command group: Clap types and handler.
//!
//! Purpose:
//! - `ralph prd ...` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap structures for PRD-related commands.
//! - Route PRD subcommands to the implementation layer.
//!
//! Not handled here:
//! - PRD parsing logic (see `crate::commands::prd`).
//! - Queue persistence or lock management.
//! - Task generation from PRD content.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Callers resolve configuration before executing commands.
//! - PRD file paths are validated to exist and be readable.
//! - Generated tasks follow the standard task schema.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::commands::prd as prd_cmd;
use crate::config;

pub fn handle_prd(args: PrdArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        PrdCommand::Create(args) => {
            let opts = prd_cmd::CreateOptions {
                path: args.path,
                multi: args.multi,
                dry_run: args.dry_run,
                priority: args.priority.map(|p| p.into()),
                tags: args.tag,
                draft: args.draft,
            };
            prd_cmd::create_from_prd(&resolved, &opts, force)
        }
    }
}

#[derive(Args)]
#[command(
    about = "Convert PRD (Product Requirements Document) markdown to tasks",
    after_long_help = "Examples:\n  ralph prd create docs/prd/new-feature.md\n  ralph prd create docs/prd/new-feature.md --multi\n  ralph prd create docs/prd/new-feature.md --dry-run\n  ralph prd create docs/prd/new-feature.md --priority high --tag feature\n  ralph prd create docs/prd/new-feature.md --draft"
)]
pub struct PrdArgs {
    #[command(subcommand)]
    pub command: PrdCommand,
}

#[derive(Subcommand)]
pub enum PrdCommand {
    /// Create task(s) from a PRD markdown file.
    #[command(
        after_long_help = "Converts a PRD markdown file into one or more Ralph tasks.\n\nBy default, creates a single consolidated task from the PRD.\nUse --multi to create one task per user story found in the PRD.\n\nPRD Format:\nThe PRD should contain standard markdown sections:\n- Title (first # heading)\n- Introduction/Overview (optional)\n- User Stories (### US-XXX: Title format)\n- Functional Requirements (optional)\n- Non-Goals (optional)\n\nExamples:\n  ralph prd create path/to/prd.md\n  ralph prd create path/to/prd.md --multi\n  ralph prd create path/to/prd.md --dry-run\n  ralph prd create path/to/prd.md --priority high --tag feature --tag v2.0\n  ralph prd create path/to/prd.md --draft\n  ralph prd create path/to/prd.md --multi --priority medium --tag user-story"
    )]
    Create(PrdCreateArgs),
}

#[derive(Args)]
pub struct PrdCreateArgs {
    /// Path to the PRD markdown file.
    #[arg(value_name = "PATH")]
    pub path: std::path::PathBuf,

    /// Create multiple tasks (one per user story) instead of a single consolidated task.
    #[arg(long)]
    pub multi: bool,

    /// Preview generated tasks without inserting into the queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Set priority for generated tasks (low, medium, high, critical).
    #[arg(long, value_enum)]
    pub priority: Option<PrdPriorityArg>,

    /// Add tags to all generated tasks (repeatable).
    #[arg(long = "tag")]
    pub tag: Vec<String>,

    /// Create tasks as draft status instead of todo.
    #[arg(long)]
    pub draft: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum PrdPriorityArg {
    Low,
    Medium,
    High,
    Critical,
}

impl From<PrdPriorityArg> for crate::contracts::TaskPriority {
    fn from(value: PrdPriorityArg) -> Self {
        match value {
            PrdPriorityArg::Low => crate::contracts::TaskPriority::Low,
            PrdPriorityArg::Medium => crate::contracts::TaskPriority::Medium,
            PrdPriorityArg::High => crate::contracts::TaskPriority::High,
            PrdPriorityArg::Critical => crate::contracts::TaskPriority::Critical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_parses_prd_create_basic() {
        let cli = crate::cli::Cli::try_parse_from(["ralph", "prd", "create", "docs/prd.md"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Prd(args) => match args.command {
                PrdCommand::Create(create_args) => {
                    assert_eq!(create_args.path, std::path::PathBuf::from("docs/prd.md"));
                    assert!(!create_args.multi);
                    assert!(!create_args.dry_run);
                    assert!(!create_args.draft);
                }
            },
            _ => panic!("expected prd command"),
        }
    }

    #[test]
    fn cli_parses_prd_create_with_flags() {
        let cli = crate::cli::Cli::try_parse_from([
            "ralph",
            "prd",
            "create",
            "docs/prd.md",
            "--multi",
            "--dry-run",
            "--priority",
            "high",
            "--tag",
            "feature",
            "--tag",
            "v2.0",
            "--draft",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Prd(args) => match args.command {
                PrdCommand::Create(create_args) => {
                    assert_eq!(create_args.path, std::path::PathBuf::from("docs/prd.md"));
                    assert!(create_args.multi);
                    assert!(create_args.dry_run);
                    assert!(create_args.draft);
                    assert_eq!(create_args.priority, Some(PrdPriorityArg::High));
                    assert_eq!(create_args.tag, vec!["feature", "v2.0"]);
                }
            },
            _ => panic!("expected prd command"),
        }
    }

    #[test]
    fn cli_parses_prd_create_priority_variants() {
        for (arg, expected) in [
            ("low", PrdPriorityArg::Low),
            ("medium", PrdPriorityArg::Medium),
            ("high", PrdPriorityArg::High),
            ("critical", PrdPriorityArg::Critical),
        ] {
            let cli = crate::cli::Cli::try_parse_from([
                "ralph",
                "prd",
                "create",
                "docs/prd.md",
                "--priority",
                arg,
            ])
            .expect("parse");
            match cli.command {
                crate::cli::Command::Prd(args) => match args.command {
                    PrdCommand::Create(create_args) => {
                        assert_eq!(
                            create_args.priority,
                            Some(expected),
                            "failed for priority: {arg}"
                        );
                    }
                },
                _ => panic!("expected prd command"),
            }
        }
    }
}
