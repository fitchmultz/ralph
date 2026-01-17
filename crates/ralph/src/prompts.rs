use crate::contracts::Task;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const WORKER_PROMPT_REL_PATH: &str = ".ralph/prompts/worker.md";

pub fn worker_prompt_path(repo_root: &Path) -> PathBuf {
	repo_root.join(WORKER_PROMPT_REL_PATH)
}

pub fn load_worker_prompt(repo_root: &Path) -> Result<String> {
	let path = worker_prompt_path(repo_root);
	match fs::read_to_string(&path) {
		Ok(contents) => Ok(contents),
		Err(err) if err.kind() == io::ErrorKind::NotFound => bail!(
			"worker prompt template not found at {} (expected repo-local prompts).",
			path.display()
		),
		Err(err) => Err(err).with_context(|| format!("read worker prompt {}", path.display())),
	}
}

pub fn render_worker_prompt(template: &str, task: &Task) -> Result<String> {
	if !template.contains("{{TASK_YAML}}") {
		bail!("worker prompt template missing {{TASK_YAML}} placeholder");
	}
	let task_yaml = serde_yaml::to_string(task).context("serialize task YAML")?;

	let mut rendered = template.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
	rendered = rendered.replace("{{TASK_YAML}}", task_yaml.trim_end());
	Ok(rendered)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::contracts::{Task, TaskStatus};

	fn dummy_task() -> Task {
		Task {
			id: "RQ-0001".to_string(),
			status: TaskStatus::Todo,
			title: "Example".to_string(),
			tags: vec!["code".to_string()],
			scope: vec!["crates/ralph/src/prompts.rs".to_string()],
			evidence: vec!["Test".to_string()],
			plan: vec!["Do thing".to_string()],
			notes: vec![],
			request: None,
			agent: None,
			created_at: None,
			updated_at: None,
			completed_at: None,
			blocked_reason: None,
		}
	}

	#[test]
	fn render_worker_prompt_replaces_task_yaml() -> Result<()> {
		let template = "Hello\n{{INTERACTIVE_INSTRUCTIONS}}\n# CURRENT TASK\n{{TASK_YAML}}\n";
		let rendered = render_worker_prompt(template, &dummy_task())?;
		assert!(rendered.contains("RQ-0001"));
		assert!(!rendered.contains("{{TASK_YAML}}"));
		Ok(())
	}
}