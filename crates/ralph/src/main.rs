//! Ralph CLI entrypoint and command routing.

use anyhow::{Context, Result};
use clap::Parser;
use ralph::{cli, redaction};

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
    let cli = cli::Cli::parse();

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

    match cli.command {
        cli::Command::Queue(args) => cli::queue::handle_queue(args.command, cli.force),
        cli::Command::Config(args) => cli::config::handle_config(args.command),
        cli::Command::Run(args) => cli::run::handle_run(args.command, cli.force),
        cli::Command::Task(args) => cli::task::handle_task(args.command, cli.force),
        cli::Command::Scan(args) => cli::scan::handle_scan(args, cli.force),
        cli::Command::Init(args) => cli::init::handle_init(args, cli.force),
        cli::Command::Prompt(args) => cli::prompt::handle_prompt(args),
        cli::Command::Doctor => cli::doctor::handle_doctor(),
    }
}
