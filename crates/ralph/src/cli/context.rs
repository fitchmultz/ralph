//! `ralph context` command: Clap types and handler.
//!
//! Responsibilities:
//! - Define CLI arguments for the `context` command group (init, update, validate).
//! - Provide handler function that delegates to command implementations.
//!
//! Not handled here:
//! - Actual file generation and manipulation (see `commands::context`).
//! - Project type detection logic.
//!
//! Invariants/assumptions:
//! - Output paths are resolved relative to the repository root.
//! - Interactive mode requires a TTY.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::commands::context as context_cmd;
use crate::config;

/// Handle the `context` command group.
pub fn handle_context(args: ContextArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        ContextCommand::Init(init_args) => {
            let report = context_cmd::run_context_init(
                &resolved,
                context_cmd::ContextInitOptions {
                    force: init_args.force,
                    project_type_hint: init_args.project_type,
                    output_path: init_args
                        .output
                        .unwrap_or_else(|| resolved.repo_root.join("AGENTS.md")),
                    interactive: init_args.interactive,
                },
            )?;

            match report.status {
                context_cmd::FileInitStatus::Created => {
                    log::info!(
                        "AGENTS.md created for {} project ({})",
                        format!("{:?}", report.detected_project_type).to_lowercase(),
                        report.output_path.display()
                    );
                }
                context_cmd::FileInitStatus::Valid => {
                    log::info!(
                        "AGENTS.md already exists ({}). Use --force to overwrite.",
                        report.output_path.display()
                    );
                }
            }
            Ok(())
        }
        ContextCommand::Update(update_args) => {
            let report = context_cmd::run_context_update(
                &resolved,
                context_cmd::ContextUpdateOptions {
                    sections: update_args.section,
                    file: update_args.file,
                    interactive: update_args.interactive,
                    dry_run: update_args.dry_run,
                    output_path: update_args
                        .output
                        .unwrap_or_else(|| resolved.repo_root.join("AGENTS.md")),
                },
            )?;

            if report.dry_run {
                log::info!("Dry run - no changes written");
            } else {
                log::info!(
                    "AGENTS.md updated: {} sections modified",
                    report.sections_updated.len()
                );
            }
            Ok(())
        }
        ContextCommand::Validate(validate_args) => {
            let report = context_cmd::run_context_validate(
                &resolved,
                context_cmd::ContextValidateOptions {
                    strict: validate_args.strict,
                    path: validate_args
                        .path
                        .unwrap_or_else(|| resolved.repo_root.join("AGENTS.md")),
                },
            )?;

            if report.valid {
                log::info!("AGENTS.md is valid and up to date");
            } else {
                log::warn!("AGENTS.md has issues:");
                if !report.missing_sections.is_empty() {
                    log::warn!("  Missing sections: {:?}", report.missing_sections);
                }
                if !report.outdated_sections.is_empty() {
                    log::warn!("  Outdated sections: {:?}", report.outdated_sections);
                }
                anyhow::bail!("Validation failed");
            }
            Ok(())
        }
    }
}

#[derive(Args)]
#[command(
    about = "Manage project context (AGENTS.md) for AI agents",
    after_long_help = "Examples:\n  ralph context init\n  ralph context init --project-type rust\n  ralph context update --section troubleshooting\n  ralph context validate\n  ralph context update --dry-run"
)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub command: ContextCommand,
}

#[derive(Subcommand)]
pub enum ContextCommand {
    /// Generate initial AGENTS.md from project detection
    #[command(
        after_long_help = "Examples:\n  ralph context init\n  ralph context init --force\n  ralph context init --project-type python --output docs/AGENTS.md"
    )]
    Init(ContextInitArgs),

    /// Update AGENTS.md with new learnings
    #[command(
        after_long_help = "Examples:\n  ralph context update --section troubleshooting\n  ralph context update --file new_learnings.md\n  ralph context update --interactive"
    )]
    Update(ContextUpdateArgs),

    /// Validate AGENTS.md is up to date with project structure
    #[command(
        after_long_help = "Examples:\n  ralph context validate\n  ralph context validate --strict"
    )]
    Validate(ContextValidateArgs),
}

#[derive(Args)]
pub struct ContextInitArgs {
    /// Force overwrite if AGENTS.md already exists
    #[arg(long)]
    pub force: bool,

    /// Project type override (auto-detect if not specified)
    #[arg(long, value_enum)]
    pub project_type: Option<ProjectTypeHint>,

    /// Output path for AGENTS.md (default: AGENTS.md in repo root)
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Interactive mode to guide through context creation
    #[arg(long, short)]
    pub interactive: bool,
}

#[derive(Args)]
pub struct ContextUpdateArgs {
    /// Section to update (can be specified multiple times)
    #[arg(long, short)]
    pub section: Vec<String>,

    /// File containing new learnings to append
    #[arg(long, short)]
    pub file: Option<PathBuf>,

    /// Interactive mode to select sections and input learnings
    #[arg(long, short)]
    pub interactive: bool,

    /// Dry run - preview changes without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Output path (default: existing AGENTS.md location)
    #[arg(long, short)]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct ContextValidateArgs {
    /// Strict mode - fail if any recommended sections are missing
    #[arg(long)]
    pub strict: bool,

    /// Path to AGENTS.md (default: auto-discover)
    #[arg(long, short)]
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum ProjectTypeHint {
    Rust,
    Python,
    TypeScript,
    Go,
    Generic,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_parses_context_init() {
        let cli = crate::cli::Cli::try_parse_from(["ralph", "context", "init"]).expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Init(init_args) => {
                    assert!(!init_args.force);
                    assert!(init_args.project_type.is_none());
                    assert!(init_args.output.is_none());
                    assert!(!init_args.interactive);
                }
                _ => panic!("expected context init command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_init_with_force() {
        let cli = crate::cli::Cli::try_parse_from(["ralph", "context", "init", "--force"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Init(init_args) => {
                    assert!(init_args.force);
                }
                _ => panic!("expected context init command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_init_with_project_type() {
        let cli =
            crate::cli::Cli::try_parse_from(["ralph", "context", "init", "--project-type", "rust"])
                .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Init(init_args) => {
                    assert_eq!(init_args.project_type, Some(ProjectTypeHint::Rust));
                }
                _ => panic!("expected context init command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_init_with_output() {
        let cli = crate::cli::Cli::try_parse_from([
            "ralph",
            "context",
            "init",
            "--output",
            "docs/AGENTS.md",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Init(init_args) => {
                    assert_eq!(init_args.output, Some(PathBuf::from("docs/AGENTS.md")));
                }
                _ => panic!("expected context init command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_update_with_section() {
        let cli = crate::cli::Cli::try_parse_from([
            "ralph",
            "context",
            "update",
            "--section",
            "troubleshooting",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Update(update_args) => {
                    assert_eq!(update_args.section, vec!["troubleshooting"]);
                    assert!(!update_args.dry_run);
                }
                _ => panic!("expected context update command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_update_with_multiple_sections() {
        let cli = crate::cli::Cli::try_parse_from([
            "ralph",
            "context",
            "update",
            "--section",
            "troubleshooting",
            "--section",
            "git-hygiene",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Update(update_args) => {
                    assert_eq!(update_args.section, vec!["troubleshooting", "git-hygiene"]);
                }
                _ => panic!("expected context update command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_update_with_dry_run() {
        let cli = crate::cli::Cli::try_parse_from(["ralph", "context", "update", "--dry-run"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Update(update_args) => {
                    assert!(update_args.dry_run);
                }
                _ => panic!("expected context update command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_validate() {
        let cli = crate::cli::Cli::try_parse_from(["ralph", "context", "validate"]).expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Validate(validate_args) => {
                    assert!(!validate_args.strict);
                    assert!(validate_args.path.is_none());
                }
                _ => panic!("expected context validate command"),
            },
            _ => panic!("expected context command"),
        }
    }

    #[test]
    fn cli_parses_context_validate_with_strict() {
        let cli = crate::cli::Cli::try_parse_from(["ralph", "context", "validate", "--strict"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Context(args) => match args.command {
                ContextCommand::Validate(validate_args) => {
                    assert!(validate_args.strict);
                }
                _ => panic!("expected context validate command"),
            },
            _ => panic!("expected context command"),
        }
    }
}
