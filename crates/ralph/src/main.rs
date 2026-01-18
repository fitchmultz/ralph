mod contracts;

mod config;
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

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::contracts::{Runner as RunnerKind, Task, TaskStatus};

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {:#}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Queue(args) => handle_queue(args.command),
        Command::Config(args) => handle_config(args.command),
        Command::Run(args) => handle_run(args.command),
        Command::Task(args) => handle_task(args.command),
        Command::Scan(args) => handle_scan(args),
        Command::Init(args) => handle_init(args),
    }
}

fn handle_queue(cmd: QueueCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        QueueCommand::Validate => {
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done)
            };
            queue::validate_queue_set(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
        }
        QueueCommand::Next(args) => {
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done)
            };
            queue::validate_queue_set(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            let next = queue::next_todo_task(&queue_file)
                .ok_or_else(|| anyhow::anyhow!("no todo tasks found"))?;
            if args.with_title {
                println!("{}\t{}", next.id.trim(), next.title.trim());
            } else {
                println!("{}", next.id.trim());
            }
        }
        QueueCommand::NextId => {
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done)
            };
            let next = queue::next_id_across(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            println!("{next}");
        }
        QueueCommand::Show(args) => {
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
            let task = queue::find_task(&queue_file, &args.task_id)
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
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
            let statuses: Vec<TaskStatus> = args.status.into_iter().map(|s| s.into()).collect();
            let limit = resolve_list_limit(args.limit, args.all);
            let tasks = queue::filter_tasks(&queue_file, &statuses, &args.tag, limit);
            for task in tasks {
                println!("{}", format_task_compact(task));
            }
        }
        QueueCommand::Done => {
            let report = queue::archive_done_tasks(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            if report.moved_ids.is_empty() && report.skipped_ids.is_empty() {
                println!(">> [RALPH] No done tasks to move.");
            } else {
                println!(
                    ">> [RALPH] Moved {} done task(s) ({} skipped as already done).",
                    report.moved_ids.len(),
                    report.skipped_ids.len()
                );
            }
        }
        QueueCommand::SetStatus {
            task_id,
            status,
            reason,
            note,
        } => {
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(
                &mut queue_file,
                &task_id,
                status.into(),
                &now,
                reason.as_deref(),
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

fn handle_init(args: InitArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let report = init_cmd::run_init(&resolved, init_cmd::InitOptions { force: args.force })?;
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

fn handle_run(cmd: RunCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        RunCommand::One => {
            run_cmd::run_one(&resolved)?;
            Ok(())
        }
        RunCommand::Loop(args) => run_cmd::run_loop(
            &resolved,
            run_cmd::RunLoopOptions {
                max_tasks: args.max_tasks,
            },
        ),
    }
}

fn handle_task(cmd: TaskCommand) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        TaskCommand::Build(args) => {
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let (runner_kind, model, reasoning_effort) = resolve_agent_args(
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
                    runner: runner_kind,
                    model,
                    reasoning_effort,
                },
            )
        }
    }
}

fn handle_scan(args: ScanArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let (runner_kind, model, reasoning_effort) = resolve_agent_args(
        &resolved,
        args.runner.as_deref(),
        args.model.as_deref(),
        args.effort.as_deref(),
    )?;

    scan_cmd::run_scan(
        &resolved,
        scan_cmd::ScanOptions {
            focus: args.focus,
            runner: runner_kind,
            model,
            reasoning_effort,
        },
    )
}

fn parse_runner(value: &str) -> Result<RunnerKind> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "codex" => Ok(RunnerKind::Codex),
        "opencode" => Ok(RunnerKind::Opencode),
        _ => bail!("--runner must be codex or opencode (got: {})", value.trim()),
    }
}

fn resolve_agent_args(
    resolved: &config::Resolved,
    runner_override: Option<&str>,
    model_override: Option<&str>,
    effort_override: Option<&str>,
) -> Result<(
    RunnerKind,
    contracts::Model,
    Option<contracts::ReasoningEffort>,
)> {
    let runner_kind = match runner_override {
        Some(value) => parse_runner(value)?,
        None => resolved.config.agent.runner.unwrap_or_default(),
    };

    let model = match model_override {
        Some(value) => runner::parse_model(value)?,
        None => resolved.config.agent.model.unwrap_or_default(),
    };

    let reasoning_effort = if runner_kind == RunnerKind::Codex {
        let effort = match effort_override {
            Some(value) => runner::parse_reasoning_effort(value)?,
            None => resolved.config.agent.reasoning_effort.unwrap_or_default(),
        };
        Some(effort)
    } else {
        None
    };

    runner::validate_model_for_runner(runner_kind, model)?;
    Ok((runner_kind, model, reasoning_effort))
}

fn format_task_compact(task: &Task) -> String {
    format!("{}\t{}\t{}", task.id.trim(), task.status, task.title.trim())
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
    after_long_help = "Examples:\n  ralph queue list\n  ralph queue show RQ-0008\n  ralph queue next --with-title\n  ralph run one\n  ralph task build \"Fix the flaky test\""
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Queue(QueueArgs),
    Config(ConfigArgs),
    Run(RunArgs),
    Task(TaskArgs),
    Scan(ScanArgs),
    Init(InitArgs),
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
    /// Ignored for opencode.
    #[arg(long)]
    effort: Option<String>,
}

#[derive(Args)]
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
    /// Ignored for opencode.
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
    /// Update a task status in the active queue.
    SetStatus {
        task_id: String,
        status: StatusArg,
        #[arg(long)]
        reason: Option<String>,
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
    One,
    Loop(RunLoopArgs),
}

#[derive(Args)]
struct RunLoopArgs {
    /// Maximum tasks to run before stopping (0 = no limit).
    #[arg(long, default_value_t = 0)]
    max_tasks: u32,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum StatusArg {
    Todo,
    Doing,
    Blocked,
    Done,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum QueueShowFormat {
    Yaml,
    Compact,
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
            StatusArg::Blocked => contracts::TaskStatus::Blocked,
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
    use std::{env, fs};
    use tempfile::TempDir;

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
            blocked_reason: None,
        };

        let rendered = super::format_task_compact(&task);
        assert_eq!(rendered, "RQ-0001\tdoing\tFix bug");
    }

    #[test]
    fn resolve_agent_args_uses_defaults_without_config_or_overrides() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let _guard = EnvGuard::enter(&temp.path().to_path_buf())?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let (runner, model, effort) = super::resolve_agent_args(&resolved, None, None, None)?;
        assert_eq!(runner, super::RunnerKind::Codex);
        assert_eq!(model, super::contracts::Model::Gpt52Codex);
        assert_eq!(effort, Some(super::contracts::ReasoningEffort::Medium));
        Ok(())
    }

    #[test]
    fn resolve_agent_args_uses_project_config_defaults() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
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
        let (runner, model, effort) = super::resolve_agent_args(&resolved, None, None, None)?;
        assert_eq!(runner, super::RunnerKind::Opencode);
        assert_eq!(model, super::contracts::Model::Gpt52);
        assert_eq!(effort, None);
        Ok(())
    }

    #[test]
    fn resolve_agent_args_cli_overrides_project_config() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
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
        let (runner, model, effort) = super::resolve_agent_args(
            &resolved,
            Some("codex"),
            Some("gpt-5.2-codex"),
            Some("high"),
        )?;
        assert_eq!(runner, super::RunnerKind::Codex);
        assert_eq!(model, super::contracts::Model::Gpt52Codex);
        assert_eq!(effort, Some(super::contracts::ReasoningEffort::High));
        Ok(())
    }

    #[test]
    fn resolve_agent_args_rejects_invalid_runner_model_combo() -> anyhow::Result<()> {
        let temp = TempDir::new().context("create temp dir")?;
        let repo_root = temp.path().to_path_buf();
        write_project_config(
            repo_root.as_path(),
            r#"version: 1
agent:
  runner: codex
  model: glm-4.7
  reasoning_effort: medium
"#,
        )?;
        let _guard = EnvGuard::enter(&repo_root)?;
        let resolved = crate::config::resolve_from_cwd().context("resolve config")?;
        let err = super::resolve_agent_args(&resolved, None, None, None).unwrap_err();
        assert!(format!("{err:#}").contains("glm-4.7"));
        Ok(())
    }
}
