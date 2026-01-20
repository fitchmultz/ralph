//! Ralph CLI entrypoint and command routing.

mod contracts;

mod config;
mod doctor_cmd;
mod fsutil;
mod init_cmd;
mod outpututil;
mod queue;
mod redaction;
mod reports;
mod run_cmd;
mod runutil;
mod timeutil;

mod gitutil;
mod prompts;
mod runner;
mod scan_cmd;
mod task_cmd;
mod tui;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::contracts::{QueueFile, Runner as RunnerKind, Task, TaskStatus};

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
    let cli = Cli::parse();

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
        Command::Queue(args) => handle_queue(args.command, cli.force),
        Command::Config(args) => handle_config(args.command),
        Command::Run(args) => handle_run(args.command, cli.force),
        Command::Task(args) => handle_task(args.command, cli.force),
        Command::Scan(args) => handle_scan(args, cli.force),
        Command::Init(args) => handle_init(args, cli.force),
        Command::Doctor => handle_doctor(),
    }
}

fn load_and_validate_queues(
    resolved: &config::Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;

    let done_file = if include_done {
        Some(queue::load_queue_or_default(&resolved.done_path)?)
    } else {
        None
    };

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    if let Some(d) = done_ref {
        queue::validate_queue_set(&queue_file, Some(d), &resolved.id_prefix, resolved.id_width)?;
    } else {
        queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
    }

    Ok((queue_file, done_file))
}

fn handle_queue(cmd: QueueCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        QueueCommand::Validate => {
            load_and_validate_queues(&resolved, true)?;
        }
        QueueCommand::Next(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            if let Some(next) = queue::next_todo_task(&queue_file) {
                if args.with_title {
                    println!(
                        "{}",
                        outpututil::format_task_id_title(&next.id, &next.title)
                    );
                } else {
                    println!("{}", outpututil::format_task_id(&next.id));
                }
                return Ok(());
            }

            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            let next_id = queue::next_id_across(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            println!("{next_id}");
        }
        QueueCommand::NextId => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            let next = queue::next_id_across(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            println!("{next}");
        }
        QueueCommand::Show(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

            let task = queue::find_task_across(&queue_file, done_ref, &args.task_id)
                .ok_or_else(|| anyhow::anyhow!("task not found: {}", args.task_id.trim()))?;

            match args.format {
                QueueShowFormat::Json => {
                    let rendered = serde_json::to_string_pretty(task)?;
                    print!("{rendered}");
                }
                QueueShowFormat::Compact => {
                    println!("{}", outpututil::format_task_compact(task));
                }
            }
        }
        QueueCommand::List(args) => {
            if args.include_done && args.only_done {
                bail!("Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only show done tasks.");
            }

            let (queue_file, done_file) =
                load_and_validate_queues(&resolved, args.include_done || args.only_done)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

            let statuses: Vec<TaskStatus> = args.status.into_iter().map(|s| s.into()).collect();
            let limit = resolve_list_limit(args.limit, args.all);

            let mut tasks: Vec<&Task> = Vec::new();
            if !args.only_done {
                tasks.extend(queue::filter_tasks(
                    &queue_file,
                    &statuses,
                    &args.tag,
                    &args.scope,
                    None,
                ));
            }
            if args.include_done || args.only_done {
                if let Some(done_ref) = done_ref {
                    tasks.extend(queue::filter_tasks(
                        done_ref,
                        &statuses,
                        &args.tag,
                        &args.scope,
                        None,
                    ));
                }
            }

            // Apply dependency filter if specified
            let tasks = if let Some(ref root_id) = args.filter_deps {
                let dependents_list = queue::get_dependents(root_id, &queue_file, done_ref);
                let dependents: std::collections::HashSet<&str> =
                    dependents_list.iter().map(|s| s.as_str()).collect();
                tasks
                    .into_iter()
                    .filter(|t| dependents.contains(t.id.trim()))
                    .collect()
            } else {
                tasks
            };

            // Apply sort if specified
            let tasks = if let Some(ref sort_by) = args.sort_by {
                match sort_by.as_str() {
                    "priority" => {
                        let mut sorted = tasks;
                        sorted.sort_by(|a, b| {
                            // Since Ord has Critical > High > Medium > Low (semantically),
                            // we reverse for descending to put higher priority first
                            let ord = if args.descending {
                                a.priority.cmp(&b.priority).reverse()
                            } else {
                                a.priority.cmp(&b.priority)
                            };
                            match ord {
                                std::cmp::Ordering::Equal => a.id.cmp(&b.id),
                                other => other,
                            }
                        });
                        sorted
                    }
                    _ => tasks,
                }
            } else {
                tasks
            };

            let max = limit.unwrap_or(usize::MAX);
            for task in tasks.into_iter().take(max) {
                match args.format {
                    QueueListFormat::Compact => {
                        println!("{}", outpututil::format_task_compact(task))
                    }
                    QueueListFormat::Long => println!("{}", outpututil::format_task_detailed(task)),
                }
            }
        }
        QueueCommand::Search(args) => {
            if args.include_done && args.only_done {
                bail!("Conflicting flags: --include-done and --only-done are mutually exclusive. Choose either to include done tasks or to only search done tasks.");
            }

            let (queue_file, done_file) =
                load_and_validate_queues(&resolved, args.include_done || args.only_done)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

            let statuses: Vec<TaskStatus> = args.status.into_iter().map(|s| s.into()).collect();

            // Pre-filter by status/tag/scope using filter_tasks
            let mut prefiltered: Vec<&Task> = Vec::new();
            if !args.only_done {
                prefiltered.extend(queue::filter_tasks(
                    &queue_file,
                    &statuses,
                    &args.tag,
                    &args.scope,
                    None,
                ));
            }
            if args.include_done || args.only_done {
                if let Some(done_ref) = done_ref {
                    prefiltered.extend(queue::filter_tasks(
                        done_ref,
                        &statuses,
                        &args.tag,
                        &args.scope,
                        None,
                    ));
                }
            }

            // Apply content search
            let results = queue::search_tasks(
                prefiltered.into_iter(),
                &args.query,
                args.regex,
                args.match_case,
            )?;

            let limit = resolve_list_limit(args.limit, args.all);
            let max = limit.unwrap_or(usize::MAX);
            for task in results.into_iter().take(max) {
                match args.format {
                    QueueListFormat::Compact => {
                        println!("{}", outpututil::format_task_compact(task))
                    }
                    QueueListFormat::Long => println!("{}", outpututil::format_task_detailed(task)),
                }
            }
        }
        QueueCommand::Done => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue done", force)?;
            let report = queue::archive_done_tasks(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            if report.moved_ids.is_empty() {
                log::info!("No done tasks to move.");
            } else {
                log::info!("Moved {} done task(s).", report.moved_ids.len());
            }
        }
        QueueCommand::Unlock => {
            let lock_dir = fsutil::queue_lock_dir(&resolved.repo_root);
            if lock_dir.exists() {
                std::fs::remove_dir_all(&lock_dir)
                    .with_context(|| format!("remove lock dir {}", lock_dir.display()))?;
                log::info!("Queue unlocked (removed {}).", lock_dir.display());
            } else {
                log::info!("Queue is not locked.");
            }
        }
        QueueCommand::SetStatus {
            task_id,
            status,
            note,
        } => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue set-status", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(
                &mut queue_file,
                &task_id,
                status.into(),
                &now,
                note.as_deref(),
            )?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
        }
        QueueCommand::SetField {
            task_id,
            key,
            value,
        } => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue set-field", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_field(&mut queue_file, &task_id, &key, &value, &now)?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!("Set field '{}' on task {}.", key, task_id);
        }
        QueueCommand::Sort(args) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue sort", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            match args.sort_by.as_str() {
                "priority" => {
                    queue::sort_tasks_by_priority(&mut queue_file, args.descending);
                }
                _ => {
                    bail!(
                        "Unsupported sort field: {}. Supported fields: priority",
                        args.sort_by
                    );
                }
            }

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!(
                "Queue sorted by {} (descending: {}).",
                args.sort_by,
                args.descending
            );
        }
        QueueCommand::Stats(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            reports::print_stats(&queue_file, done_ref, &args.tag)?;
        }
        QueueCommand::History(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            reports::print_history(&queue_file, done_ref, args.days)?;
        }
        QueueCommand::Burndown(args) => {
            let (queue_file, done_file) = load_and_validate_queues(&resolved, true)?;
            let done_ref = done_file
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            reports::print_burndown(&queue_file, done_ref, args.days)?;
        }
        QueueCommand::Schema => {
            let schema = schemars::schema_for!(contracts::QueueFile);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
    }
    Ok(())
}

fn handle_config(cmd: ConfigCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        ConfigCommand::Show => {
            let rendered = serde_json::to_string_pretty(&resolved.config)?;
            print!("{rendered}");
        }
        ConfigCommand::Paths => {
            println!("repo_root: {}", resolved.repo_root.display());
            println!("queue: {}", resolved.queue_path.display());
            println!("done: {}", resolved.done_path.display());
            if let Some(path) = resolved.global_config_path.as_ref() {
                println!("global_config: {}", path.display());
            } else {
                println!("global_config: (unavailable)");
            }
            if let Some(path) = resolved.project_config_path.as_ref() {
                println!("project_config: {}", path.display());
            } else {
                println!("project_config: (unavailable)");
            }
        }
        ConfigCommand::Schema => {
            let schema = schemars::schema_for!(contracts::Config);
            println!("{}", serde_json::to_string_pretty(&schema)?);
        }
    }
    Ok(())
}

fn handle_init(args: InitArgs, force_lock: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let report = init_cmd::run_init(
        &resolved,
        init_cmd::InitOptions {
            force: args.force,
            force_lock,
        },
    )?;

    fn report_status(label: &str, status: init_cmd::FileInitStatus, path: &std::path::Path) {
        match status {
            init_cmd::FileInitStatus::Created => {
                log::info!("{}: created ({})", label, path.display())
            }
            init_cmd::FileInitStatus::Valid => {
                log::info!("{}: exists (valid) ({})", label, path.display())
            }
        }
    }

    report_status("queue", report.queue_status, &resolved.queue_path);
    report_status("done", report.done_status, &resolved.done_path);
    if let Some(status) = report.readme_status {
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        report_status("readme", status, &readme_path);
    }
    if let Some(path) = resolved.project_config_path.as_ref() {
        report_status("config", report.config_status, path);
    } else {
        log::info!("config: unavailable");
    }
    Ok(())
}

fn handle_doctor() -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    doctor_cmd::run_doctor(&resolved)
}

fn handle_run(cmd: RunCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        RunCommand::One(args) => {
            if args.interactive {
                let overrides = resolve_run_agent_overrides(&args.agent)?;
                // Capture the values we need by moving them into the factory
                let resolved_clone = resolved.clone();
                let runner_factory = move |task_id: String, handler: runner::OutputHandler| {
                    let resolved = resolved_clone.clone();
                    let overrides = overrides.clone();
                    let force = force;
                    move || {
                        run_cmd::run_one_with_id(
                            &resolved,
                            &overrides,
                            force,
                            &task_id,
                            Some(handler),
                        )
                    }
                };
                // Tasks are executed within TUI, run_tui returns None
                let _ = tui::run_tui(&resolved.queue_path, runner_factory)?;
                Ok(())
            } else {
                let overrides = resolve_run_agent_overrides(&args.agent)?;
                let _ = run_cmd::run_one(&resolved, &overrides, force)?;
                Ok(())
            }
        }
        RunCommand::Loop(args) => {
            if args.interactive {
                let overrides = resolve_run_agent_overrides(&args.agent)?;
                // Capture the values we need by moving them into the factory
                let resolved_clone = resolved.clone();
                let runner_factory = move |task_id: String, handler: runner::OutputHandler| {
                    let resolved = resolved_clone.clone();
                    let overrides = overrides.clone();
                    let force = force;
                    move || {
                        run_cmd::run_one_with_id(
                            &resolved,
                            &overrides,
                            force,
                            &task_id,
                            Some(handler),
                        )
                    }
                };
                // Tasks are executed within TUI, run_tui returns None
                let _ = tui::run_tui(&resolved.queue_path, runner_factory)?;
                Ok(())
            } else {
                let overrides = resolve_run_agent_overrides(&args.agent)?;
                run_cmd::run_loop(
                    &resolved,
                    run_cmd::RunLoopOptions {
                        max_tasks: args.max_tasks,
                        agent_overrides: overrides,
                        force,
                    },
                )
            }
        }
    }
}

fn handle_task(cmd: TaskCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        TaskCommand::Build(args) => {
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let settings = resolve_agent_args(
                &resolved,
                args.runner.as_deref(),
                args.model.as_deref(),
                args.effort.as_deref(),
            )?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner: settings.runner,
                    model: settings.model,
                    reasoning_effort: settings.reasoning_effort,
                    force,
                },
            )
        }
    }
}

fn handle_scan(args: ScanArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let settings = resolve_agent_args(
        &resolved,
        args.runner.as_deref(),
        args.model.as_deref(),
        args.effort.as_deref(),
    )?;

    scan_cmd::run_scan(
        &resolved,
        scan_cmd::ScanOptions {
            focus: args.focus,
            runner: settings.runner,
            model: settings.model,
            reasoning_effort: settings.reasoning_effort,
            force,
        },
    )
}

fn parse_runner(value: &str) -> Result<RunnerKind> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "codex" => Ok(RunnerKind::Codex),
        "opencode" => Ok(RunnerKind::Opencode),
        "gemini" => Ok(RunnerKind::Gemini),
        "claude" => Ok(RunnerKind::Claude),
        _ => bail!(
            "Invalid runner: --runner must be 'codex', 'opencode', 'gemini', or 'claude' (got: {}). Set a supported runner in .ralph/config.yaml or via the --runner flag.",
            value.trim()
        ),
    }
}

fn resolve_run_agent_overrides(args: &RunAgentArgs) -> Result<run_cmd::AgentOverrides> {
    let runner = match args.runner.as_deref() {
        Some(value) => Some(parse_runner(value)?),
        None => None,
    };

    let model = match args.model.as_deref() {
        Some(value) => Some(runner::parse_model(value)?),
        None => None,
    };

    let reasoning_effort = match args.effort.as_deref() {
        Some(value) => Some(runner::parse_reasoning_effort(value)?),
        None => None,
    };

    if let (Some(runner_kind), Some(model)) = (runner, model.as_ref()) {
        runner::validate_model_for_runner(runner_kind, model)?;
    }

    Ok(run_cmd::AgentOverrides {
        runner,
        model,
        reasoning_effort,
    })
}

fn resolve_agent_args(
    resolved: &config::Resolved,
    runner_override: Option<&str>,
    model_override: Option<&str>,
    effort_override: Option<&str>,
) -> Result<runner::AgentSettings> {
    let runner_kind = match runner_override {
        Some(value) => Some(parse_runner(value)?),
        None => None,
    };

    let model = match model_override {
        Some(value) => Some(runner::parse_model(value)?),
        None => None,
    };

    let effort = match effort_override {
        Some(value) => Some(runner::parse_reasoning_effort(value)?),
        None => None,
    };

    runner::resolve_agent_settings(runner_kind, model, effort, None, &resolved.config.agent)
}

fn resolve_list_limit(limit: u32, all: bool) -> Option<usize> {
    if all || limit == 0 {
        None
    } else {
        Some(limit as usize)
    }
}

#[derive(Parser)]
#[command(name = "ralph")]
#[command(about = "Ralph")]
#[command(
    after_long_help = "Runner selection:\n  - CLI flags override project config, which overrides global config, which overrides built-in defaults.\n  - Default runner/model come from config files: project config (.ralph/config.yaml) > global config (~/.config/ralph/config.yaml) > built-in.\n  - `task build` and `scan` accept --runner/--model/--effort as one-off overrides.\n  - `run one` and `run loop` accept --runner/--model/--effort as one-off overrides; otherwise they use task.agent overrides when present; otherwise config agent defaults.\n\nConfig example (.ralph/config.yaml):\n  version: 1\n  agent:\n    runner: opencode\n    model: gpt-5.2\n    opencode_bin: opencode\n    gemini_bin: gemini\n    claude_bin: claude\n\nNotes:\n  - Allowed runners: codex, opencode, gemini, claude\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude accept arbitrary model ids)\n\nExamples:\n  ralph queue list\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI gaps\"\n  ralph task build --runner codex --model gpt-5.2-codex --effort high \"Fix the flaky test\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph scan --runner claude --model sonnet --focus \"risk audit\"\n  ralph task build --runner claude --model opus \"Add tests for X\"\n  ralph run one"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Force operations (e.g., bypass stale queue locks).
    #[arg(long, global = true)]
    force: bool,

    /// Increase output verbosity (sets log level to info).
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    Queue(QueueArgs),
    Config(ConfigArgs),
    Run(RunArgs),
    Task(TaskArgs),
    Scan(ScanArgs),
    Init(InitArgs),
    /// Verify environment readiness and configuration.
    #[command(after_long_help = "Example:\n  ralph doctor")]
    Doctor,
}

#[derive(Args)]
#[command(
    about = "Inspect and manage the task queue",
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph queue next-id\n  ralph queue set-status RQ-0001 doing --note \"Starting work\""
)]
struct QueueArgs {
    #[command(subcommand)]
    command: QueueCommand,
}

#[derive(Args)]
#[command(
    about = "Inspect and manage Ralph configuration",
    after_long_help = "Examples:\n  ralph config show\n  ralph config paths"
)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Args)]
#[command(
    about = "Run the Ralph supervisor (executes queued tasks via codex/opencode/gemini)",
    after_long_help = "Runner selection:\n  - `ralph run` selects runner/model/effort with this precedence:\n      1) CLI overrides (flags on `run one` / `run loop`)\n      2) the task's `agent` override (if present in .ralph/queue.json)\n      3) otherwise the resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\nNotes:\n  - Allowed runners: codex, opencode, gemini\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini accept arbitrary model ids)\n  - `--effort` is codex-only and is ignored for opencode.\n\nTo change defaults for this repo, edit .ralph/config.json:\n  version: 1\n  agent:\n    runner: opencode\n    model: gpt-5.2\n    gemini_bin: gemini\n\nExamples:\n  ralph run one\n  ralph run one --runner opencode --model gpt-5.2\n  ralph run one --runner codex --model gpt-5.2-codex --effort high\n  ralph run one --runner gemini --model gemini-3-flash-preview\n  ralph run loop --max-tasks 0\n  ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2"
)]
struct RunArgs {
    #[command(subcommand)]
    command: RunCommand,
}

#[derive(Args)]
#[command(
    about = "Create and build tasks from freeform requests",
    after_long_help = "Examples:\n  ralph task build \"Add tests for the new queue logic\"\n  ralph task build --runner opencode --model gpt-5.2 \"Fix CLI help strings\""
)]
struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Args)]
#[command(
    about = "Bootstrap Ralph files in the current repository",
    after_long_help = "Examples:\n  ralph init\n  ralph init --force"
)]
struct InitArgs {
    /// Overwrite existing files if they already exist.
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Build a new task from a natural language request.
    #[command(
        after_long_help = "Example:\n  ralph task build \"Add tests for the new queue logic\""
    )]
    Build(TaskBuildArgs),
}

#[derive(Args)]
#[command(
    after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n\nExamples:\n  ralph task build \"Add integration tests for run one\"\n  ralph task build --runner opencode --model gpt-5.2 \"Add docs for OpenCode setup\"\n  ralph task build --runner gemini --model gemini-3-flash-preview \"Draft risk checklist\"\n  ralph task build --runner codex --model gpt-5.2-codex --effort high \"Fix queue validation\"\n  echo \"Triage flaky CI\" | ralph task build --runner codex --model gpt-5.2-codex --effort medium"
)]
struct TaskBuildArgs {
    /// Freeform request text; if omitted, reads from stdin.
    #[arg(value_name = "REQUEST")]
    request: Vec<String>,

    /// Optional hint tags (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    tags: String,

    /// Optional hint scope (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    scope: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(long)]
    effort: Option<String>,
}

#[derive(Args)]
#[command(
    about = "Scan the repository for new tasks and focus areas",
    after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n\nExamples:\n  ralph scan --focus \"production readiness gaps\"\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI and safety gaps\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph scan --runner codex --model gpt-5.2-codex --effort high --focus \"queue correctness\""
)]
struct ScanArgs {
    /// Optional focus prompt to guide the scan.
    #[arg(long, default_value = "")]
    focus: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(long)]
    effort: Option<String>,
}

#[derive(Subcommand)]
enum QueueCommand {
    /// Validate the active queue (and done archive if present).
    #[command(after_long_help = "Example:\n  ralph queue validate")]
    Validate,
    /// Print the next todo task (ID by default).
    #[command(after_long_help = "Examples:\n  ralph queue next\n  ralph queue next --with-title")]
    Next(QueueNextArgs),
    /// Print the next available task ID (across queue + done archive).
    #[command(after_long_help = "Example:\n  ralph queue next-id")]
    NextId,
    /// Show a task by ID.
    Show(QueueShowArgs),
    /// List tasks in queue order.
    List(QueueListArgs),
    /// Search tasks by content (title, evidence, plan, notes).
    #[command(
        after_long_help = "Examples:\n  ralph queue search \"authentication\"\n  ralph queue search \"RQ-\\d{4}\" --regex\n  ralph queue search \"TODO\" --match-case\n  ralph queue search \"fix\" --status todo --tag rust"
    )]
    Search(QueueSearchArgs),
    /// Move completed tasks from queue.json to done.json.
    #[command(after_long_help = "Example:\n  ralph queue done")]
    Done,
    /// Remove the queue lock file.
    #[command(after_long_help = "Example:\n  ralph queue unlock")]
    Unlock,
    /// Update a task status in the active queue.
    #[command(
        after_long_help = "Example:\n  ralph queue set-status RQ-0001 doing --note \"Starting work\""
    )]
    SetStatus {
        task_id: String,
        status: StatusArg,
        #[arg(long)]
        note: Option<String>,
    },
    /// Set a custom field on a task.
    #[command(
        after_long_help = "Examples:\n  ralph queue set-field RQ-0001 severity high\n  ralph queue set-field RQ-0002 complexity \"O(n log n)\""
    )]
    SetField {
        task_id: String,
        /// Custom field key (must not contain whitespace).
        key: String,
        /// Custom field value.
        value: String,
    },
    /// Sort tasks by priority (reorders the queue file).
    #[command(after_long_help = "Examples:\n  ralph queue sort\n  ralph queue sort --descending")]
    Sort(QueueSortArgs),
    /// Show task statistics (completion rate, avg duration, tag breakdown).
    #[command(
        after_long_help = "Examples:\n  ralph queue stats\n  ralph queue stats --tag rust --tag cli"
    )]
    Stats(QueueStatsArgs),
    /// Show task history timeline (creation/completion events by day).
    #[command(
        after_long_help = "Examples:\n  ralph queue history\n  ralph queue history --days 14"
    )]
    History(QueueHistoryArgs),
    /// Show burndown chart of remaining tasks over time.
    #[command(
        after_long_help = "Examples:\n  ralph queue burndown\n  ralph queue burndown --days 30"
    )]
    Burndown(QueueBurndownArgs),
    /// Print the JSON schema for the queue file.
    #[command(after_long_help = "Example:\n  ralph queue schema")]
    Schema,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Show the resolved Ralph configuration (YAML).
    #[command(after_long_help = "Example:\n  ralph config show")]
    Show,
    /// Print paths to the queue, done archive, and config files.
    #[command(after_long_help = "Example:\n  ralph config paths")]
    Paths,
    /// Print the JSON schema for the configuration.
    #[command(after_long_help = "Example:\n  ralph config schema")]
    Schema,
}

#[derive(Subcommand)]
enum RunCommand {
    #[command(
        about = "Run exactly one task (the first todo in .ralph/queue.json)",
        after_long_help = "Runner selection (precedence):\n  1) CLI overrides (--runner/--model/--effort)\n  2) task.agent in .ralph/queue.json (if present)\n  3) config defaults (.ralph/config.json then ~/.config/ralph/config.json)\n\nExamples:\n  ralph run one\n  ralph run one --runner opencode --model gpt-5.2\n  ralph run one --runner gemini --model gemini-3-flash-preview\n  ralph run one --runner codex --model gpt-5.2-codex --effort high\n  ralph queue next --with-title"
    )]
    One(RunOneArgs),
    #[command(
        about = "Run tasks repeatedly until no todo remain (or --max-tasks is reached)",
        after_long_help = "Examples:\n  ralph run loop --max-tasks 0\n  ralph run loop --max-tasks 3\n  ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2"
    )]
    Loop(RunLoopArgs),
}

#[derive(Args, Clone, Debug, Default)]
struct RunAgentArgs {
    /// Runner override for this invocation (codex, opencode, gemini). Overrides task.agent and config.
    #[arg(long)]
    runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview (codex supports only gpt-5.2-codex/gpt-5.2; opencode/gemini accept arbitrary model ids).
    #[arg(long)]
    model: Option<String>,

    /// Codex reasoning effort override (minimal, low, medium, high). Ignored for opencode and gemini.
    #[arg(long)]
    effort: Option<String>,
}

#[derive(Args)]
struct RunOneArgs {
    /// Launch interactive TUI mode for task selection and management.
    #[arg(short = 'i', long)]
    interactive: bool,

    #[command(flatten)]
    agent: RunAgentArgs,
}

#[derive(Args)]
struct RunLoopArgs {
    /// Maximum tasks to run before stopping (0 = no limit).
    #[arg(long, default_value_t = 0)]
    max_tasks: u32,

    /// Launch interactive TUI mode for task selection and management.
    #[arg(short = 'i', long)]
    interactive: bool,

    #[command(flatten)]
    agent: RunAgentArgs,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum StatusArg {
    /// Task is waiting to be started.
    Todo,
    /// Task is currently being worked on.
    Doing,
    /// Task is complete.
    Done,
    /// Task was rejected (dependents can proceed).
    Rejected,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum QueueShowFormat {
    /// Full JSON representation of the task.
    Json,
    /// Compact tab-separated summary (ID, status, title).
    Compact,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum QueueListFormat {
    /// Compact tab-separated summary (ID, status, title).
    Compact,
    /// Detailed tab-separated format including tags, scope, and timestamps.
    Long,
}

#[derive(Args)]
#[command(after_long_help = "Example:\n  ralph queue next --with-title")]
struct QueueNextArgs {
    /// Include the task title after the ID.
    #[arg(long)]
    with_title: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue show RQ-0001\n  ralph queue show RQ-0001 --format compact"
)]
struct QueueShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Json)]
    format: QueueShowFormat,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue list --status doing --scope crates/ralph\n  ralph queue list --include-done --limit 20\n  ralph queue list --only-done --all\n  ralph queue list --filter-deps=RQ-0100"
)]
struct QueueListArgs {
    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    status: Vec<StatusArg>,

    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    tag: Vec<String>,

    /// Filter by scope token (repeatable, case-insensitive; substring match).
    #[arg(long)]
    scope: Vec<String>,

    /// Filter by tasks that depend on the given task ID (recursively).
    #[arg(long)]
    filter_deps: Option<String>,

    /// Include tasks from .ralph/done.yaml after active queue output.
    #[arg(long)]
    include_done: bool,

    /// Only list tasks from .ralph/done.yaml (ignores active queue).
    #[arg(long)]
    only_done: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueListFormat::Compact)]
    format: QueueListFormat,

    /// Maximum tasks to show (0 = no limit).
    #[arg(long, default_value_t = 50)]
    limit: u32,

    /// Show all tasks (ignores --limit).
    #[arg(long)]
    all: bool,

    /// Sort by field (e.g., priority).
    #[arg(long)]
    sort_by: Option<String>,

    /// Sort in descending order.
    #[arg(long)]
    descending: bool,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph queue sort\n  ralph queue sort --descending")]
struct QueueSortArgs {
    /// Sort by field (default: priority).
    #[arg(long, default_value = "priority")]
    sort_by: String,

    /// Sort in descending order (highest priority first).
    #[arg(long)]
    descending: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue search \"authentication\"\n  ralph queue search \"RQ-\\d{4}\" --regex\n  ralph queue search \"TODO\" --match-case\n  ralph queue search \"fix\" --status todo --tag rust"
)]
struct QueueSearchArgs {
    /// Search query (substring or regex pattern).
    #[arg(value_name = "QUERY")]
    query: String,

    /// Interpret query as a regular expression.
    #[arg(long)]
    regex: bool,

    /// Case-sensitive search (default: case-insensitive).
    #[arg(long)]
    match_case: bool,

    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    status: Vec<StatusArg>,

    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    tag: Vec<String>,

    /// Filter by scope token (repeatable, case-insensitive; substring match).
    #[arg(long)]
    scope: Vec<String>,

    /// Include tasks from .ralph/done.yaml in search.
    #[arg(long)]
    include_done: bool,

    /// Only search tasks in .ralph/done.yaml (ignores active queue).
    #[arg(long)]
    only_done: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueListFormat::Compact)]
    format: QueueListFormat,

    /// Maximum results to show (0 = no limit).
    #[arg(long, default_value_t = 50)]
    limit: u32,

    /// Show all results (ignores --limit).
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue stats\n  ralph queue stats --tag rust --tag cli"
)]
struct QueueStatsArgs {
    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    tag: Vec<String>,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph queue history\n  ralph queue history --days 14")]
struct QueueHistoryArgs {
    /// Number of days to show (default: 7).
    #[arg(long, default_value_t = 7)]
    days: u32,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph queue burndown\n  ralph queue burndown --days 30")]
struct QueueBurndownArgs {
    /// Number of days to show (default: 7).
    #[arg(long, default_value_t = 7)]
    days: u32,
}

impl From<StatusArg> for contracts::TaskStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Todo => contracts::TaskStatus::Todo,
            StatusArg::Doing => contracts::TaskStatus::Doing,
            StatusArg::Done => contracts::TaskStatus::Done,
            StatusArg::Rejected => contracts::TaskStatus::Rejected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::contracts::{QueueFile, TaskStatus};
    use anyhow::Context;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::{env, fs};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    struct EnvGuard {
        cwd: PathBuf,
        xdg_config_home: Option<OsString>,
    }

    impl EnvGuard {
        fn enter(path: &PathBuf) -> anyhow::Result<Self> {
            let cwd = env::current_dir().context("read cwd")?;
            let xdg_config_home = env::var_os("XDG_CONFIG_HOME");
            let xdg_path = path.join("xdg");
            fs::create_dir_all(xdg_path.join("ralph")).context("create xdg config dir")?;
            env::set_var("XDG_CONFIG_HOME", &xdg_path);
            env::set_current_dir(path).context("set cwd")?;
            Ok(Self {
                cwd,
                xdg_config_home,
            })
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.cwd);
            match &self.xdg_config_home {
                Some(value) => env::set_var("XDG_CONFIG_HOME", value),
                None => env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }

    fn write_project_config(repo_root: &std::path::Path, contents: &str) -> anyhow::Result<()> {
        let config_dir = repo_root.join(".ralph");
        fs::create_dir_all(&config_dir).context("create .ralph dir")?;
        fs::write(config_dir.join("config.json"), contents).context("write config.json")?;
        Ok(())
    }

    #[test]
    fn queue_json_roundtrip_preserves_ids_and_statuses() -> anyhow::Result<()> {
        let path = repo_root().join(".ralph/queue.json");
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;

        let parsed: QueueFile =
            serde_json::from_str(&raw).with_context(|| "parse .ralph/queue.json as QueueFile")?;

        let rendered = serde_json::to_string_pretty(&parsed)
            .with_context(|| "serialize QueueFile back to JSON")?;

        let reparsed: QueueFile = serde_json::from_str(&rendered)
            .with_context(|| "parse serialized JSON as QueueFile")?;

        let left: Vec<(String, TaskStatus)> = parsed
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.status))
            .collect();
        let right: Vec<(String, TaskStatus)> = reparsed
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.status))
            .collect();

        assert_eq!(left, right);
        Ok(())
    }

    #[test]
    fn resolve_agent_args_uses_defaults_without_config_or_overrides() -> anyhow::Result<()> {
        let _lock = env_lock().lock().expect("env lock");
        let temp = TempDir::new().context("create temp dir")?;
        let _guard = EnvGuard::enter(&temp.path().to_path_buf())?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let settings = super::resolve_agent_args(&resolved, None, None, None)?;
        assert_eq!(settings.runner, super::RunnerKind::Claude);
        assert_eq!(
            settings.model,
            super::contracts::Model::Custom("sonnet".to_string())
        );
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_agent_args_uses_project_config_defaults() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();
        write_project_config(
            repo_root.as_path(),
            r#"{"version":1,"agent":{"runner":"opencode","model":"gpt-5.2","reasoning_effort":"high"}}"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let settings = super::resolve_agent_args(&resolved, None, None, None)?;
        assert_eq!(settings.runner, super::RunnerKind::Opencode);
        assert_eq!(settings.model, super::contracts::Model::Gpt52);
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_agent_args_cli_overrides_project_config() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();
        write_project_config(
            repo_root.as_path(),
            r#"{"version":1,"agent":{"runner":"opencode","model":"gpt-5.2","reasoning_effort":"low"}}"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let settings = super::resolve_agent_args(
            &resolved,
            Some("codex"),
            Some("gpt-5.2-codex"),
            Some("high"),
        )?;
        assert_eq!(settings.runner, super::RunnerKind::Codex);
        assert_eq!(settings.model, super::contracts::Model::Gpt52Codex);
        assert_eq!(
            settings.reasoning_effort,
            Some(super::contracts::ReasoningEffort::High)
        );
        Ok(())
    }

    #[test]
    fn resolve_agent_args_defaults_to_glm47_for_opencode_runner() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();

        // Config has Codex defaults
        write_project_config(
            repo_root.as_path(),
            r#"{"version":1,"agent":{"runner":"codex","model":"gpt-5.2-codex","reasoning_effort":"high"}}"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;

        // CLI override selects Opencode, but no model override.
        // Should default to Glm47, ignoring config model gpt-5.2-codex.
        let settings = super::resolve_agent_args(&resolved, Some("opencode"), None, None)?;
        assert_eq!(settings.runner, super::RunnerKind::Opencode);
        assert_eq!(settings.model, super::contracts::Model::Glm47);
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_agent_args_defaults_to_gemini_flash_for_gemini_runner() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();

        // Config has Codex defaults
        write_project_config(
            repo_root.as_path(),
            r#"{"version":1,"agent":{"runner":"codex","model":"gpt-5.2-codex","reasoning_effort":"high"}}"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;

        let settings = super::resolve_agent_args(&resolved, Some("gemini"), None, None)?;
        assert_eq!(settings.runner, super::RunnerKind::Gemini);
        assert_eq!(settings.model.as_str(), "gemini-3-flash-preview");
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_agent_args_defaults_for_codex_when_config_model_incompatible() -> anyhow::Result<()>
    {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();
        write_project_config(
            repo_root.as_path(),
            r#"{"version":1,"agent":{"runner":"codex","model":"zai-coding-plan/glm-4.7","reasoning_effort":"medium"}}"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let settings = super::resolve_agent_args(&resolved, None, None, None)?;
        assert_eq!(settings.runner, super::RunnerKind::Codex);
        assert_eq!(settings.model, super::contracts::Model::Gpt52Codex);
        assert_eq!(
            settings.reasoning_effort,
            Some(super::contracts::ReasoningEffort::Medium)
        );
        Ok(())
    }
}
