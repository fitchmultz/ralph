use crate::contracts::{Model, ReasoningEffort, Runner};
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

const OPENCODE_PROMPT_FILE_MESSAGE: &str = "Follow the attached prompt file verbatim.";

pub struct RunnerOutput {
	pub status: ExitStatus,
	pub stdout: String,
	pub stderr: String,
}

impl RunnerOutput {
	pub fn success(&self) -> bool {
		self.status.success()
	}

	pub fn combined(&self) -> String {
		if self.stdout.is_empty() {
			return self.stderr.clone();
		}
		if self.stderr.is_empty() {
			return self.stdout.clone();
		}
		format!("{}{}", self.stdout, self.stderr)
	}
}

pub fn validate_model_for_runner(runner: Runner, model: Model) -> Result<()> {
	if runner == Runner::Codex && model == Model::Glm47 {
		bail!("model glm-4.7 is not supported for codex runner");
	}
	Ok(())
}

pub fn run_prompt(
	runner: Runner,
	work_dir: &Path,
	codex_bin: &str,
	opencode_bin: &str,
	model: Model,
	reasoning_effort: Option<ReasoningEffort>,
	prompt: &str,
) -> Result<RunnerOutput> {
	validate_model_for_runner(runner, model)?;
	match runner {
		Runner::Codex => run_codex(work_dir, codex_bin, model, reasoning_effort, prompt),
		Runner::Opencode => run_opencode(work_dir, opencode_bin, model, prompt),
	}
}

fn run_codex(
	work_dir: &Path,
	bin: &str,
	model: Model,
	reasoning_effort: Option<ReasoningEffort>,
	prompt: &str,
) -> Result<RunnerOutput> {
	let mut cmd = Command::new(bin);
	cmd.current_dir(work_dir);
	cmd.arg("exec")
		.arg("--full-auto")
		.arg("--sandbox")
		.arg("workspace-write")
		.arg("--model")
		.arg(model_as_str(model));

	if let Some(effort) = reasoning_effort {
		cmd.arg("-c")
			.arg(format!("model_reasoning_effort=\"{}\"", effort_as_str(effort)));
	}

	cmd.arg("-")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());

	let mut child = cmd.spawn().with_context(|| format!("spawn {}", bin))?;
	{
		let stdin = child.stdin.as_mut().context("open codex stdin")?;
		stdin
			.write_all(prompt.as_bytes())
			.context("write prompt to stdin")?;
	}

	let output = child.wait_with_output().context("wait for codex to exit")?;
	Ok(RunnerOutput {
		status: output.status,
		stdout: String::from_utf8_lossy(&output.stdout).to_string(),
		stderr: String::from_utf8_lossy(&output.stderr).to_string(),
	})
}

fn run_opencode(work_dir: &Path, bin: &str, model: Model, prompt: &str) -> Result<RunnerOutput> {
	let mut tmp = tempfile::Builder::new()
		.prefix("ralph_prompt_")
		.suffix(".md")
		.tempfile()
		.context("create temp prompt file")?;

	tmp.write_all(prompt.as_bytes()).context("write prompt file")?;
	tmp.flush().context("flush prompt file")?;

	let mut cmd = Command::new(bin);
	cmd.current_dir(work_dir);
	cmd.arg("run")
		.arg("--model")
		.arg(model_as_str(model))
		.arg("--file")
		.arg(tmp.path())
		.arg("--")
		.arg(OPENCODE_PROMPT_FILE_MESSAGE)
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());

	let output = cmd.output().with_context(|| format!("run {}", bin))?;
	Ok(RunnerOutput {
		status: output.status,
		stdout: String::from_utf8_lossy(&output.stdout).to_string(),
		stderr: String::from_utf8_lossy(&output.stderr).to_string(),
	})
}

fn model_as_str(model: Model) -> &'static str {
	match model {
		Model::Gpt52Codex => "gpt-5.2-codex",
		Model::Gpt52 => "gpt-5.2",
		Model::Glm47 => "glm-4.7",
	}
}

fn effort_as_str(effort: ReasoningEffort) -> &'static str {
	match effort {
		ReasoningEffort::Minimal => "minimal",
		ReasoningEffort::Low => "low",
		ReasoningEffort::Medium => "medium",
		ReasoningEffort::High => "high",
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn validate_model_for_runner_rejects_glm47_on_codex() {
		let err = validate_model_for_runner(Runner::Codex, Model::Glm47).unwrap_err();
		let msg = format!("{err:#}");
		assert!(msg.contains("glm-4.7"));
	}
}