//! `ralph watch` command: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap arguments for watch commands.
//! - Dispatch watch execution with file watching and task detection.
//!
//! Not handled here:
//! - File system watching implementation details.
//! - TODO/FIXME comment detection logic.
//! - Queue mutation operations.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Watch mode respects gitignore patterns for file exclusion.

use crate::commands::watch::{CommentType, WatchOptions};
use crate::{commands::watch as watch_cmd, config};
use anyhow::Result;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// Comment types to detect in watched files.
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum WatchCommentType {
    /// Detect TODO comments.
    Todo,
    /// Detect FIXME comments.
    Fixme,
    /// Detect HACK comments.
    Hack,
    /// Detect XXX comments.
    Xxx,
    /// Detect all comment types (default).
    All,
}

impl From<WatchCommentType> for CommentType {
    fn from(value: WatchCommentType) -> Self {
        match value {
            WatchCommentType::Todo => CommentType::Todo,
            WatchCommentType::Fixme => CommentType::Fixme,
            WatchCommentType::Hack => CommentType::Hack,
            WatchCommentType::Xxx => CommentType::Xxx,
            WatchCommentType::All => CommentType::All,
        }
    }
}

pub fn handle_watch(args: WatchArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    let comment_types: Vec<CommentType> = if args.comments.is_empty() {
        vec![CommentType::All]
    } else {
        args.comments.iter().map(|&c| c.into()).collect()
    };

    let patterns: Vec<String> = if args.patterns.is_empty() {
        vec![
            "*.rs".to_string(),
            "*.ts".to_string(),
            "*.js".to_string(),
            "*.py".to_string(),
            "*.go".to_string(),
            "*.java".to_string(),
            "*.md".to_string(),
            "*.toml".to_string(),
            "*.json".to_string(),
        ]
    } else {
        args.patterns.clone()
    };

    let paths: Vec<PathBuf> = if args.paths.is_empty() {
        vec![std::env::current_dir()?]
    } else {
        args.paths.clone()
    };

    watch_cmd::run_watch(
        &resolved,
        WatchOptions {
            patterns,
            debounce_ms: args.debounce_ms,
            auto_queue: args.auto_queue,
            notify: args.notify,
            ignore_patterns: args.ignore_patterns,
            comment_types,
            paths,
            force,
        },
    )
}

#[derive(Args)]
#[command(
    about = "Watch files for changes and auto-detect tasks from TODO/FIXME/HACK/XXX comments",
    after_long_help = "Examples:
  ralph watch
  ralph watch src/
  ralph watch --patterns \"*.rs,*.toml\"
  ralph watch --auto-queue
  ralph watch --notify
  ralph watch --comments todo,fixme
  ralph watch --debounce-ms 1000
  ralph watch --ignore-patterns \"vendor/,target/,node_modules/\""
)]
pub struct WatchArgs {
    /// Directories or files to watch (defaults to current directory).
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// File patterns to watch (comma-separated, default: *.rs,*.ts,*.js,*.py,*.go,*.java,*.md,*.toml,*.json).
    #[arg(long, value_delimiter = ',')]
    pub patterns: Vec<String>,

    /// Debounce duration in milliseconds (default: 500).
    #[arg(long, default_value_t = 500)]
    pub debounce_ms: u64,

    /// Automatically create tasks without prompting.
    #[arg(long)]
    pub auto_queue: bool,

    /// Enable desktop notifications for new tasks.
    #[arg(long)]
    pub notify: bool,

    /// Additional gitignore-style exclusions (comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub ignore_patterns: Vec<String>,

    /// Comment types to detect: todo,fixme,hack,xxx,all (default: all).
    #[arg(long, value_enum, value_delimiter = ',')]
    pub comments: Vec<WatchCommentType>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn watch_help_examples_exist() {
        let mut cmd = Cli::command();
        let watch = cmd.find_subcommand_mut("watch").expect("watch subcommand");
        let help = watch.render_long_help().to_string();

        assert!(help.contains("ralph watch"), "missing basic watch example");
        assert!(
            help.contains("--auto-queue"),
            "missing --auto-queue example"
        );
        assert!(help.contains("--notify"), "missing --notify example");
        assert!(help.contains("--comments"), "missing --comments example");
    }

    #[test]
    fn watch_parses_default_args() {
        let cli = Cli::try_parse_from(["ralph", "watch"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert!(args.paths.is_empty());
                assert!(args.patterns.is_empty());
                assert_eq!(args.debounce_ms, 500);
                assert!(!args.auto_queue);
                assert!(!args.notify);
                assert!(args.ignore_patterns.is_empty());
                assert!(args.comments.is_empty());
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_paths() {
        let cli = Cli::try_parse_from(["ralph", "watch", "src/", "tests/"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert_eq!(args.paths.len(), 2);
                assert_eq!(args.paths[0], PathBuf::from("src/"));
                assert_eq!(args.paths[1], PathBuf::from("tests/"));
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_patterns() {
        let cli =
            Cli::try_parse_from(["ralph", "watch", "--patterns", "*.rs,*.toml"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert_eq!(args.patterns, vec!["*.rs", "*.toml"]);
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_debounce() {
        let cli = Cli::try_parse_from(["ralph", "watch", "--debounce-ms", "1000"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert_eq!(args.debounce_ms, 1000);
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_auto_queue() {
        let cli = Cli::try_parse_from(["ralph", "watch", "--auto-queue"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert!(args.auto_queue);
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_notify() {
        let cli = Cli::try_parse_from(["ralph", "watch", "--notify"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert!(args.notify);
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_comments() {
        let cli =
            Cli::try_parse_from(["ralph", "watch", "--comments", "todo,fixme"]).expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert_eq!(args.comments.len(), 2);
                assert_eq!(args.comments[0], WatchCommentType::Todo);
                assert_eq!(args.comments[1], WatchCommentType::Fixme);
            }
            _ => panic!("expected watch command"),
        }
    }

    #[test]
    fn watch_parses_ignore_patterns() {
        let cli = Cli::try_parse_from(["ralph", "watch", "--ignore-patterns", "vendor/,target/"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Watch(args) => {
                assert_eq!(args.ignore_patterns, vec!["vendor/", "target/"]);
            }
            _ => panic!("expected watch command"),
        }
    }
}
