mod contracts;

mod config;
mod fsutil;
mod queue;
mod run_cmd;
mod timeutil;

mod gitutil;
mod prompts;
mod runner;
mod scan_cmd;
mod task_cmd;

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::contracts::Runner as RunnerKind;

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
			queue::validate_queue_set(&queue_file, done_ref, &resolved.id_prefix, resolved.id_width)?;
		}
		QueueCommand::NextId => {
			let queue_file = queue::load_queue(&resolved.queue_path)?;
			let done = queue::load_queue_or_default(&resolved.done_path)?;
			let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
				None
			} else {
				Some(&done)
			};
			let next = queue::next_id_across(&queue_file, done_ref, &resolved.id_prefix, resolved.id_width)?;
			println!("{next}");
		}
		QueueCommand::Archive => {
			let report = queue::archive_done_tasks(
				&resolved.queue_path,
				&resolved.done_path,
				&resolved.id_prefix,
				resolved.id_width,
			)?;
			if report.moved_ids.is_empty() && report.skipped_ids.is_empty() {
				println!(">> [RALPH] No done tasks to archive.");
			} else {
				println!(
					">> [RALPH] Archived {} done task(s) ({} skipped as already archived).",
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
			let runner_kind = parse_runner(&args.runner)?;
			let model = runner::parse_model(&args.model)?;
			let effort = runner::parse_reasoning_effort(&args.effort)?;
			let reasoning_effort = if runner_kind == RunnerKind::Codex {
				Some(effort)
			} else {
				None
			};

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
	let runner_kind = parse_runner(&args.runner)?;
	let model = runner::parse_model(&args.model)?;
	let effort = runner::parse_reasoning_effort(&args.effort)?;
	let reasoning_effort = if runner_kind == RunnerKind::Codex {
		Some(effort)
	} else {
		None
	};

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

#[derive(Parser)]
#[command(name = "ralph")]
#[command(about = "Ralph (Rust rewrite)")]
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
}

#[derive(Args)]
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

	/// Runner to use (default: codex).
	#[arg(long, default_value = "codex")]
	runner: String,

	/// Model to use (default: gpt-5.2-codex).
	#[arg(long, default_value = "gpt-5.2-codex")]
	model: String,

	/// Codex reasoning effort (default: low). Ignored for opencode.
	#[arg(long, default_value = "low")]
	effort: String,
}

#[derive(Args)]
struct ScanArgs {
	/// Optional focus prompt to guide the scan.
	#[arg(long, default_value = "")]
	focus: String,

	/// Runner to use (default: codex).
	#[arg(long, default_value = "codex")]
	runner: String,

	/// Model to use (default: gpt-5.2).
	#[arg(long, default_value = "gpt-5.2")]
	model: String,

	/// Codex reasoning effort (default: high). Ignored for opencode.
	#[arg(long, default_value = "high")]
	effort: String,
}

#[derive(Subcommand)]
enum QueueCommand {
	Validate,
	NextId,
	Archive,
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
	use super::contracts::{QueueFile, TaskStatus};
	use anyhow::Context;
	use std::path::PathBuf;

	fn repo_root() -> PathBuf {
		PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
	}

	#[test]
	fn queue_yaml_roundtrip_preserves_ids_and_statuses() -> anyhow::Result<()> {
		let path = repo_root().join(".ralph/queue.yaml");
		let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;

		let parsed: QueueFile =
			serde_yaml::from_str(&raw).with_context(|| "parse .ralph/queue.yaml as QueueFile")?;

		let rendered = serde_yaml::to_string(&parsed).with_context(|| "serialize QueueFile back to YAML")?;

		let reparsed: QueueFile =
			serde_yaml::from_str(&rendered).with_context(|| "parse serialized YAML as QueueFile")?;

		let left: Vec<(String, TaskStatus)> = parsed.tasks.iter().map(|t| (t.id.clone(), t.status)).collect();
		let right: Vec<(String, TaskStatus)> = reparsed.tasks.iter().map(|t| (t.id.clone(), t.status)).collect();

		assert_eq!(left, right);
		Ok(())
	}
}
