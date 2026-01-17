use crate::config;
use crate::contracts::{Model, ReasoningEffort, Runner, TaskStatus};
use crate::{prompts, queue, runner, timeutil};
use anyhow::{bail, Result};

const OUTPUT_TAIL_LINES: usize = 20;
const OUTPUT_TAIL_LINE_MAX_CHARS: usize = 200;

pub fn run_one(resolved: &config::Resolved) -> Result<()> {
	let mut queue_file = queue::load_queue(&resolved.queue_path)?;
	queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;

	let idx = match queue_file.tasks.iter().position(|t| t.status == TaskStatus::Todo) {
		Some(idx) => idx,
		None => {
			println!(">> [RALPH] No todo tasks found.");
			return Ok(());
		}
	};

	let task_id = queue_file.tasks[idx].id.trim().to_string();
	if task_id.is_empty() {
		bail!("selected task has empty id");
	}

	let now = timeutil::now_utc_rfc3339()?;
	queue::set_status(&mut queue_file, &task_id, TaskStatus::Doing, &now, None, None)?;
	queue::save_queue(&resolved.queue_path, &queue_file)?;

	let task = queue_file.tasks[idx].clone();
	let task_agent = task.agent.as_ref();

	let runner_kind: Runner = task_agent
		.and_then(|agent| agent.runner)
		.or(resolved.config.agent.runner)
		.unwrap_or_default();

	let model: Model = task_agent
		.and_then(|agent| agent.model)
		.or(resolved.config.agent.model)
		.unwrap_or_default();

	let reasoning_effort: Option<ReasoningEffort> = task_agent
		.and_then(|agent| agent.reasoning_effort)
		.or(resolved.config.agent.reasoning_effort);

	runner::validate_model_for_runner(runner_kind, model)?;

	let codex_bin = resolved.config.agent.codex_bin.as_deref().unwrap_or("codex");
	let opencode_bin = resolved.config.agent.opencode_bin.as_deref().unwrap_or("opencode");

	let template = prompts::load_worker_prompt(&resolved.repo_root)?;
	let prompt = prompts::render_worker_prompt(&template, &task)?;

	let output = runner::run_prompt(
		runner_kind,
		&resolved.repo_root,
		codex_bin,
		opencode_bin,
		model,
		reasoning_effort,
		&prompt,
	)?;

	if !output.stdout.is_empty() {
		print!("{}", output.stdout);
	}
	if !output.stderr.is_empty() {
		eprint!("{}", output.stderr);
	}

	if output.success() {
		println!(">> [RALPH] Runner completed successfully for {task_id}.");
		return Ok(());
	}

	let exit_reason = match output.status.code() {
		Some(code) => format!("runner exited non-zero (code={code})"),
		None => "runner terminated by signal".to_string(),
	};

	let mut latest = queue::load_queue(&resolved.queue_path)?;
	let now2 = timeutil::now_utc_rfc3339()?;
	queue::set_status(
		&mut latest,
		&task_id,
		TaskStatus::Blocked,
		&now2,
		Some(&exit_reason),
		None,
	)?;

	if let Some(task_mut) = latest.tasks.iter_mut().find(|t| t.id.trim() == task_id) {
		let combined = output.combined();
		let tail = tail_lines(&combined, OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);
		if !tail.is_empty() {
			task_mut.notes.push("runner output (tail):".to_string());
			for line in tail {
				task_mut.notes.push(format!("runner: {line}"));
			}
		}
	}

	queue::save_queue(&resolved.queue_path, &latest)?;
	bail!("runner failed; {task_id} marked blocked: {exit_reason}")
}

fn tail_lines(text: &str, max_lines: usize, max_chars: usize) -> Vec<String> {
	if max_lines == 0 || text.trim().is_empty() {
		return Vec::new();
	}
	let mut lines: Vec<&str> = text
		.lines()
		.map(|l| l.trim_end())
		.filter(|l| !l.trim().is_empty())
		.collect();

	if lines.len() > max_lines {
		lines = lines[lines.len() - max_lines..].to_vec();
	}

	lines
		.into_iter()
		.map(|line| truncate_chars(line.trim(), max_chars))
		.collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
	if max_chars == 0 {
		return String::new();
	}
	let mut chars = value.chars();
	let mut out = String::new();
	for _ in 0..max_chars {
		match chars.next() {
			Some(ch) => out.push(ch),
			None => return out,
		}
	}
	if chars.next().is_none() {
		return out;
	}
	if max_chars <= 3 {
		return out;
	}
	out.truncate(max_chars - 3);
	out.push_str("...");
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn truncate_chars_adds_ellipsis() {
		let value = "abcdefghijklmnopqrstuvwxyz";
		let truncated = truncate_chars(value, 10);
		assert_eq!(truncated, "abcdefg...");
	}
}