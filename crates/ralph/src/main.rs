//! Ralph CLI entrypoint and command routing.
//!
//! Responsibilities:
//! - Load environment defaults, parse CLI args, and dispatch to command handlers.
//! - Initialize logging/redaction and apply CLI-level behavior toggles.
//!
//! Not handled here:
//! - CLI flag definitions (see `crate::cli`).
//! - Queue persistence, prompt rendering, or runner execution.
//!
//! Invariants/assumptions:
//! - CLI arguments are normalized before Clap parsing.
//! - Command handlers enforce their own safety checks and validation.

use anyhow::{Context, Result};
use clap::Parser;
use ralph::{cli, redaction, sanity};
use std::ffi::OsString;

fn main() {
    if let Err(err) = run() {
        use colored::Colorize;
        let msg = format!("{:#}", err);
        let redacted = redaction::redact_text(&msg);
        eprintln!("{} {}", "Error:".red().bold(), redacted);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = normalize_repo_prompt_args(std::env::args_os());
    let cli = cli::Cli::parse_from(args);

    // Initialize color output settings early, before any colored output
    cli::color::init_color(cli.color, cli.no_color);

    let mut builder = env_logger::Builder::from_default_env();
    if suppress_terminal_logs(&cli.command) {
        builder.target(env_logger::Target::Pipe(Box::new(std::io::sink())));
    }
    if cli.verbose {
        builder.filter_level(log::LevelFilter::Debug);
    } else if std::env::var("RUST_LOG").is_err() {
        builder.filter_level(log::LevelFilter::Info);
    }

    // We want to capture the max level *before* we consume the builder into a logger,
    // but env_logger::Builder doesn't expose it easily after build.
    // However, we can set the global max level ourselves after init if we knew it.
    // A simpler approach with env_logger 0.11+ is to let it parse env vars, then build.
    // But `builder.init()` consumes the builder and sets the logger.
    // We need `builder.build()` to get the logger, then wrap it.
    let logger = builder.build();
    let max_level = logger.filter();
    redaction::RedactedLogger::init(Box::new(logger), max_level)
        .context("initialize redacted logger")?;

    // Run sanity checks before commands that need them
    let should_run_sanity = sanity::should_run_sanity_checks(&cli.command);
    if should_run_sanity && !cli.no_sanity_checks {
        let resolved = ralph::config::resolve_from_cwd_for_doctor()?;
        // Extract non_interactive flag from run commands
        let non_interactive = match &cli.command {
            cli::Command::Run(run_args) => match &run_args.command {
                cli::run::RunCommand::One(one_args) => one_args.non_interactive,
                cli::run::RunCommand::Loop(loop_args) => loop_args.non_interactive,
                cli::run::RunCommand::Resume(resume_args) => resume_args.non_interactive,
            },
            _ => false,
        };
        let options = sanity::SanityOptions {
            auto_fix: cli.auto_fix,
            skip: false,
            non_interactive,
        };
        let sanity_result = sanity::run_sanity_checks(&resolved, &options)?;

        // If there are issues that need attention and we're not in auto-fix mode,
        // we might want to warn the user
        if !sanity::report_sanity_results(&sanity_result, cli.auto_fix) {
            anyhow::bail!(
                "Sanity checks failed. Please resolve the issues above or run with --auto-fix."
            );
        }
    }

    match cli.command {
        cli::Command::Queue(args) => cli::queue::handle_queue(args.command, cli.force),
        cli::Command::Config(args) => cli::config::handle_config(args.command),
        cli::Command::Run(args) => cli::run::handle_run(args.command, cli.force, cli.no_progress),
        cli::Command::Task(args) => cli::task::handle_task(*args, cli.force),
        cli::Command::Scan(args) => cli::scan::handle_scan(args, cli.force),
        cli::Command::Init(args) => cli::init::handle_init(args, cli.force),
        cli::Command::Prompt(args) => cli::prompt::handle_prompt(args),
        cli::Command::Doctor(args) => cli::doctor::handle_doctor(args),
        cli::Command::Tui(args) => {
            cli::tui::handle_tui(args, cli.color, cli.force, cli.no_progress)
        }
        cli::Command::Context(args) => cli::context::handle_context(args),
        cli::Command::Prd(args) => cli::prd::handle_prd(args, cli.force),
        cli::Command::Completions(args) => cli::completions::handle_completions(args),
        cli::Command::Migrate(args) => cli::migrate::handle_migrate(args),
        cli::Command::Version(args) => cli::version::handle_version(args),
        cli::Command::Watch(args) => cli::watch::handle_watch(args, cli.force),
        cli::Command::Webhook(args) => {
            let resolved = ralph::config::resolve_from_cwd()?;
            cli::webhook::handle_webhook(&args, &resolved)
        }
        cli::Command::Productivity(args) => cli::productivity::handle(args),
        cli::Command::Plugin(args) => {
            let resolved = ralph::config::resolve_from_cwd()?;
            ralph::commands::plugin::run(&args, &resolved)
        }
    }
}

fn normalize_repo_prompt_args<I>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = OsString>,
{
    let mut normalized = Vec::new();
    let mut passthrough = false;

    for arg in args {
        if passthrough {
            normalized.push(arg);
            continue;
        }

        if arg == std::ffi::OsStr::new("--") {
            passthrough = true;
            normalized.push(arg);
            continue;
        }

        let as_str = arg.to_str();
        if as_str == Some("-rp") {
            normalized.push(OsString::from("--repo-prompt"));
            continue;
        }
        if let Some(value) = as_str.and_then(|s| s.strip_prefix("-rp=")) {
            let mut rewritten = OsString::from("--repo-prompt=");
            rewritten.push(value);
            normalized.push(rewritten);
            continue;
        }

        normalized.push(arg);
    }

    normalized
}

fn suppress_terminal_logs(command: &cli::Command) -> bool {
    match command {
        cli::Command::Tui(_) => true,
        cli::Command::Run(args) => match &args.command {
            cli::run::RunCommand::One(run_args) => run_args.interactive,
            cli::run::RunCommand::Loop(run_args) => run_args.interactive,
            cli::run::RunCommand::Resume(_) => false,
        },
        cli::Command::Doctor(_) => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppress_terminal_logs_for_tui() {
        let cmd = cli::Command::Tui(cli::tui::TuiArgs {
            read_only: false,
            no_mouse: false,
            ascii_borders: false,
            agent: ralph::agent::RunAgentArgs::default(),
        });
        assert!(suppress_terminal_logs(&cmd));
    }

    #[test]
    fn suppress_terminal_logs_for_interactive_run_one() {
        let cmd = cli::Command::Run(cli::run::RunArgs {
            command: cli::run::RunCommand::One(cli::run::RunOneArgs {
                interactive: true,
                debug: false,
                id: None,
                visualize: false,
                non_interactive: false,
                parallel_worker: false,
                agent: ralph::agent::RunAgentArgs::default(),
            }),
        });
        assert!(suppress_terminal_logs(&cmd));
    }

    #[test]
    fn suppress_terminal_logs_for_interactive_run_loop() {
        let cmd = cli::Command::Run(cli::run::RunArgs {
            command: cli::run::RunCommand::Loop(cli::run::RunLoopArgs {
                max_tasks: 0,
                interactive: true,
                debug: false,
                visualize: false,
                resume: false,
                non_interactive: false,
                parallel: None,
                agent: ralph::agent::RunAgentArgs::default(),
            }),
        });
        assert!(suppress_terminal_logs(&cmd));
    }

    #[test]
    fn no_log_suppression_for_non_interactive_commands() {
        let cmd = cli::Command::Run(cli::run::RunArgs {
            command: cli::run::RunCommand::One(cli::run::RunOneArgs {
                interactive: false,
                debug: false,
                id: Some("RQ-0001".to_string()),
                visualize: false,
                non_interactive: false,
                parallel_worker: false,
                agent: ralph::agent::RunAgentArgs::default(),
            }),
        });
        assert!(!suppress_terminal_logs(&cmd));
    }

    #[test]
    fn normalize_repo_prompt_args_rewrites_short_flag() {
        let args = vec![
            OsString::from("ralph"),
            OsString::from("-rp"),
            OsString::from("plan"),
        ];
        let normalized = normalize_repo_prompt_args(args);
        assert_eq!(
            normalized,
            vec![
                OsString::from("ralph"),
                OsString::from("--repo-prompt"),
                OsString::from("plan")
            ]
        );
    }

    #[test]
    fn normalize_repo_prompt_args_rewrites_equals_form() {
        let args = vec![OsString::from("ralph"), OsString::from("-rp=tools")];
        let normalized = normalize_repo_prompt_args(args);
        assert_eq!(
            normalized,
            vec![
                OsString::from("ralph"),
                OsString::from("--repo-prompt=tools")
            ]
        );
    }

    #[test]
    fn normalize_repo_prompt_args_respects_double_dash() {
        let args = vec![
            OsString::from("ralph"),
            OsString::from("--"),
            OsString::from("-rp"),
            OsString::from("plan"),
        ];
        let normalized = normalize_repo_prompt_args(args);
        assert_eq!(
            normalized,
            vec![
                OsString::from("ralph"),
                OsString::from("--"),
                OsString::from("-rp"),
                OsString::from("plan")
            ]
        );
    }
}
