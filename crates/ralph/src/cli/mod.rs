//! Ralph CLI surface (Clap types) and shared CLI helpers.
//!
//! This module centralizes the Clap-derived `Cli` and `Command` definitions while
//! delegating command-group logic to submodules (e.g. `queue`, `run`, `prompt`).
//! Keeping `main.rs` thin reduces churn and improves testability.

pub mod config;
pub mod doctor;
pub mod init;
pub mod prompt;
pub mod queue;
pub mod run;
pub mod scan;
pub mod task;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::contracts::QueueFile;

#[derive(Parser)]
#[command(name = "ralph")]
#[command(about = "Ralph")]
#[command(
    after_long_help = "Runner selection:\n  - CLI flags override project config, which overrides global config, which overrides built-in defaults.\n  - Default runner/model come from config files: project config (.ralph/config.json) > global config (~/.config/ralph/config.json) > built-in.\n  - `task` and `scan` accept --runner/--model/--effort as one-off overrides.\n  - `run one` and `run loop` accept --runner/--model/--effort as one-off overrides; otherwise they use task.agent overrides when present; otherwise config agent defaults.\n\nConfig example (.ralph/config.json):\n  {\n    \"version\": 1,\n    \"agent\": {\n      \"runner\": \"opencode\",\n      \"model\": \"gpt-5.2\",\n      \"opencode_bin\": \"opencode\",\n      \"gemini_bin\": \"gemini\",\n      \"claude_bin\": \"claude\"\n    }\n  }\n\nNotes:\n  - Allowed runners: codex, opencode, gemini, claude\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude accept arbitrary model ids)\n  - Use -i/--interactive with `run one` or `run loop` to launch the TUI for task selection and management\n\nExamples:\n  ralph queue list\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI gaps\"\n  ralph task --runner codex --model gpt-5.2-codex --effort high \"Fix the flaky test\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph scan --runner claude --model sonnet --focus \"risk audit\"\n  ralph task --runner claude --model opus \"Add tests for X\"\n  ralph run one\n  ralph run one -i\n  ralph run loop --max-tasks 1\n  ralph run loop -i"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Force operations (e.g., bypass stale queue locks).
    #[arg(long, global = true)]
    pub force: bool,

    /// Increase output verbosity (sets log level to info).
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    Queue(queue::QueueArgs),
    Config(config::ConfigArgs),
    Run(run::RunArgs),
    Task(task::TaskArgs),
    Scan(scan::ScanArgs),
    Init(init::InitArgs),
    /// Render and print the final compiled prompts used by Ralph (for debugging/auditing).
    #[command(
        after_long_help = "Examples:\n  ralph prompt worker --phase 1 --rp-on\n  ralph prompt worker --phase 2 --task-id RQ-0001 --plan-file .ralph/cache/plans/RQ-0001.md\n  ralph prompt scan --focus \"CI gaps\" --rp-off\n  ralph prompt task-builder --request \"Add tests\" --tags rust,tests --scope crates/ralph --rp-on\n"
    )]
    Prompt(prompt::PromptArgs),
    /// Verify environment readiness and configuration.
    #[command(after_long_help = "Example:\n  ralph doctor")]
    Doctor,
}

pub(crate) fn load_and_validate_queues(
    resolved: &crate::config::Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let queue_file = crate::queue::load_queue(&resolved.queue_path)?;

    let done_file = if include_done {
        Some(crate::queue::load_queue_or_default(&resolved.done_path)?)
    } else {
        None
    };

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    if let Some(d) = done_ref {
        crate::queue::validate_queue_set(
            &queue_file,
            Some(d),
            &resolved.id_prefix,
            resolved.id_width,
        )?;
    } else {
        crate::queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
    }

    Ok((queue_file, done_file))
}

pub(crate) fn resolve_list_limit(limit: u32, all: bool) -> Option<usize> {
    if all || limit == 0 {
        None
    } else {
        Some(limit as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::{run, Cli, Command};
    use crate::cli::task;
    use clap::Parser;

    #[test]
    fn cli_parses_queue_list_smoke() {
        let cli = Cli::try_parse_from(["ralph", "queue", "list"]).expect("parse");
        match cli.command {
            Command::Queue(_) => {}
            other => panic!(
                "expected queue command, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cli_rejects_invalid_prompt_phase() {
        let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "4"])
            .err()
            .expect("parse failure");
        let msg = err.to_string();
        assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
    }

    #[test]
    fn cli_parses_run_git_revert_mode() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--git-revert-mode", "disabled"])
            .expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert_eq!(args.agent.git_revert_mode.as_deref(), Some("disabled"));
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_include_draft() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--include-draft"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert!(args.agent.include_draft);
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_one_id() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--id", "RQ-0001"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert_eq!(args.id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_rejects_run_one_id_with_interactive() {
        let err = Cli::try_parse_from(["ralph", "run", "one", "--id", "RQ-0001", "-i"])
            .err()
            .expect("parse failure");
        let msg = err.to_string();
        assert!(
            msg.contains("cannot be used with") || msg.contains("conflicts"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_parses_task_default_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "Add", "tests"]).expect("parse");
        match cli.command {
            Command::Task(task::TaskArgs { command, build }) => {
                assert!(command.is_none(), "expected implicit build subcommand");
                assert_eq!(build.request, vec!["Add".to_string(), "tests".to_string()]);
            }
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_ready_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "ready", "RQ-0005"]).expect("parse");
        match cli.command {
            Command::Task(task::TaskArgs { command, .. }) => match command {
                Some(task::TaskCommand::Ready(args)) => {
                    assert_eq!(args.task_id, "RQ-0005");
                }
                _ => panic!("expected task ready command"),
            },
            _ => panic!("expected task command"),
        }
    }
}
