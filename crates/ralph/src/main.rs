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
    // Load .env file, warning on errors but ignoring "not found"
    if let Err(e) = dotenvy::dotenv() {
        // Only warn on non-NotFound errors (e.g., permission denied, parse errors)
        if is_not_found_error(&e) {
            // Silently ignore - no .env file is expected
        } else {
            // Note: Logger isn't initialized yet, use eprintln
            // Redact to avoid accidentally logging secrets from malformed .env files
            let msg = format!("Warning: failed to load .env file: {e}");
            eprintln!("{}", redaction::redact_text(&msg));
        }
    }
    let args = normalize_repo_prompt_args(std::env::args_os());
    let cli = cli::Cli::parse_from(args);

    // Initialize color output settings early, before any colored output
    cli::color::init_color(cli.color, cli.no_color);

    let mut builder = env_logger::Builder::from_default_env();
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
        cli::Command::Run(args) => cli::run::handle_run(args.command, cli.force),
        cli::Command::Task(args) => cli::task::handle_task(*args, cli.force),
        cli::Command::Scan(args) => cli::scan::handle_scan(args, cli.force),
        cli::Command::Init(args) => cli::init::handle_init(args, cli.force),
        cli::Command::App(args) => cli::app::handle_app(args.command),
        cli::Command::Prompt(args) => cli::prompt::handle_prompt(args),
        cli::Command::Doctor(args) => cli::doctor::handle_doctor(args),
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
        cli::Command::Daemon(args) => cli::daemon::handle_daemon(args.command),
        cli::Command::CliSpec(args) => cli::handle_cli_spec(args),
    }
}

/// Check if a dotenvy error is a "file not found" error.
/// This is the only error we silently ignore.
fn is_not_found_error(e: &dotenvy::Error) -> bool {
    use std::io;
    match e {
        dotenvy::Error::Io(io_err) if io_err.kind() == io::ErrorKind::NotFound => true,
        // Also check for the generic "not found" case from dotenvy's internal handling
        _ => {
            let err_str = e.to_string().to_lowercase();
            err_str.contains("not found") || err_str.contains("no such file")
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

#[cfg(test)]
mod tests {
    use super::*;

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
