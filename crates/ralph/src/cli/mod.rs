//! Ralph CLI surface (Clap types) and shared CLI helpers.
//!
//! Responsibilities:
//! - Centralize the top-level `Cli` and `Command` definitions for Clap.
//! - Delegate command-group logic to submodules (queue/run/task/scan/etc.).
//! - Provide small shared CLI helpers used across command groups.
//!
//! Not handled here:
//! - Command execution logic (see submodules).
//! - Queue persistence or lock management.
//! - Prompt rendering or runner execution.
//!
//! Invariants/assumptions:
//! - Subcommands validate their own inputs and config dependencies.
//! - CLI parsing happens after argument normalization in `main`.

pub mod config;
pub mod doctor;
pub mod init;
pub mod interactive;
pub mod prompt;
pub mod queue;
pub mod run;
pub mod scan;
pub mod task;
pub mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::contracts::QueueFile;

#[derive(Parser)]
#[command(name = "ralph")]
#[command(about = "Ralph")]
#[command(
    after_long_help = "Runner selection:\n  - CLI flags override project config, which overrides global config, which overrides built-in defaults.\n  - Default runner/model come from config files: project config (.ralph/config.json) > global config (~/.config/ralph/config.json) > built-in.\n  - `task` and `scan` accept --runner/--model/--effort as one-off overrides.\n  - `run one` and `run loop` accept --runner/--model/--effort as one-off overrides; otherwise they use task.agent overrides when present; otherwise config agent defaults.\n\nConfig example (.ralph/config.json):\n  {\n    \"version\": 1,\n    \"agent\": {\n      \"runner\": \"opencode\",\n      \"model\": \"gpt-5.2\",\n      \"opencode_bin\": \"opencode\",\n      \"gemini_bin\": \"gemini\",\n      \"claude_bin\": \"claude\"\n    }\n  }\n\nNotes:\n  - Allowed runners: codex, opencode, gemini, claude\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude accept arbitrary model ids)\n  - Use -i/--interactive with `run one` or `run loop` to launch the TUI for task execution\n  - Use `ralph tui` for the full interactive UI; pass `--read-only` to disable execution\n\nExamples:\n  ralph queue list\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI gaps\"\n  ralph task --runner codex --model gpt-5.2-codex --effort high \"Fix the flaky test\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph scan --runner claude --model sonnet --focus \"risk audit\"\n  ralph task --runner claude --model opus \"Add tests for X\"\n  ralph run one\n  ralph run one -i\n  ralph run loop --max-tasks 1\n  ralph run loop -i\n  ralph tui"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Force operations (e.g., bypass stale queue locks; bypass clean-repo safety checks for commands that enforce them, e.g. `run one`, `run loop`, and `scan`).
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
    /// Launch the interactive TUI (queue management + execution + loop).
    Tui(tui::TuiArgs),
    /// Render and print the final compiled prompts used by Ralph (for debugging/auditing).
    #[command(
        after_long_help = "Examples:\n  ralph prompt worker --phase 1 --repo-prompt plan\n  ralph prompt worker --phase 2 --task-id RQ-0001 --plan-file .ralph/cache/plans/RQ-0001.md\n  ralph prompt scan --focus \"CI gaps\" --repo-prompt off\n  ralph prompt task-builder --request \"Add tests\" --tags rust,tests --scope crates/ralph --repo-prompt tools\n"
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
    crate::queue::load_and_validate_queues(resolved, include_done)
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
    use super::{run, tui, Cli, Command};
    use crate::cli::{queue, task};
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
    fn cli_parses_queue_archive_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "queue", "archive"]).expect("parse");
        match cli.command {
            Command::Queue(queue::QueueArgs { command }) => match command {
                queue::QueueCommand::Archive => {}
                _ => panic!("expected queue archive command"),
            },
            _ => panic!("expected queue command"),
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
    fn cli_parses_run_git_commit_push_off() {
        let cli =
            Cli::try_parse_from(["ralph", "run", "one", "--git-commit-push-off"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert!(args.agent.git_commit_push_off);
                    assert!(!args.agent.git_commit_push_on);
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
    fn cli_parses_run_one_debug() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--debug"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert!(args.debug);
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_loop_debug() {
        let cli = Cli::try_parse_from(["ralph", "run", "loop", "--debug"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::Loop(args) => {
                    assert!(args.debug);
                }
                _ => panic!("expected run loop command"),
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
    fn cli_parses_task_update_without_id() {
        let cli = Cli::try_parse_from(["ralph", "task", "update"]).expect("parse");
        match cli.command {
            Command::Task(task::TaskArgs { command, .. }) => match command {
                Some(task::TaskCommand::Update(args)) => {
                    assert!(args.task_id.is_none());
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_update_with_id() {
        let cli = Cli::try_parse_from(["ralph", "task", "update", "RQ-0001"]).expect("parse");
        match cli.command {
            Command::Task(task::TaskArgs { command, .. }) => match command {
                Some(task::TaskCommand::Update(args)) => {
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
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
    fn cli_rejects_run_one_id_with_interactive_long() {
        let err = Cli::try_parse_from(["ralph", "run", "one", "--id", "RQ-0001", "--interactive"])
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

    #[test]
    fn cli_parses_task_done_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "done", "RQ-0001"]).expect("parse");
        match cli.command {
            Command::Task(task::TaskArgs { command, .. }) => match command {
                Some(task::TaskCommand::Done(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                }
                _ => panic!("expected task done command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_reject_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "reject", "RQ-0002"]).expect("parse");
        match cli.command {
            Command::Task(task::TaskArgs { command, .. }) => match command {
                Some(task::TaskCommand::Reject(args)) => {
                    assert_eq!(args.task_id, "RQ-0002");
                }
                _ => panic!("expected task reject command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_rejects_queue_set_status_subcommand() {
        let result = Cli::try_parse_from(["ralph", "queue", "set-status", "RQ-0001", "doing"]);
        assert!(result.is_err(), "expected queue set-status to be rejected");
        let msg = result.err().unwrap().to_string().to_lowercase();
        assert!(
            msg.contains("unrecognized") || msg.contains("unexpected") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_parses_run_one_interactive() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "-i"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => assert!(args.interactive),
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_one_interactive_long() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--interactive"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => assert!(args.interactive),
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_loop_interactive() {
        let cli = Cli::try_parse_from(["ralph", "run", "loop", "-i"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::Loop(args) => assert!(args.interactive),
                _ => panic!("expected run loop command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_loop_interactive_long() {
        let cli = Cli::try_parse_from(["ralph", "run", "loop", "--interactive"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::Loop(args) => assert!(args.interactive),
                _ => panic!("expected run loop command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_tui_command() {
        let cli = Cli::try_parse_from(["ralph", "tui"]).expect("parse");
        match cli.command {
            Command::Tui(tui::TuiArgs { .. }) => {}
            _ => panic!("expected tui command"),
        }
    }

    #[test]
    fn cli_parses_tui_read_only() {
        let cli = Cli::try_parse_from(["ralph", "tui", "--read-only"]).expect("parse");
        match cli.command {
            Command::Tui(tui::TuiArgs { read_only, .. }) => {
                assert!(read_only);
            }
            _ => panic!("expected tui command"),
        }
    }

    #[test]
    fn cli_parses_run_loop_interactive_with_max_tasks() {
        let cli =
            Cli::try_parse_from(["ralph", "run", "loop", "-i", "--max-tasks", "3"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::Loop(args) => {
                    assert!(args.interactive);
                    assert_eq!(args.max_tasks, 3);
                }
                _ => panic!("expected run loop command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_one_interactive_with_runner_override() {
        let cli = Cli::try_parse_from([
            "ralph", "run", "one", "-i", "--runner", "opencode", "--model", "gpt-5.2",
        ])
        .expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert!(args.interactive);
                    assert_eq!(args.agent.runner.as_deref(), Some("opencode"));
                    assert_eq!(args.agent.model.as_deref(), Some("gpt-5.2"));
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_tui_with_agent_overrides() {
        let cli = Cli::try_parse_from(["ralph", "tui", "--runner", "claude", "--model", "opus"])
            .expect("parse");
        match cli.command {
            Command::Tui(tui::TuiArgs { read_only, agent }) => {
                assert!(!read_only);
                assert_eq!(agent.runner.as_deref(), Some("claude"));
                assert_eq!(agent.model.as_deref(), Some("opus"));
            }
            _ => panic!("expected tui command"),
        }
    }

    #[test]
    fn cli_parses_tui_read_only_with_agent_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "tui",
            "--read-only",
            "--runner",
            "opencode",
            "--model",
            "gpt-5.2",
        ])
        .expect("parse");
        match cli.command {
            Command::Tui(tui::TuiArgs { read_only, agent }) => {
                assert!(read_only);
                assert_eq!(agent.runner.as_deref(), Some("opencode"));
                assert_eq!(agent.model.as_deref(), Some("gpt-5.2"));
            }
            _ => panic!("expected tui command"),
        }
    }

    #[test]
    fn cli_parses_run_one_interactive_with_include_draft() {
        let cli =
            Cli::try_parse_from(["ralph", "run", "one", "-i", "--include-draft"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert!(args.interactive);
                    assert!(args.agent.include_draft);
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_rejects_run_loop_with_id_flag() {
        let err = Cli::try_parse_from(["ralph", "run", "loop", "--id", "RQ-0001"])
            .err()
            .expect("parse failure");
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_parses_tui_with_repo_prompt_plan() {
        let cli = Cli::try_parse_from(["ralph", "tui", "--repo-prompt", "plan"]).expect("parse");
        match cli.command {
            Command::Tui(tui::TuiArgs { agent, .. }) => {
                assert_eq!(agent.repo_prompt, Some(crate::agent::RepoPromptMode::Plan));
            }
            _ => panic!("expected tui command"),
        }
    }

    #[test]
    fn cli_parses_tui_with_repo_prompt_off() {
        let cli = Cli::try_parse_from(["ralph", "tui", "--repo-prompt", "off"]).expect("parse");
        match cli.command {
            Command::Tui(tui::TuiArgs { agent, .. }) => {
                assert_eq!(agent.repo_prompt, Some(crate::agent::RepoPromptMode::Off));
            }
            _ => panic!("expected tui command"),
        }
    }

    #[test]
    fn cli_parses_run_one_interactive_with_phases() {
        let cli =
            Cli::try_parse_from(["ralph", "run", "one", "-i", "--phases", "2"]).expect("parse");
        match cli.command {
            Command::Run(run::RunArgs { command }) => match command {
                run::RunCommand::One(args) => {
                    assert!(args.interactive);
                    assert_eq!(args.agent.phases, Some(2));
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }
}
