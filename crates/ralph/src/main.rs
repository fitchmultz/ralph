mod contracts;

mod config;
mod fsutil;
mod queue;
mod run_cmd;
mod timeutil;

mod prompts;
mod runner;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};

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
	}
}

fn handle_queue(cmd: QueueCommand) -> Result<()> {
	let resolved = config::resolve_from_cwd()?;
	match cmd {
		QueueCommand::Validate => {
			let queue_file = queue::load_queue(&resolved.queue_path)?;
			queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
		}
		QueueCommand::NextId => {
			let queue_file = queue::load_queue(&resolved.queue_path)?;
			let next = queue::next_id(&queue_file, &resolved.id_prefix, resolved.id_width)?;
			println!("{next}");
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
		RunCommand::One => run_cmd::run_one(&resolved),
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

#[derive(Subcommand)]
enum QueueCommand {
	Validate,
	NextId,
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