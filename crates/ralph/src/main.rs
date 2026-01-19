mod contracts;

mod config;
mod doctor_cmd;
mod fsutil;
mod init_cmd;
mod outpututil;
mod queue;
mod redaction;
mod run_cmd;
mod timeutil;

mod gitutil;
mod prompts;
mod runner;
mod scan_cmd;
mod task_cmd;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::contracts::{QueueFile, Runner as RunnerKind, Task, TaskStatus};

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {:#}", err);
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
    builder.init();

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
    let (queue_file, repaired_queue) = queue::load_queue_with_repair(&resolved.queue_path)?;
    queue::warn_if_repaired(&resolved.queue_path, repaired_queue);

    let done_file = if include_done {
        let (done, repaired_done) = queue::load_queue_or_default_with_repair(&resolved.done_path)?;
        queue::warn_if_repaired(&resolved.done_path, repaired_done);
        Some(done)
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
            let (queue_file, _) = load_and_validate_queues(&resolved, true)?;
            let next = queue::next_todo_task(&queue_file)
                .ok_or_else(|| anyhow::anyhow!("no todo tasks found"))?;
            if args.with_title {
                println!("{}\t{}", next.id.trim(), next.title.trim());
            } else {
                println!("{}", next.id.trim());
            }
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
                QueueShowFormat::Yaml => {
                    let rendered = serde_yaml::to_string(task)?;
                    print!("{rendered}");
                }
                QueueShowFormat::Compact => {
                    println!("{}", format_task_compact(task));
                }
            }
        }
        QueueCommand::List(args) => {
            if args.include_done && args.only_done {
                bail!("--include-done and --only-done are mutually exclusive");
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

            let max = limit.unwrap_or(usize::MAX);
            for task in tasks.into_iter().take(max) {
                match args.format {
                    QueueListFormat::Compact => println!("{}", format_task_compact(task)),
                    QueueListFormat::Long => println!("{}", format_task_long(task)),
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
        QueueCommand::Repair => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue repair", force)?;
            let report = queue::repair_queue(&resolved.queue_path)?;
            if report.repaired {
                log::info!("Repaired queue YAML.");
            } else {
                log::info!("Queue YAML is already valid (no repairs needed).");
            }
        }
        QueueCommand::SetStatus {
            task_id,
            status,
            note,
        } => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "queue set-status", force)?;
            let (mut queue_file, repaired_queue) =
                queue::load_queue_with_repair(&resolved.queue_path)?;
            queue::warn_if_repaired(&resolved.queue_path, repaired_queue);
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
    }
    Ok(())
}

fn handle_config(cmd: ConfigCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        ConfigCommand::Show => {
            let rendered = serde_yaml::to_string(&resolved.config)?;
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
    if report.queue_created {
        println!("queue: created ({})", resolved.queue_path.display());
    } else {
        println!("queue: exists ({})", resolved.queue_path.display());
    }
    if report.done_created {
        println!("done: created ({})", resolved.done_path.display());
    } else {
        println!("done: exists ({})", resolved.done_path.display());
    }
    if report.config_created {
        if let Some(path) = resolved.project_config_path.as_ref() {
            println!("config: created ({})", path.display());
        } else {
            println!("config: created");
        }
    } else if let Some(path) = resolved.project_config_path.as_ref() {
        println!("config: exists ({})", path.display());
    } else {
        println!("config: exists");
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
            let overrides = resolve_run_agent_overrides(&args.agent)?;
            let _ = run_cmd::run_one(&resolved, &overrides, force)?;
            Ok(())
        }
        RunCommand::Loop(args) => {
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
        _ => bail!(
            "--runner must be codex, opencode, or gemini (got: {})",
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

fn format_task_compact(task: &Task) -> String {
    format!("{}\t{}\t{}", task.id.trim(), task.status, task.title.trim())
}

fn format_task_long(task: &Task) -> String {
    fn join_trimmed(values: &[String]) -> String {
        values
            .iter()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .collect::<Vec<&str>>()
            .join(",")
    }

    let tags = join_trimmed(&task.tags);
    let scope = join_trimmed(&task.scope);
    let updated_at = task.updated_at.as_deref().unwrap_or("").trim();
    let completed_at = task.completed_at.as_deref().unwrap_or("").trim();

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        task.id.trim(),
        task.status,
        task.title.trim(),
        tags,
        scope,
        updated_at,
        completed_at
    )
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
#[command(about = "Ralph (Rust rewrite)")]
#[command(
    after_long_help = "Runner selection:\n  - CLI flags override project config, which overrides global config, which overrides built-in defaults.\n  - Default runner/model come from config files: project config (.ralph/config.yaml) > global config (~/.config/ralph/config.yaml) > built-in.\n  - `task build` and `scan` accept --runner/--model/--effort as one-off overrides.\n  - `run one` and `run loop` accept --runner/--model/--effort as one-off overrides; otherwise they use task.agent overrides when present; otherwise config agent defaults.\n\nConfig example (.ralph/config.yaml):\n  version: 1\n  agent:\n    runner: opencode\n    model: gpt-5.2\n    opencode_bin: opencode\n    gemini_bin: gemini\n\nNotes:\n  - Allowed runners: codex, opencode, gemini\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini accept arbitrary model ids)\n\nExamples:\n  ralph queue list\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI gaps\"\n  ralph task build --runner codex --model gpt-5.2-codex --effort high \"Fix the flaky test\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph run one"
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
    Doctor,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue list --status todo --tag rust\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph queue next-id"
)]
struct QueueArgs {
    #[command(subcommand)]
    command: QueueCommand,
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Args)]
#[command(
    about = "Run the Ralph supervisor (executes queued tasks via codex/opencode/gemini)",
    after_long_help = "Runner selection:\n  - `ralph run` selects runner/model/effort with this precedence:\n      1) CLI overrides (flags on `run one` / `run loop`)\n      2) the task's `agent` override (if present in .ralph/queue.yaml)\n      3) otherwise the resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\nNotes:\n  - Allowed runners: codex, opencode, gemini\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini accept arbitrary model ids)\n  - `--effort` is codex-only and is ignored for opencode.\n\nTo change defaults for this repo, edit .ralph/config.yaml:\n  version: 1\n  agent:\n    runner: opencode\n    model: gpt-5.2\n    gemini_bin: gemini\n\nExamples:\n  ralph run one\n  ralph run one --runner opencode --model gpt-5.2\n  ralph run one --runner codex --model gpt-5.2-codex --effort high\n  ralph run one --runner gemini --model gemini-3-flash-preview\n  ralph run loop --max-tasks 0\n  ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2"
)]
struct RunArgs {
    #[command(subcommand)]
    command: RunCommand,
}

#[derive(Args)]
struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Args)]
struct InitArgs {
    /// Overwrite existing files if they already exist.
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand)]
enum TaskCommand {
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
    Validate,
    /// Print the next todo task (ID by default).
    Next(QueueNextArgs),
    /// Print the next available task ID (across queue + done archive).
    NextId,
    /// Show a task by ID.
    Show(QueueShowArgs),
    /// List tasks in queue order.
    List(QueueListArgs),
    /// Move completed tasks from queue.yaml to done.yaml.
    Done,
    /// Remove the queue lock file.
    Unlock,
    /// Repair invalid YAML scalars in the queue file.
    Repair,
    /// Update a task status in the active queue.
    SetStatus {
        task_id: String,
        status: StatusArg,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    Show,
    Paths,
}

#[derive(Subcommand)]
enum RunCommand {
    #[command(
        about = "Run exactly one task (the first todo in .ralph/queue.yaml)",
        after_long_help = "Runner selection (precedence):\n  1) CLI overrides (--runner/--model/--effort)\n  2) task.agent in .ralph/queue.yaml (if present)\n  3) config defaults (.ralph/config.yaml then ~/.config/ralph/config.yaml)\n\nExamples:\n  ralph run one\n  ralph run one --runner opencode --model gpt-5.2\n  ralph run one --runner gemini --model gemini-3-flash-preview\n  ralph run one --runner codex --model gpt-5.2-codex --effort high\n  ralph queue next --with-title"
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
    #[command(flatten)]
    agent: RunAgentArgs,
}

#[derive(Args)]
struct RunLoopArgs {
    /// Maximum tasks to run before stopping (0 = no limit).
    #[arg(long, default_value_t = 0)]
    max_tasks: u32,

    #[command(flatten)]
    agent: RunAgentArgs,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum StatusArg {
    Todo,
    Doing,
    Done,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum QueueShowFormat {
    Yaml,
    Compact,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum QueueListFormat {
    Compact,
    Long,
}

#[derive(Args)]
struct QueueNextArgs {
    /// Include the task title after the ID.
    #[arg(long)]
    with_title: bool,
}

#[derive(Args)]
struct QueueShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Yaml)]
    format: QueueShowFormat,
}

#[derive(Args)]
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
}

impl From<StatusArg> for contracts::TaskStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Todo => contracts::TaskStatus::Todo,
            StatusArg::Doing => contracts::TaskStatus::Doing,
            StatusArg::Done => contracts::TaskStatus::Done,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::contracts::{QueueFile, Task, TaskStatus};
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
        fs::write(config_dir.join("config.yaml"), contents).context("write config.yaml")?;
        Ok(())
    }

    #[test]
    fn queue_yaml_roundtrip_preserves_ids_and_statuses() -> anyhow::Result<()> {
        let path = repo_root().join(".ralph/queue.yaml");
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;

        let parsed: QueueFile =
            serde_yaml::from_str(&raw).with_context(|| "parse .ralph/queue.yaml as QueueFile")?;

        let rendered =
            serde_yaml::to_string(&parsed).with_context(|| "serialize QueueFile back to YAML")?;

        let reparsed: QueueFile = serde_yaml::from_str(&rendered)
            .with_context(|| "parse serialized YAML as QueueFile")?;

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
    fn format_task_compact_trims_fields() {
        let task = Task {
            id: " RQ-0001 ".to_string(),
            status: TaskStatus::Doing,
            title: "  Fix bug  ".to_string(),
            tags: vec![],
            scope: vec![],
            evidence: vec!["e".to_string()],
            plan: vec!["p".to_string()],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
        };

        let rendered = super::format_task_compact(&task);
        assert_eq!(rendered, "RQ-0001\tdoing\tFix bug");
    }

    #[test]
    fn format_task_long_trims_and_renders_optional_timestamps() {
        let task = Task {
            id: " RQ-0001 ".to_string(),
            status: TaskStatus::Done,
            title: "  Ship it  ".to_string(),
            tags: vec![" rust ".to_string(), "queue".to_string()],
            scope: vec![" crates/ralph/src/main.rs ".to_string()],
            evidence: vec!["e".to_string()],
            plan: vec!["p".to_string()],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: Some(" 2026-01-18T06:30:00Z ".to_string()),
        };

        let rendered = super::format_task_long(&task);
        assert_eq!(
            rendered,
            "RQ-0001\tdone\tShip it\trust,queue\tcrates/ralph/src/main.rs\t\t2026-01-18T06:30:00Z"
        );
    }

    #[test]
    fn resolve_agent_args_uses_defaults_without_config_or_overrides() -> anyhow::Result<()> {
        let _lock = env_lock().lock().expect("env lock");
        let temp = TempDir::new().context("create temp dir")?;
        let _guard = EnvGuard::enter(&temp.path().to_path_buf())?;
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

    #[test]
    fn resolve_agent_args_uses_project_config_defaults() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();
        write_project_config(
            repo_root.as_path(),
            r#"version: 1
agent:
  runner: opencode
  model: gpt-5.2
  reasoning_effort: high
"#,
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
            r#"version: 1
agent:
  runner: opencode
  model: gpt-5.2
  reasoning_effort: low
"#,
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
            r#"version: 1
agent:
  runner: codex
  model: gpt-5.2-codex
  reasoning_effort: high
"#,
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
            r#"version: 1
agent:
  runner: codex
  model: gpt-5.2-codex
  reasoning_effort: high
"#,
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
    fn resolve_agent_args_rejects_invalid_runner_model_combo() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _lock = env_lock().lock().expect("env lock");
        let repo_root = temp.path().to_path_buf();
        write_project_config(
            repo_root.as_path(),
            r#"version: 1
agent:
  runner: codex
  model: zai-coding-plan/glm-4.7
  reasoning_effort: medium
"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let err = super::resolve_agent_args(&resolved, None, None, None).unwrap_err();
        assert!(format!("{err:#}").contains("zai-coding-plan/glm-4.7"));
        Ok(())
    }
}
