use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use crate::redaction;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
    pub skipped_ids: Vec<String>,
}

pub fn load_queue(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                anyhow!(
                    "queue file not found: {}; run `ralph init` or inspect paths via `ralph config paths`",
                    path.display()
                )
            } else {
                anyhow::anyhow!(err).context(format!("read queue file {}", path.display()))
            }
        })?;
    let queue: QueueFile = serde_yaml::from_str(&raw)
        .with_context(|| format!("parse queue YAML {}", path.display()))?;
    Ok(queue)
}

pub fn load_queue_or_default(path: &Path) -> Result<QueueFile> {
    if !path.exists() {
        return Ok(QueueFile::default());
    }
    load_queue(path)
}

pub fn load_queue_with_repair(path: &Path) -> Result<(QueueFile, bool)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    match serde_yaml::from_str::<QueueFile>(&raw) {
        Ok(queue) => Ok((queue, false)),
        Err(err) => {
            let repaired = repair_yaml_scalars(&raw)
                .ok_or_else(|| anyhow!("parse queue YAML {}: {err}", path.display()))?;
            let queue: QueueFile = serde_yaml::from_str(&repaired)
                .with_context(|| format!("parse repaired queue YAML {}", path.display()))?;
            fsutil::write_atomic(path, repaired.as_bytes())
                .with_context(|| format!("write repaired queue YAML {}", path.display()))?;
            Ok((queue, true))
        }
    }
}

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_yaml::to_string(queue).context("serialize queue YAML")?;
    let rendered = repair_yaml_scalars(&rendered).unwrap_or(rendered);
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue YAML {}", path.display()))?;
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

pub fn validate_queue_set(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    validate_queue(active, id_prefix, id_width)?;
    if let Some(done) = done {
        validate_queue(done, id_prefix, id_width)?;
    }

    Ok(())
}

pub fn next_id_across(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<String> {
    validate_queue_set(active, done, id_prefix, id_width)?;
    let expected_prefix = normalize_prefix(id_prefix);

    let mut max_value: u32 = 0;
    for (idx, task) in active.tasks.iter().enumerate() {
        let value = validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
        if value > max_value {
            max_value = value;
        }
    }
    if let Some(done) = done {
        for (idx, task) in done.tasks.iter().enumerate() {
            let value = validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
            if value > max_value {
                max_value = value;
            }
        }
    }

    let next_value = max_value.saturating_add(1);
    Ok(format_id(&expected_prefix, next_value, id_width))
}

pub fn archive_done_tasks(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<ArchiveReport> {
    let mut active = load_queue(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    validate_queue(&active, id_prefix, id_width)?;
    validate_queue(&done, id_prefix, id_width)?;

    let mut done_ids: HashSet<String> =
        done.tasks.iter().map(|t| t.id.trim().to_string()).collect();
    for task in &active.tasks {
        if task.status == TaskStatus::Done {
            continue;
        }
        let key = task.id.trim().to_string();
        if done_ids.contains(&key) {
            bail!("duplicate task id across queue + done: {}", key);
        }
    }
    let mut moved_ids = Vec::new();
    let mut skipped_ids = Vec::new();
    let mut remaining = Vec::new();

    for task in active.tasks.into_iter() {
        if task.status != TaskStatus::Done {
            remaining.push(task);
            continue;
        }

        let key = task.id.trim().to_string();
        if done_ids.contains(&key) {
            skipped_ids.push(key);
            continue;
        }

        done_ids.insert(key.clone());
        moved_ids.push(key);
        done.tasks.push(task);
    }

    active.tasks = remaining;

    if moved_ids.is_empty() && skipped_ids.is_empty() {
        return Ok(ArchiveReport {
            moved_ids,
            skipped_ids,
        });
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(ArchiveReport {
        moved_ids,
        skipped_ids,
    })
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
                let redacted = redaction::redact_text(reason);
                let trimmed = redacted.trim();
                if !trimmed.is_empty() {
                    task.blocked_reason = Some(sanitize_yaml_text("Reason: ", trimmed).to_string());
                }
            }
        }
        TaskStatus::Todo | TaskStatus::Doing => {
            task.completed_at = None;
            task.blocked_reason = None;
        }
    }

    if let Some(note) = note {
        let redacted = redaction::redact_text(note);
        let trimmed = redacted.trim();
        if !trimmed.is_empty() {
            task.notes
                .push(sanitize_yaml_text("Note: ", trimmed).to_string());
        }
    }

    Ok(())
}

pub fn find_task<'a>(queue: &'a QueueFile, task_id: &str) -> Option<&'a Task> {
    let needle = task_id.trim();
    if needle.is_empty() {
        return None;
    }
    queue.tasks.iter().find(|task| task.id.trim() == needle)
}

fn sanitize_yaml_text(prefix: &str, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(first) = trimmed.chars().next() {
        if first == '`' {
            return format!("{prefix}{trimmed}");
        }
    }
    trimmed.to_string()
}

fn repair_yaml_scalars(raw: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::new();

    for line in raw.lines() {
        let mut updated = line.to_string();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(&updated);
            out.push('\n');
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let indent = line.len() - trimmed.len();
            if indent > 2 && should_quote_scalar(rest) && !looks_like_mapping(rest) {
                let escaped = escape_single_quotes(rest.trim());
                updated = format!("{}- '{}'", " ".repeat(indent), escaped);
                changed = true;
            }
        } else if let Some((left, right)) = line.split_once(": ") {
            let key = left.trim();
            if !key.is_empty() && should_quote_scalar(right) {
                let escaped = escape_single_quotes(right.trim());
                let indent = left.len() - left.trim_start().len();
                updated = format!("{}{}: '{}'", " ".repeat(indent), key, escaped);
                changed = true;
            }
        }

        out.push_str(&updated);
        out.push('\n');
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn should_quote_scalar(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('\'')
        || trimmed.starts_with('"')
        || trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.starts_with('|')
        || trimmed.starts_with('>')
    {
        return false;
    }
    trimmed.contains(": ")
}

fn looks_like_mapping(value: &str) -> bool {
    let trimmed = value.trim_start();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    for ch in chars {
        if ch == ':' {
            return true;
        }
        if !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
            return false;
        }
    }
    false
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "''")
}

pub fn next_todo_task(queue: &QueueFile) -> Option<&Task> {
    queue
        .tasks
        .iter()
        .find(|task| task.status == TaskStatus::Todo)
}

pub fn filter_tasks<'a>(
    queue: &'a QueueFile,
    statuses: &[TaskStatus],
    tags: &[String],
    limit: Option<usize>,
) -> Vec<&'a Task> {
    let status_filter: HashSet<TaskStatus> = statuses.iter().copied().collect();
    let tag_filter: HashSet<String> = tags
        .iter()
        .map(|tag| normalize_tag(tag))
        .filter(|tag| !tag.is_empty())
        .collect();

    let has_status_filter = !status_filter.is_empty();
    let has_tag_filter = !tag_filter.is_empty();

    let mut out = Vec::new();
    for task in &queue.tasks {
        if has_status_filter && !status_filter.contains(&task.status) {
            continue;
        }
        if has_tag_filter
            && !task
                .tags
                .iter()
                .any(|tag| tag_filter.contains(&normalize_tag(tag)))
        {
            continue;
        }

        out.push(task);
        if let Some(limit) = limit {
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().to_lowercase()
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

fn validate_task_id(
    index: usize,
    raw_id: &str,
    expected_prefix: &str,
    id_width: usize,
) -> Result<u32> {
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
        bail!(
            "task[{}] id numeric suffix must be digits (got: {})",
            index,
            num
        );
    }

    let value: u32 = num.parse().with_context(|| {
        format!(
            "task[{}] id numeric suffix must parse as integer (got: {})",
            index, num
        )
    })?;
    Ok(value)
}

fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            tags,
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
    fn validate_rejects_duplicate_ids() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001"), task("RQ-0001")],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("duplicate"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn set_status_updates_timestamps_and_fields() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            None,
            Some("started"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Doing);
        assert_eq!(t.updated_at.as_deref(), Some(now));
        assert_eq!(t.completed_at, None);
        assert_eq!(t.blocked_reason, None);
        assert_eq!(t.notes, vec!["started".to_string()]);

        let now2 = "2026-01-17T00:01:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Blocked,
            now2,
            Some("ci failed"),
            None,
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Blocked);
        assert_eq!(t.updated_at.as_deref(), Some(now2));
        assert_eq!(t.completed_at, None);
        assert_eq!(t.blocked_reason.as_deref(), Some("ci failed"));

        let now3 = "2026-01-17T00:02:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Done,
            now3,
            None,
            Some("completed"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Done);
        assert_eq!(t.updated_at.as_deref(), Some(now3));
        assert_eq!(t.completed_at.as_deref(), Some(now3));
        assert_eq!(t.blocked_reason, None);
        assert!(t.notes.iter().any(|n| n == "completed"));

        Ok(())
    }

    #[test]
    fn set_status_redacts_reason_and_note() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Blocked,
            now,
            Some("token=abc12345"),
            Some("API_KEY=abc12345"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.blocked_reason.as_deref(), Some("token=[REDACTED]"));
        assert_eq!(t.notes, vec!["API_KEY=[REDACTED]".to_string()]);

        Ok(())
    }

    #[test]
    fn set_status_sanitizes_leading_backticks() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Blocked,
            now,
            Some("`token` exposed"),
            Some("`make ci` failed"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.blocked_reason.as_deref(), Some("Reason: `token` exposed"));
        assert_eq!(t.notes, vec!["Note: `make ci` failed".to_string()]);

        Ok(())
    }

    #[test]
    fn validate_queue_set_allows_cross_file_duplicates() {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        validate_queue_set(&active, Some(&done), "RQ", 4).expect("allow duplicates");
    }

    #[test]
    fn next_id_across_includes_done() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0002")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0009")],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4)?;
        assert_eq!(next, "RQ-0010");
        Ok(())
    }

    #[test]
    fn load_queue_with_repair_quotes_colon_scalars() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");

        let raw = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Normalize empty queue YAML and handle tasks: null safely
    tags:
      - queue
    scope:
      - crates/ralph/src/queue.rs
    evidence:
      - queues can break: null tasks
    plan:
      - Fix parsing: add a safe default
"#;
        std::fs::write(&queue_path, raw)?;

        let (queue, repaired) = load_queue_with_repair(&queue_path)?;
        assert!(repaired);
        assert_eq!(
            queue.tasks[0].title,
            "Normalize empty queue YAML and handle tasks: null safely"
        );

        let repaired_raw = std::fs::read_to_string(&queue_path)?;
        assert!(repaired_raw
            .contains("title: 'Normalize empty queue YAML and handle tasks: null safely'"));
        assert!(repaired_raw.contains("- 'queues can break: null tasks'"));
        assert!(repaired_raw.contains("- 'Fix parsing: add a safe default'"));
        Ok(())
    }

    #[test]
    fn archive_done_tasks_moves_and_dedupes() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");
        let done_path = dir.path().join("done.yaml");

        let mut done_task = task("RQ-0002");
        done_task.status = TaskStatus::Done;

        let mut active_task = task("RQ-0001");
        active_task.status = TaskStatus::Done;

        let active = QueueFile {
            version: 1,
            tasks: vec![active_task.clone(), done_task.clone()],
        };

        let done = QueueFile {
            version: 1,
            tasks: vec![done_task],
        };

        save_queue(&queue_path, &active)?;
        save_queue(&done_path, &done)?;

        let report = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;
        assert_eq!(report.moved_ids, vec!["RQ-0001".to_string()]);
        assert_eq!(report.skipped_ids, vec!["RQ-0002".to_string()]);

        let active_after = load_queue(&queue_path)?;
        assert!(active_after.tasks.is_empty());

        let done_after = load_queue(&done_path)?;
        assert_eq!(done_after.tasks.len(), 2);

        let report2 = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;
        assert!(report2.moved_ids.is_empty());
        assert!(report2.skipped_ids.is_empty());
        Ok(())
    }

    #[test]
    fn find_task_returns_none_for_missing_or_blank() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        assert!(find_task(&queue, "").is_none());
        assert!(find_task(&queue, "RQ-9999").is_none());
    }

    #[test]
    fn next_todo_task_picks_first_todo() {
        let mut todo = task("RQ-0002");
        todo.status = TaskStatus::Todo;
        let mut doing = task("RQ-0001");
        doing.status = TaskStatus::Doing;

        let queue = QueueFile {
            version: 1,
            tasks: vec![doing, todo],
        };
        let found = next_todo_task(&queue).expect("todo task");
        assert_eq!(found.id, "RQ-0002");
    }

    #[test]
    fn next_todo_task_none_when_empty() {
        let mut doing = task("RQ-0001");
        doing.status = TaskStatus::Doing;
        let queue = QueueFile {
            version: 1,
            tasks: vec![doing],
        };
        assert!(next_todo_task(&queue).is_none());
    }

    #[test]
    fn filter_tasks_by_status_and_tag() {
        let mut todo = task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["rust".to_string(), "queue".to_string()],
        );
        todo.title = "First".to_string();
        let doing = task_with("RQ-0002", TaskStatus::Doing, vec!["docs".to_string()]);
        let done = task_with("RQ-0003", TaskStatus::Done, vec!["RUST".to_string()]);

        let queue = QueueFile {
            version: 1,
            tasks: vec![todo, doing, done],
        };

        let filtered = filter_tasks(
            &queue,
            &[TaskStatus::Todo, TaskStatus::Done],
            &["rust".to_string()],
            None,
        );
        let ids: Vec<&str> = filtered.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["RQ-0001", "RQ-0003"]);
    }

    #[test]
    fn filter_tasks_applies_limit() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001"), task("RQ-0002"), task("RQ-0003")],
        };

        let filtered = filter_tasks(&queue, &[], &[], Some(2));
        let ids: Vec<&str> = filtered.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["RQ-0001", "RQ-0002"]);
    }
}
