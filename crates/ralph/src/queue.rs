use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;

pub fn load_queue(path: &Path) -> Result<QueueFile> {
	let raw = std::fs::read_to_string(path).with_context(|| format!("read queue file {}", path.display()))?;
	let queue: QueueFile = serde_yaml::from_str(&raw).with_context(|| format!("parse queue YAML {}", path.display()))?;
	Ok(queue)
}

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
	let rendered = serde_yaml::to_string(queue).context("serialize queue YAML")?;
	fsutil::write_atomic(path, rendered.as_bytes()).with_context(|| format!("write queue YAML {}", path.display()))?;
	Ok(())
}

pub fn validate_queue(queue: &QueueFile, id_prefix: &str, id_width: usize) -> Result<()> {
	if queue.version != 1 {
		bail!("queue.yaml version must be 1 (got {})", queue.version);
	}
	if id_width == 0 {
		bail!("id_width must be > 0");
	}

	let expected_prefix = normalize_prefix(id_prefix);
	if expected_prefix.is_empty() {
		bail!("id_prefix must be non-empty");
	}

	let mut seen = HashSet::new();
	for (idx, task) in queue.tasks.iter().enumerate() {
		validate_task_required_fields(idx, task)?;
		validate_task_id(idx, &task.id, &expected_prefix, id_width)?;

		let key = task.id.trim().to_string();
		if !seen.insert(key.clone()) {
			bail!("duplicate task id detected: {}", key);
		}
	}

	Ok(())
}

pub fn next_id(queue: &QueueFile, id_prefix: &str, id_width: usize) -> Result<String> {
	validate_queue(queue, id_prefix, id_width)?;
	let expected_prefix = normalize_prefix(id_prefix);

	let mut max_value: u32 = 0;
	for (idx, task) in queue.tasks.iter().enumerate() {
		let value = validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
		if value > max_value {
			max_value = value;
		}
	}

	let next_value = max_value.saturating_add(1);
	Ok(format_id(&expected_prefix, next_value, id_width))
}

pub fn set_status(
	queue: &mut QueueFile,
	task_id: &str,
	status: TaskStatus,
	now_rfc3339: &str,
	reason: Option<&str>,
	note: Option<&str>,
) -> Result<()> {
	let now = now_rfc3339.trim();
	if now.is_empty() {
		bail!("now timestamp is required");
	}

	let needle = task_id.trim();
	if needle.is_empty() {
		bail!("task_id is required");
	}

	let task = queue
		.tasks
		.iter_mut()
		.find(|t| t.id.trim() == needle)
		.ok_or_else(|| anyhow!("task not found: {}", needle))?;

	task.status = status;
	task.updated_at = Some(now.to_string());

	match status {
		TaskStatus::Done => {
			task.completed_at = Some(now.to_string());
			task.blocked_reason = None;
		}
		TaskStatus::Blocked => {
			task.completed_at = None;
			if let Some(reason) = reason {
				let trimmed = reason.trim();
				if !trimmed.is_empty() {
					task.blocked_reason = Some(trimmed.to_string());
				}
			}
		}
		TaskStatus::Todo | TaskStatus::Doing => {
			task.completed_at = None;
			task.blocked_reason = None;
		}
	}

	if let Some(note) = note {
		let trimmed = note.trim();
		if !trimmed.is_empty() {
			task.notes.push(trimmed.to_string());
		}
	}

	Ok(())
}

fn normalize_prefix(prefix: &str) -> String {
	prefix.trim().to_uppercase()
}

fn validate_task_required_fields(index: usize, task: &Task) -> Result<()> {
	if task.id.trim().is_empty() {
		bail!("task[{}] id is required", index);
	}
	if task.title.trim().is_empty() {
		bail!("task[{}] title is required (id={})", index, task.id);
	}
	ensure_list_non_empty("tags", index, &task.id, &task.tags)?;
	ensure_list_non_empty("scope", index, &task.id, &task.scope)?;
	ensure_list_non_empty("evidence", index, &task.id, &task.evidence)?;
	ensure_list_non_empty("plan", index, &task.id, &task.plan)?;
	Ok(())
}

fn ensure_list_non_empty(label: &str, index: usize, id: &str, values: &[String]) -> Result<()> {
	if values.is_empty() {
		bail!("task[{}] {} must be non-empty (id={})", index, label, id);
	}
	for (i, value) in values.iter().enumerate() {
		if value.trim().is_empty() {
			bail!(
				"task[{}] {}[{}] must be non-empty (id={})",
				index,
				label,
				i,
				id
			);
		}
	}
	Ok(())
}

fn validate_task_id(index: usize, raw_id: &str, expected_prefix: &str, id_width: usize) -> Result<u32> {
	let trimmed = raw_id.trim();
	let (prefix_raw, num_raw) = trimmed
		.split_once('-')
		.ok_or_else(|| anyhow!("task[{}] id must contain '-' (got: {})", index, trimmed))?;

	let prefix = prefix_raw.trim().to_uppercase();
	if prefix != expected_prefix {
		bail!(
			"task[{}] id prefix must be {} (got: {})",
			index,
			expected_prefix,
			prefix
		);
	}

	let num = num_raw.trim();
	if num.len() != id_width {
		bail!(
			"task[{}] id numeric width must be {} digits (got: {})",
			index,
			id_width,
			num
		);
	}
	if !num.chars().all(|c| c.is_ascii_digit()) {
		bail!("task[{}] id numeric suffix must be digits (got: {})", index, num);
	}

	let value: u32 = num
		.parse()
		.with_context(|| format!("task[{}] id numeric suffix must parse as integer (got: {})", index, num))?;
	Ok(value)
}

fn format_id(prefix: &str, number: u32, width: usize) -> String {
	format!("{}-{:0width$}", prefix, number, width = width)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::contracts::{Task, TaskStatus};

	fn task(id: &str) -> Task {
		Task {
			id: id.to_string(),
			status: TaskStatus::Todo,
			title: "Test task".to_string(),
			tags: vec!["code".to_string()],
			scope: vec!["crates/ralph".to_string()],
			evidence: vec!["observed".to_string()],
			plan: vec!["do thing".to_string()],
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
	fn next_id_increments_properly() -> Result<()> {
		let queue = QueueFile {
			version: 1,
			tasks: vec![task("RQ-0001"), task("RQ-0010")],
		};
		let next = next_id(&queue, "RQ", 4)?;
		assert_eq!(next, "RQ-0011");
		Ok(())
	}

	#[test]
	fn validate_rejects_duplicate_ids() {
		let queue = QueueFile {
			version: 1,
			tasks: vec![task("RQ-0001"), task("RQ-0001")],
		};
		let err = validate_queue(&queue, "RQ", 4).unwrap_err();
		let msg = format!("{err:#}");
		assert!(msg.to_lowercase().contains("duplicate"), "unexpected error: {msg}");
	}

	#[test]
	fn set_status_updates_timestamps_and_fields() -> Result<()> {
		let mut queue = QueueFile {
			version: 1,
			tasks: vec![task("RQ-0001")],
		};

		let now = "2026-01-17T00:00:00Z";
		set_status(&mut queue, "RQ-0001", TaskStatus::Doing, now, None, Some("started"))?;
		let t = &queue.tasks[0];
		assert_eq!(t.status, TaskStatus::Doing);
		assert_eq!(t.updated_at.as_deref(), Some(now));
		assert_eq!(t.completed_at, None);
		assert_eq!(t.blocked_reason, None);
		assert_eq!(t.notes, vec!["started".to_string()]);

		let now2 = "2026-01-17T00:01:00Z";
		set_status(&mut queue, "RQ-0001", TaskStatus::Blocked, now2, Some("ci failed"), None)?;
		let t = &queue.tasks[0];
		assert_eq!(t.status, TaskStatus::Blocked);
		assert_eq!(t.updated_at.as_deref(), Some(now2));
		assert_eq!(t.completed_at, None);
		assert_eq!(t.blocked_reason.as_deref(), Some("ci failed"));

		let now3 = "2026-01-17T00:02:00Z";
		set_status(&mut queue, "RQ-0001", TaskStatus::Done, now3, None, Some("completed"))?;
		let t = &queue.tasks[0];
		assert_eq!(t.status, TaskStatus::Done);
		assert_eq!(t.updated_at.as_deref(), Some(now3));
		assert_eq!(t.completed_at.as_deref(), Some(now3));
		assert_eq!(t.blocked_reason, None);
		assert!(t.notes.iter().any(|n| n == "completed"));

		Ok(())
	}
}