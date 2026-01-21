//! Task queue persistence, validation, and pruning.
//!
//! This module handles loading, saving, and validating task queues stored
//! as JSON files (.ralph/queue.json for active tasks, .ralph/done.json
//! for completed tasks). It provides operations for moving completed tasks,
//! updating task status, repairing queue data, and pruning old tasks from
//! the done archive.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use crate::redaction;
use crate::timeutil;
use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
}

/// Result of a prune operation on the done archive.
#[derive(Debug, Clone, Default)]
pub struct PruneReport {
    /// IDs of tasks that were pruned (or would be pruned in dry-run).
    pub pruned_ids: Vec<String>,
    /// IDs of tasks that were kept (protected by keep-last or didn't match filters).
    pub kept_ids: Vec<String>,
}

/// Options for pruning tasks from the done archive.
#[derive(Debug, Clone)]
pub struct PruneOptions {
    /// Minimum age in days for a task to be pruned (None = no age filter).
    pub age_days: Option<u32>,
    /// Statuses to prune (empty = all statuses).
    pub statuses: HashSet<TaskStatus>,
    /// Keep the N most recently completed tasks regardless of other filters.
    pub keep_last: Option<u32>,
    /// If true, report what would be pruned without writing to disk.
    pub dry_run: bool,
}

#[derive(Debug, Default, Clone)]
pub struct RepairReport {
    pub fixed_tasks: usize,
    pub remapped_ids: Vec<(String, String)>,
    pub fixed_timestamps: usize,
}

impl RepairReport {
    pub fn is_empty(&self) -> bool {
        self.fixed_tasks == 0 && self.remapped_ids.is_empty() && self.fixed_timestamps == 0
    }
}

pub fn repair_queue(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
    dry_run: bool,
) -> Result<RepairReport> {
    let mut active = load_queue_or_default(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    let mut report = RepairReport::default();
    let expected_prefix = normalize_prefix(id_prefix);
    let now = timeutil::now_utc_rfc3339_or_fallback();

    // 1. Scan for max ID to ensure new IDs don't collide
    let mut max_id_val: u32 = 0;
    let mut scan_max = |tasks: &[Task]| {
        for task in tasks {
            if let Ok(val) = validate_task_id(0, &task.id, &expected_prefix, id_width) {
                max_id_val = max_id_val.max(val);
            }
        }
    };
    scan_max(&active.tasks);
    scan_max(&done.tasks);

    let mut next_id_val = max_id_val + 1;
    let mut seen_ids = HashSet::new();

    // Helper to repair a list of tasks
    let mut repair_tasks = |tasks: &mut Vec<Task>| {
        for task in tasks.iter_mut() {
            let mut modified = false;

            // Fix missing fields
            if task.title.trim().is_empty() {
                task.title = "Untitled".to_string();
                modified = true;
            }
            if task.tags.is_empty() {
                task.tags.push("untagged".to_string());
                modified = true;
            }
            if task.scope.is_empty() {
                task.scope.push("unknown".to_string());
                modified = true;
            }
            if task.evidence.is_empty() {
                task.evidence.push("None provided".to_string());
                modified = true;
            }
            if task.plan.is_empty() {
                task.plan.push("To be determined".to_string());
                modified = true;
            }
            if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
                task.request = Some("Imported task".to_string());
                modified = true;
            }

            // Fix timestamps
            let mut fix_ts = |ts: &mut Option<String>, label: &str| {
                if let Some(val) = ts {
                    if OffsetDateTime::parse(val, &Rfc3339).is_err() {
                        *ts = Some(now.clone());
                        report.fixed_timestamps += 1;
                    }
                } else {
                    // Create/Update required
                    if label == "created_at" || label == "updated_at" {
                        *ts = Some(now.clone());
                        report.fixed_timestamps += 1;
                    }
                }
            };
            fix_ts(&mut task.created_at, "created_at");
            fix_ts(&mut task.updated_at, "updated_at");
            if task.status == TaskStatus::Done || task.status == TaskStatus::Rejected {
                fix_ts(&mut task.completed_at, "completed_at");
            }

            if modified {
                report.fixed_tasks += 1;
            }

            // Fix ID
            // We use a normalized key for collision detection
            let id_key = task.id.trim().to_uppercase();
            let is_valid_format = validate_task_id(0, &task.id, &expected_prefix, id_width).is_ok();

            if !is_valid_format || seen_ids.contains(&id_key) || id_key.is_empty() {
                let new_id = format_id(&expected_prefix, next_id_val, id_width);
                next_id_val += 1;
                report.remapped_ids.push((task.id.clone(), new_id.clone()));
                task.id = new_id.clone();
                seen_ids.insert(new_id);
            } else {
                seen_ids.insert(id_key);
            }
        }
    };

    repair_tasks(&mut active.tasks);
    repair_tasks(&mut done.tasks);

    if !dry_run && !report.is_empty() {
        save_queue(queue_path, &active)?;
        save_queue(done_path, &done)?;
    }
    Ok(report)
}

pub fn acquire_queue_lock(repo_root: &Path, label: &str, force: bool) -> Result<fsutil::DirLock> {
    let lock_dir = fsutil::queue_lock_dir(repo_root);
    fsutil::acquire_dir_lock(&lock_dir, label, force)
}

pub fn load_queue_or_default(path: &Path) -> Result<QueueFile> {
    if !path.exists() {
        return Ok(QueueFile::default());
    }
    load_queue(path)
}

pub fn load_queue(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    let queue = serde_json::from_str::<QueueFile>(&raw)
        .with_context(|| format!("parse queue {} as JSON", path.display()))?;
    Ok(queue)
}

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(())
}

pub fn validate_queue(queue: &QueueFile, id_prefix: &str, id_width: usize) -> Result<()> {
    if queue.version != 1 {
        bail!("Unsupported queue.json version: {}. Ralph requires version 1. Update the 'version' field in .ralph/queue.json.", queue.version);
    }
    if id_width == 0 {
        bail!("Invalid id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width.");
    }

    let expected_prefix = normalize_prefix(id_prefix);
    if expected_prefix.is_empty() {
        bail!("Empty id_prefix: prefix is required. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix.");
    }

    let mut seen = HashSet::new();
    for (idx, task) in queue.tasks.iter().enumerate() {
        validate_task_required_fields(idx, task)?;
        validate_task_id(idx, &task.id, &expected_prefix, id_width)?;

        if task.status == TaskStatus::Rejected {
            continue;
        }

        let key = task.id.trim().to_string();
        if !seen.insert(key.clone()) {
            bail!("Duplicate task ID detected: {}. Ensure each task in .ralph/queue.json has a unique ID.", key);
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

        let active_ids: HashSet<&str> = active
            .tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Rejected)
            .map(|t| t.id.trim())
            .collect();
        for task in &done.tasks {
            if task.status == TaskStatus::Rejected {
                continue;
            }
            let id = task.id.trim();
            if active_ids.contains(id) {
                bail!("Duplicate task ID detected across queue and done: {}. Ensure task IDs are unique across .ralph/queue.json and .ralph/done.json.", id);
            }
        }
    }

    // Validate dependencies
    validate_dependencies(active, done)?;

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
        if task.status == TaskStatus::Rejected {
            continue;
        }
        if value > max_value {
            max_value = value;
        }
    }
    if let Some(done) = done {
        for (idx, task) in done.tasks.iter().enumerate() {
            let value = validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
            if task.status == TaskStatus::Rejected {
                continue;
            }
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

    validate_queue_set(&active, Some(&done), id_prefix, id_width)?;

    let mut moved_ids = Vec::new();
    let mut remaining = Vec::new();

    for task in active.tasks.into_iter() {
        if task.status != TaskStatus::Done {
            remaining.push(task);
            continue;
        }

        let key = task.id.trim().to_string();
        moved_ids.push(key);
        done.tasks.push(task);
    }

    active.tasks = remaining;

    if moved_ids.is_empty() {
        return Ok(ArchiveReport { moved_ids });
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(ArchiveReport { moved_ids })
}

/// Complete a single task and move it to the done archive.
///
/// Validates that the task exists in the active queue, is in a valid
/// starting state (todo or doing), updates its status and timestamps,
/// appends any provided notes, and atomically moves it from queue.json
/// to the end of done.json.
///
/// # Arguments
/// * `queue_path` - Path to the active queue file
/// * `done_path` - Path to the done archive file (created if missing)
/// * `task_id` - ID of the task to complete
/// * `status` - Terminal status (Done or Rejected)
/// * `now_rfc3339` - Current UTC timestamp as RFC3339 string
/// * `notes` - Optional notes to append to the task
/// * `id_prefix` - Expected task ID prefix (e.g., "RQ")
/// * `id_width` - Expected numeric width for task IDs (e.g., 4)
#[allow(clippy::too_many_arguments)]
pub fn complete_task(
    queue_path: &Path,
    done_path: &Path,
    task_id: &str,
    status: TaskStatus,
    now_rfc3339: &str,
    notes: &[String],
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    // Validate the completion status is terminal
    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            // Valid terminal statuses
        }
        TaskStatus::Todo | TaskStatus::Doing => {
            bail!(
                "Invalid completion status: only 'done' or 'rejected' are allowed. Got: {:?}. Use 'ralph queue complete {} done' or 'ralph queue complete {} rejected'.",
                status, task_id, task_id
            );
        }
    }

    // Load and validate the active queue
    let mut active = load_queue(queue_path)?;
    validate_queue(&active, id_prefix, id_width)?;

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    // Find the task in the active queue
    let task_idx = active
        .tasks
        .iter()
        .position(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "task not found in active queue: {}. Ensure the task exists in .ralph/queue.json.",
                needle
            )
        })?;

    let task = &active.tasks[task_idx];

    // Validate that the task is in a state that can be completed
    match task.status {
        TaskStatus::Todo | TaskStatus::Doing => {
            // Valid starting states
        }
        TaskStatus::Done | TaskStatus::Rejected => {
            bail!(
                "task {} is already in a terminal state: {:?}. Cannot complete a task that is already done or rejected.",
                needle, task.status
            );
        }
    }

    // Remove the task from the active queue
    let mut completed_task = active.tasks.remove(task_idx);

    // Update the task with completion status and timestamps
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    completed_task.status = status;
    completed_task.updated_at = Some(now.to_string());
    completed_task.completed_at = Some(now.to_string());

    // Append redacted notes
    for note in notes {
        let redacted = redaction::redact_text(note);
        let trimmed = redacted.trim();
        if !trimmed.is_empty() {
            completed_task.notes.push(trimmed.to_string());
        }
    }

    // Load or create the done archive
    let mut done = load_queue_or_default(done_path)?;

    // Validate the combined queue set (to catch duplicate IDs)
    validate_queue_set(&active, Some(&done), id_prefix, id_width)?;

    // Append the completed task to the done archive
    done.tasks.push(completed_task);

    // Save both files atomically
    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(())
}

pub fn set_status(
    queue: &mut QueueFile,
    task_id: &str,
    status: TaskStatus,
    now_rfc3339: &str,
    note: Option<&str>,
) -> Result<()> {
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    task.status = status;
    task.updated_at = Some(now.to_string());

    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            task.completed_at = Some(now.to_string());
        }
        TaskStatus::Todo | TaskStatus::Doing => {
            task.completed_at = None;
        }
    }

    if let Some(note) = note {
        let redacted = redaction::redact_text(note);
        let trimmed = redacted.trim();
        if !trimmed.is_empty() {
            task.notes.push(trimmed.to_string());
        }
    }

    Ok(())
}

pub fn set_field(
    queue: &mut QueueFile,
    task_id: &str,
    key: &str,
    value: &str,
    now_rfc3339: &str,
) -> Result<()> {
    let key_trimmed = key.trim();
    if key_trimmed.is_empty() {
        bail!("Missing custom field key: a key is required for this operation. Provide a valid key (e.g., 'severity').");
    }
    if key_trimmed.chars().any(|c| c.is_whitespace()) {
        bail!(
            "Invalid custom field key: '{}' contains whitespace. Custom field keys must not contain whitespace.",
            key_trimmed
        );
    }

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided.");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    task.custom_fields
        .insert(key_trimmed.to_string(), value.trim().to_string());
    task.updated_at = Some(now.to_string());

    Ok(())
}

pub fn find_task<'a>(queue: &'a QueueFile, task_id: &str) -> Option<&'a Task> {
    let needle = task_id.trim();
    if needle.is_empty() {
        return None;
    }
    queue.tasks.iter().find(|task| task.id.trim() == needle)
}

pub fn find_task_across<'a>(
    active: &'a QueueFile,
    done: Option<&'a QueueFile>,
    task_id: &str,
) -> Option<&'a Task> {
    find_task(active, task_id).or_else(|| done.and_then(|d| find_task(d, task_id)))
}

fn normalize_scope(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn next_todo_task(queue: &QueueFile) -> Option<&Task> {
    queue
        .tasks
        .iter()
        .find(|task| task.status == TaskStatus::Todo)
}

pub fn task_id_set(queue: &QueueFile) -> HashSet<String> {
    let mut set = HashSet::new();
    for task in &queue.tasks {
        let id = task.id.trim();
        if id.is_empty() {
            continue;
        }
        set.insert(id.to_string());
    }
    set
}

pub fn added_tasks(before: &HashSet<String>, after: &QueueFile) -> Vec<(String, String)> {
    let mut added = Vec::new();
    for task in &after.tasks {
        let id = task.id.trim();
        if id.is_empty() || before.contains(id) {
            continue;
        }
        added.push((id.to_string(), task.title.trim().to_string()));
    }
    added
}

pub fn backfill_missing_fields(
    queue: &mut QueueFile,
    new_task_ids: &[String],
    default_request: &str,
    now_utc: &str,
) {
    let now = now_utc.trim();
    if now.is_empty() {
        return;
    }

    for task in queue.tasks.iter_mut() {
        if !new_task_ids.contains(&task.id.trim().to_string()) {
            continue;
        }

        if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
            let req = default_request.trim();
            if !req.is_empty() {
                task.request = Some(req.to_string());
            }
        }

        if task.created_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.created_at = Some(now.to_string());
        }

        if task.updated_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.updated_at = Some(now.to_string());
        }
    }
}

pub fn sort_tasks_by_priority(queue: &mut QueueFile, descending: bool) {
    queue.tasks.sort_by(|a, b| {
        // Since Ord has Critical > High > Medium > Low (semantically),
        // we reverse for descending to put higher priority first
        let ord = if descending {
            a.priority.cmp(&b.priority).reverse()
        } else {
            a.priority.cmp(&b.priority)
        };
        // Use task ID as tiebreaker for stable ordering
        match ord {
            std::cmp::Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        }
    });
}

pub fn filter_tasks<'a>(
    queue: &'a QueueFile,
    statuses: &[TaskStatus],
    tags: &[String],
    scopes: &[String],
    limit: Option<usize>,
) -> Vec<&'a Task> {
    let status_filter: HashSet<TaskStatus> = statuses.iter().copied().collect();
    let tag_filter: HashSet<String> = tags
        .iter()
        .map(|tag| normalize_tag(tag))
        .filter(|tag| !tag.is_empty())
        .collect();
    let scope_filter: Vec<String> = scopes
        .iter()
        .map(|scope| normalize_scope(scope))
        .filter(|scope| !scope.is_empty())
        .collect();

    let has_status_filter = !status_filter.is_empty();
    let has_tag_filter = !tag_filter.is_empty();
    let has_scope_filter = !scope_filter.is_empty();

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
        if has_scope_filter
            && !task.scope.iter().any(|scope| {
                let hay = normalize_scope(scope);
                scope_filter.iter().any(|needle| hay.contains(needle))
            })
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

pub fn search_tasks<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    query: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<Vec<&'a Task>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let matcher = if use_regex {
        let regex = Regex::new(query).with_context(|| {
            format!(
                "Invalid regular expression pattern '{}'. Provide a valid regex pattern or use substring search without --regex.",
                query
            )
        })?;
        SearchMatcher::Regex(regex)
    } else {
        let pattern = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        SearchMatcher::Substring {
            pattern,
            case_sensitive,
        }
    };

    let mut results = Vec::new();
    for task in tasks {
        if matcher.matches(&task.title)
            || task.evidence.iter().any(|e| matcher.matches(e))
            || task.plan.iter().any(|p| matcher.matches(p))
            || task.notes.iter().any(|n| matcher.matches(n))
        {
            results.push(task);
        }
    }

    Ok(results)
}

enum SearchMatcher {
    Regex(Regex),
    Substring {
        pattern: String,
        case_sensitive: bool,
    },
}

impl SearchMatcher {
    fn matches(&self, text: &str) -> bool {
        let haystack = text.trim();
        if haystack.is_empty() {
            return false;
        }
        match self {
            SearchMatcher::Regex(re) => re.is_match(haystack),
            SearchMatcher::Substring {
                pattern,
                case_sensitive,
            } => {
                if *case_sensitive {
                    haystack.contains(pattern)
                } else {
                    haystack.to_lowercase().contains(pattern)
                }
            }
        }
    }
}

fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().to_lowercase()
}

fn validate_task_required_fields(index: usize, task: &Task) -> Result<()> {
    if task.id.trim().is_empty() {
        bail!("Missing task ID: task at index {} is missing an 'id' field. Add a valid ID (e.g., 'RQ-0001') to the task.", index);
    }
    if task.title.trim().is_empty() {
        bail!("Missing task title: task {} (index {}) is missing a 'title' field. Add a descriptive title (e.g., 'Fix login bug').", task.id, index);
    }
    ensure_list_non_empty("tags", index, &task.id, &task.tags)?;
    ensure_list_non_empty("scope", index, &task.id, &task.scope)?;
    ensure_list_non_empty("evidence", index, &task.id, &task.evidence)?;
    ensure_list_non_empty("plan", index, &task.id, &task.plan)?;
    ensure_field_present("request", index, &task.id, task.request.as_deref())?;

    // Validate custom field keys
    for (key_idx, (key, _value)) in task.custom_fields.iter().enumerate() {
        let key_trimmed = key.trim();
        if key_trimmed.is_empty() {
            bail!(
                "Empty custom field key: task {} (index {}) has an empty key at custom_fields[{}]. Remove the empty key or provide a valid key.",
                task.id, index, key_idx
            );
        }
        if key_trimmed.chars().any(|c| c.is_whitespace()) {
            bail!(
                "Invalid custom field key: task {} (index {}) has a key with whitespace at custom_fields[{}]: '{}'. Custom field keys must not contain whitespace.",
                task.id, index, key_idx, key_trimmed
            );
        }
    }

    if let Some(ts) = task.created_at.as_deref() {
        validate_rfc3339("created_at", index, &task.id, ts)?;
    } else {
        bail!("Missing created_at: task {} (index {}) is missing the 'created_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13Z').", task.id, index);
    }

    if let Some(ts) = task.updated_at.as_deref() {
        validate_rfc3339("updated_at", index, &task.id, ts)?;
    } else {
        bail!("Missing updated_at: task {} (index {}) is missing the 'updated_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13Z').", task.id, index);
    }

    if let Some(ts) = task.completed_at.as_deref() {
        validate_rfc3339("completed_at", index, &task.id, ts)?;
    }

    Ok(())
}

fn validate_rfc3339(label: &str, index: usize, id: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!(
            "Missing {}: task {} (index {}) requires a non-empty '{}' field. Add a valid RFC3339 UTC timestamp (e.g., '2026-01-19T05:23:13Z').",
            label,
            id,
            index,
            label
        );
    }
    OffsetDateTime::parse(trimmed, &Rfc3339).with_context(|| {
        format!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={}). Example: '2026-01-19T05:23:13Z'.",
            index, label, trimmed, id
        )
    })?;
    Ok(())
}

fn ensure_list_non_empty(label: &str, index: usize, id: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        bail!("Empty {}: task {} (index {}) '{}' field cannot be empty. Add at least one item to the list.", label, id, index, label);
    }
    for (i, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            bail!(
                "Empty {} item: task {} (index {}) contains an empty string at {}[{}]. Remove the empty item or add content.",
                label,
                id,
                index,
                label,
                i
            );
        }
    }
    Ok(())
}

fn ensure_field_present(label: &str, index: usize, id: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(()),
        _ => bail!(
            "Missing {}: task {} (index {}) requires a non-empty '{}' field. Ensure the field is present and has a value.",
            label,
            id,
            index,
            label
        ),
    }
}

fn validate_task_id(
    index: usize,
    raw_id: &str,
    expected_prefix: &str,
    id_width: usize,
) -> Result<u32> {
    let trimmed = raw_id.trim();
    let (prefix_raw, num_raw) = trimmed.split_once('-').ok_or_else(|| {
        anyhow!(
            "Invalid task ID format: task at index {} has ID '{}' which is missing a '-'. Task IDs must follow the 'PREFIX-NUMBER' format (e.g., '{}-0001').",
            index,
            trimmed,
            expected_prefix
        )
    })?;

    let prefix = prefix_raw.trim().to_uppercase();
    if prefix != expected_prefix {
        bail!(
            "Mismatched task ID prefix: task at index {} has prefix '{}' but expected '{}'. Update the task ID to '{}' or change the prefix in .ralph/config.json.",
            index,
            prefix,
            expected_prefix,
            format_id(expected_prefix, 1, id_width)
        );
    }

    let num = num_raw.trim();
    if num.len() != id_width {
        bail!(
            "Invalid task ID width: task at index {} has a numeric suffix of length {} but expected {}. Pad the numeric part with leading zeros (e.g., '{}').",
            index,
            num.len(),
            id_width,
            format_id(expected_prefix, num.parse().unwrap_or(1), id_width)
        );
    }
    if !num.chars().all(|c| c.is_ascii_digit()) {
        bail!(
            "Invalid task ID: task at index {} has non-digit characters in its numeric suffix '{}'. Ensure the ID suffix contains only digits (e.g., '0001').",
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

/// Check if all dependencies for a task are met (referenced tasks are Done).
/// Dependencies are met if the referenced task is Done in either queue or done archive.
pub fn are_dependencies_met(task: &Task, active: &QueueFile, done: Option<&QueueFile>) -> bool {
    let task_id = task.id.trim();
    for dep_id in &task.depends_on {
        let dep_id = dep_id.trim();
        if dep_id.is_empty() {
            continue;
        }
        // Skip self-references (will be caught by validation)
        if dep_id == task_id {
            return false;
        }
        // Check if dependency exists and is Done or Rejected in active queue
        let met = active.tasks.iter().any(|t| {
            t.id.trim() == dep_id
                && (t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
        });
        if met {
            continue;
        }
        // Check if dependency exists and is Done or Rejected in done archive
        let done_met = done.is_some_and(|d| {
            d.tasks.iter().any(|t| {
                t.id.trim() == dep_id
                    && (t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
            })
        });
        if !done_met {
            return false;
        }
    }
    true
}

/// Get all tasks that depend on the given task ID (recursively).
/// Returns a list of task IDs that depend on the root task.
pub fn get_dependents(root_id: &str, active: &QueueFile, done: Option<&QueueFile>) -> Vec<String> {
    let mut dependents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let root_id = root_id.trim();

    fn collect_dependents(
        task_id: &str,
        active: &QueueFile,
        done: Option<&QueueFile>,
        dependents: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(task_id) {
            return;
        }
        visited.insert(task_id.to_string());

        // Check all tasks in active queue
        for task in &active.tasks {
            let current_id = task.id.trim();
            if task.depends_on.iter().any(|d| d.trim() == task_id) {
                if !dependents.contains(&current_id.to_string()) {
                    dependents.push(current_id.to_string());
                }
                collect_dependents(current_id, active, done, dependents, visited);
            }
        }

        // Check all tasks in done archive
        if let Some(done_file) = done {
            for task in &done_file.tasks {
                let current_id = task.id.trim();
                if task.depends_on.iter().any(|d| d.trim() == task_id) {
                    if !dependents.contains(&current_id.to_string()) {
                        dependents.push(current_id.to_string());
                    }
                    collect_dependents(current_id, active, done, dependents, visited);
                }
            }
        }
    }

    collect_dependents(root_id, active, done, &mut dependents, &mut visited);
    dependents
}

fn validate_dependencies(active: &QueueFile, done: Option<&QueueFile>) -> Result<()> {
    let all_task_ids: HashSet<&str> = active
        .tasks
        .iter()
        .map(|t| t.id.trim())
        .chain(
            done.iter()
                .flat_map(|d| d.tasks.iter().map(|t| t.id.trim())),
        )
        .collect();

    // Build adjacency list for cycle detection
    let mut graph: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();

    for task in &active.tasks {
        let task_id = task.id.trim();
        for dep_id in &task.depends_on {
            let dep_id = dep_id.trim();
            if dep_id.is_empty() {
                continue;
            }

            // Check for self-reference
            if dep_id == task_id {
                bail!(
                    "Self-dependency detected: task {} depends on itself. Remove the self-reference from the depends_on field in .ralph/queue.json.",
                    task_id
                );
            }

            // Check that dependency exists
            if !all_task_ids.contains(dep_id) {
                bail!(
                    "Invalid dependency: task {} depends on non-existent task {}. Ensure the dependency task ID exists in .ralph/queue.json or .ralph/done.json.",
                    task_id,
                    dep_id
                );
            }

            // Build graph for cycle detection
            graph.entry(task_id).or_default().push(dep_id);
        }
    }

    // Also check done archive for dependencies
    if let Some(done_file) = done {
        for task in &done_file.tasks {
            let task_id = task.id.trim();
            for dep_id in &task.depends_on {
                let dep_id = dep_id.trim();
                if dep_id.is_empty() {
                    continue;
                }

                // Check for self-reference
                if dep_id == task_id {
                    bail!(
                        "Self-dependency detected: task {} depends on itself. Remove the self-reference from the depends_on field in .ralph/done.json.",
                        task_id
                    );
                }

                // Check that dependency exists
                if !all_task_ids.contains(dep_id) {
                    bail!(
                        "Invalid dependency: task {} depends on non-existent task {}. Ensure the dependency task ID exists in .ralph/queue.json or .ralph/done.json.",
                        task_id,
                        dep_id
                    );
                }

                // Build graph for cycle detection
                graph.entry(task_id).or_default().push(dep_id);
            }
        }
    }

    // Detect cycles using DFS
    let mut visited = std::collections::HashSet::new();
    let mut rec_stack = std::collections::HashSet::new();

    for node in graph.keys() {
        if has_cycle(node, &graph, &mut visited, &mut rec_stack) {
            bail!(
                "Circular dependency detected involving task {}. Task dependencies must form a DAG (no cycles). Review the depends_on fields to break the cycle.",
                node
            );
        }
    }

    Ok(())
}

fn has_cycle(
    node: &str,
    graph: &std::collections::HashMap<&str, Vec<&str>>,
    visited: &mut std::collections::HashSet<String>,
    rec_stack: &mut std::collections::HashSet<String>,
) -> bool {
    let node_key = node.to_string();
    visited.insert(node_key.clone());
    rec_stack.insert(node_key.clone());

    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors.iter() {
            if !visited.contains(*neighbor) {
                if has_cycle(neighbor, graph, visited, rec_stack) {
                    return true;
                }
            } else if rec_stack.contains(*neighbor) {
                return true;
            }
        }
    }

    rec_stack.remove(&node_key);
    false
}

fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

/// Prune tasks from the done archive based on age, status, and keep-last rules.
///
/// This function loads the done archive, applies pruning rules, and optionally
/// saves the result. Pruning preserves the original order of remaining tasks.
///
/// # Arguments
/// * `done_path` - Path to the done archive file
/// * `options` - Pruning options (age filter, status filter, keep-last, dry-run)
///
/// # Returns
/// A `PruneReport` containing the IDs of pruned and kept tasks.
pub fn prune_done_tasks(done_path: &Path, options: PruneOptions) -> Result<PruneReport> {
    let mut done = load_queue_or_default(done_path)?;
    let report = prune_done_queue(&mut done.tasks, &options)?;

    if !options.dry_run && !report.pruned_ids.is_empty() {
        save_queue(done_path, &done)?;
    }

    Ok(report)
}

/// Core pruning logic for a task list.
///
/// Tasks are sorted by completion date (most recent first), then keep-last
/// protection is applied, then age and status filters. The original order of
/// remaining tasks is preserved.
fn prune_done_queue(tasks: &mut Vec<Task>, options: &PruneOptions) -> Result<PruneReport> {
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let now_dt = parse_completed_at(&now).unwrap_or_else(OffsetDateTime::now_utc);

    let age_duration = options.age_days.map(|d| Duration::days(d as i64));

    // Sort indices by completion date descending (most recent first)
    let mut indices: Vec<usize> = (0..tasks.len()).collect();
    indices.sort_by(|&i, &j| compare_completed_desc(&tasks[i], &j, tasks));

    // Apply keep-last protection using task IDs instead of indices
    let mut keep_set: HashSet<String> = HashSet::new();
    if let Some(keep_n) = options.keep_last {
        for &idx in indices.iter().take(keep_n as usize) {
            keep_set.insert(tasks[idx].id.clone());
        }
    }

    let mut pruned_ids = Vec::new();
    let mut kept_ids = Vec::new();

    // Filter tasks - iterate by index to avoid borrow issues
    let mut keep_mask = vec![false; tasks.len()];
    for (idx, task) in tasks.iter().enumerate() {
        // Check keep-last protection first
        if keep_set.contains(&task.id) {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        // Check status filter
        if !options.statuses.is_empty() && !options.statuses.contains(&task.status) {
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        // Check age filter
        if let Some(ref completed_at) = task.completed_at {
            if let Some(task_dt) = parse_completed_at(completed_at) {
                if let Some(age_dur) = age_duration {
                    // Calculate age: now - task_dt
                    // Use checked_sub to handle potential underflow gracefully
                    let age = if now_dt >= task_dt {
                        now_dt - task_dt
                    } else {
                        // Task is in the future (clock skew), treat as 0 age
                        Duration::ZERO
                    };
                    if age < age_dur {
                        // Too recent to prune
                        keep_mask[idx] = true;
                        kept_ids.push(task.id.clone());
                        continue;
                    }
                }
            } else {
                // Invalid completed_at - keep for safety (don't prune by age)
                keep_mask[idx] = true;
                kept_ids.push(task.id.clone());
                continue;
            }
        } else {
            // Missing completed_at - keep for safety (don't prune by age)
            keep_mask[idx] = true;
            kept_ids.push(task.id.clone());
            continue;
        }

        // Task passes all filters - mark for pruning
        pruned_ids.push(task.id.clone());
    }

    // Remove pruned tasks while preserving order
    let mut new_tasks = Vec::new();
    for (idx, task) in tasks.drain(..).enumerate() {
        if keep_mask[idx] {
            new_tasks.push(task);
        }
    }
    *tasks = new_tasks;

    Ok(PruneReport {
        pruned_ids,
        kept_ids,
    })
}

/// Parse an RFC3339 timestamp into OffsetDateTime.
/// Returns None if the timestamp is invalid.
fn parse_completed_at(ts: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(ts, &Rfc3339).ok()
}

/// Compare two tasks by completion date for descending sort.
/// Tasks with valid completed_at come first (most recent), then tasks with
/// missing or invalid timestamps (treated as oldest).
fn compare_completed_desc(a: &Task, idx_b: &usize, tasks: &[Task]) -> Ordering {
    let b = &tasks[*idx_b];
    let a_ts = parse_completed_at;
    let b_ts = parse_completed_at;

    match (a.completed_at.as_deref(), b.completed_at.as_deref()) {
        (Some(ts_a), Some(ts_b)) => {
            match (a_ts(ts_a), b_ts(ts_b)) {
                (Some(dt_a), Some(dt_b)) => dt_a.cmp(&dt_b).reverse(), // Most recent first
                (Some(_), None) => Ordering::Less,                     // Valid comes before invalid
                (None, Some(_)) => Ordering::Greater,                  // Invalid comes after valid
                (None, None) => Ordering::Equal,
            }
        }
        (Some(_), None) => Ordering::Less, // Valid comes before missing
        (None, Some(_)) => Ordering::Greater, // Missing comes after valid
        (None, None) => Ordering::Equal,
    }
}

#[allow(dead_code)]
pub fn delete_task(queue: &mut QueueFile, task_id: &str) -> Result<bool> {
    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let original_len = queue.tasks.len();
    queue.tasks.retain(|t| t.id.trim() != needle);

    let deleted = queue.tasks.len() < original_len;
    if !deleted {
        bail!("task not found: {}", needle);
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    /// Create a task with a specific completion timestamp.
    fn done_task_with_completed(id: &str, completed_at: &str) -> Task {
        let mut t = task_with(id, TaskStatus::Done, vec!["done".to_string()]);
        t.completed_at = Some(completed_at.to_string());
        t
    }

    /// Create a task without a completion timestamp.
    fn done_task_missing_completed(id: &str) -> Task {
        let mut t = task_with(id, TaskStatus::Done, vec!["done".to_string()]);
        t.completed_at = None;
        t
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: HashMap::new(),
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
    fn validate_rejects_missing_request() {
        let mut task = task("RQ-0001");
        task.request = None;
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("Missing request"));
    }

    #[test]
    fn validate_rejects_empty_request() {
        let mut task = task("RQ-0001");
        task.request = Some("".to_string());
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("Missing request"));
    }

    #[test]
    fn validate_rejects_missing_created_at() {
        let mut task = task("RQ-0001");
        task.created_at = None;
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("Missing created_at"));
    }

    #[test]
    fn validate_rejects_missing_updated_at() {
        let mut task = task("RQ-0001");
        task.updated_at = None;
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("Missing updated_at"));
    }

    #[test]
    fn validate_rejects_invalid_rfc3339() {
        let mut task = task("RQ-0001");
        task.created_at = Some("not a date".to_string());
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
    }

    #[test]
    fn set_status_rejects_invalid_rfc3339() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let err =
            set_status(&mut queue, "RQ-0001", TaskStatus::Doing, "invalid", None).unwrap_err();
        assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
        Ok(())
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
            Some("started"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Doing);
        assert_eq!(t.updated_at.as_deref(), Some(now));
        assert_eq!(t.completed_at, None);
        assert_eq!(t.notes, vec!["started".to_string()]);

        let now2 = "2026-01-17T00:02:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Done,
            now2,
            Some("completed"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Done);
        assert_eq!(t.updated_at.as_deref(), Some(now2));
        assert_eq!(t.completed_at.as_deref(), Some(now2));
        assert!(t.notes.iter().any(|n| n == "completed"));

        Ok(())
    }

    #[test]
    fn set_status_redacts_note() -> Result<()> {
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
            Some("API_KEY=abc12345"),
        )?;

        let t = &queue.tasks[0];
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
            TaskStatus::Doing,
            now,
            Some("`make ci` failed"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.notes, vec!["`make ci` failed".to_string()]);

        Ok(())
    }

    #[test]
    fn validate_queue_set_rejects_cross_file_duplicates() {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        let err = validate_queue_set(&active, Some(&done), "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("Duplicate task ID detected across queue and done"));
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
    fn backfill_missing_fields_populates_request() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].request = None;

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T00:00:00Z",
        );

        assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_populates_timestamps() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].created_at = None;
        queue.tasks[0].updated_at = None;

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_skips_existing_values() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "new request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(queue.tasks[0].request, Some("test request".to_string()));
        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T00:00:00Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T00:00:00Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_only_affects_specified_ids() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.request = None;
        let t2 = task("RQ-0002");
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![t1, t2],
        };

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "backfilled request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(
            queue.tasks[0].request,
            Some("backfilled request".to_string())
        );
        assert_eq!(queue.tasks[1].request, Some("test request".to_string()));
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_handles_empty_string_as_missing() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].request = Some("".to_string());
        queue.tasks[0].created_at = Some("".to_string());
        queue.tasks[0].updated_at = Some("".to_string());

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_empty_now_skips() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].created_at = None;
        queue.tasks[0].updated_at = None;

        backfill_missing_fields(&mut queue, &["RQ-0001".to_string()], "default request", "");

        assert_eq!(queue.tasks[0].created_at, None);
        assert_eq!(queue.tasks[0].updated_at, None);
        Ok(())
    }

    #[test]
    fn sort_tasks_by_priority_descending() {
        use crate::contracts::TaskPriority;
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec![]),
                task_with("RQ-0002", TaskStatus::Todo, vec![]),
                task_with("RQ-0003", TaskStatus::Todo, vec![]),
            ],
        };
        queue.tasks[0].priority = TaskPriority::Low;
        queue.tasks[1].priority = TaskPriority::Critical;
        queue.tasks[2].priority = TaskPriority::High;

        sort_tasks_by_priority(&mut queue, true);

        assert_eq!(queue.tasks[0].id, "RQ-0002"); // Critical first
        assert_eq!(queue.tasks[1].id, "RQ-0003"); // High second
        assert_eq!(queue.tasks[2].id, "RQ-0001"); // Low last
    }

    #[test]
    fn sort_tasks_by_priority_ascending() {
        use crate::contracts::TaskPriority;
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec![]),
                task_with("RQ-0002", TaskStatus::Todo, vec![]),
                task_with("RQ-0003", TaskStatus::Todo, vec![]),
            ],
        };
        queue.tasks[0].priority = TaskPriority::Low;
        queue.tasks[1].priority = TaskPriority::Critical;
        queue.tasks[2].priority = TaskPriority::High;

        sort_tasks_by_priority(&mut queue, false);

        assert_eq!(queue.tasks[0].id, "RQ-0001"); // Low first
        assert_eq!(queue.tasks[1].id, "RQ-0003"); // High second
        assert_eq!(queue.tasks[2].id, "RQ-0002"); // Critical last
    }

    #[test]
    fn task_defaults_to_medium_priority() {
        use crate::contracts::TaskPriority;
        let task = task("RQ-0001");
        assert_eq!(task.priority, TaskPriority::Medium);
    }

    #[test]
    fn priority_ord_implements_correct_ordering() {
        use crate::contracts::TaskPriority;
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Medium);
        assert!(TaskPriority::Medium > TaskPriority::Low);
    }

    #[test]
    fn search_tasks_substring_case_insensitive() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix login bug".to_string();
        t1.evidence = vec!["Users report authentication failure".to_string()];
        t1.plan = vec!["Debug auth service".to_string()];
        t1.notes = vec!["Check token expiration".to_string()];

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();
        t2.evidence = vec!["Documentation needs refresh".to_string()];

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = search_tasks(tasks, "LOGIN", false, false)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_substring_case_sensitive() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix Login bug".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Fix login bug".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = search_tasks(tasks, "Login", false, true)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_regex_valid_pattern() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix RQ-1234 bug".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = search_tasks(tasks, r"RQ-\d{4}", true, false)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_regex_invalid_pattern() {
        let t1 = task("RQ-0001");
        let tasks: Vec<&Task> = vec![&t1];
        let err = search_tasks(tasks, r"(?P<unclosed", true, false).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Invalid regular expression"));
    }

    #[test]
    fn search_tasks_matches_all_fields() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();
        t1.evidence = vec!["Login fails".to_string()];
        t1.plan = vec!["Debug token".to_string()];
        t1.notes = vec!["Checked logs".to_string()];

        let tasks: Vec<&Task> = vec![&t1];

        // Title match
        let results = search_tasks(tasks.iter().copied(), "authentication", false, false)?;
        assert_eq!(results.len(), 1);

        // Evidence match
        let results = search_tasks(tasks.iter().copied(), "login fails", false, false)?;
        assert_eq!(results.len(), 1);

        // Plan match
        let results = search_tasks(tasks.iter().copied(), "debug token", false, false)?;
        assert_eq!(results.len(), 1);

        // Notes match
        let results = search_tasks(tasks.iter().copied(), "checked logs", false, false)?;
        assert_eq!(results.len(), 1);

        Ok(())
    }

    #[test]
    fn search_tasks_empty_query_returns_empty() -> Result<()> {
        let t1 = task("RQ-0001");
        let tasks: Vec<&Task> = vec![&t1];
        let results = search_tasks(tasks.iter().copied(), "", false, false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn search_tasks_no_match_returns_empty() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        let results = search_tasks(tasks.iter().copied(), "database", false, false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn search_tasks_regex_case_sensitive_flag() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix LOGIN bug".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        // Regex is case-sensitive by default, --match-case only affects substring mode
        let results = search_tasks(tasks.iter().copied(), "LOGIN", true, false)?;
        assert_eq!(results.len(), 1);

        let results = search_tasks(tasks.iter().copied(), "login", true, false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn validate_queue_allows_duplicate_if_one_is_rejected() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
                task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]),
            ],
        };
        assert!(validate_queue(&queue, "RQ", 4).is_ok());
    }

    #[test]
    fn validate_queue_set_allows_duplicate_across_files_if_rejected() {
        let active = QueueFile {
            version: 1,
            tasks: vec![task_with(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["tag".to_string()],
            )],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task_with(
                "RQ-0001",
                TaskStatus::Rejected,
                vec!["tag".to_string()],
            )],
        };
        assert!(validate_queue_set(&active, Some(&done), "RQ", 4).is_ok());

        let active2 = QueueFile {
            version: 1,
            tasks: vec![task_with(
                "RQ-0001",
                TaskStatus::Rejected,
                vec!["tag".to_string()],
            )],
        };
        let done2 = QueueFile {
            version: 1,
            tasks: vec![task_with(
                "RQ-0001",
                TaskStatus::Done,
                vec!["tag".to_string()],
            )],
        };
        assert!(validate_queue_set(&active2, Some(&done2), "RQ", 4).is_ok());
    }

    #[test]
    fn next_id_across_ignores_rejected() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
                task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]),
            ],
        };
        let next = next_id_across(&active, None, "RQ", 4)?;
        assert_eq!(next, "RQ-0002");
        Ok(())
    }

    #[test]
    fn next_id_across_includes_done_non_rejected() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task_with(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["tag".to_string()],
            )],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0005", TaskStatus::Done, vec!["tag".to_string()]),
                task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]),
            ],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4)?;
        assert_eq!(next, "RQ-0006");
        Ok(())
    }

    #[test]
    fn complete_task_moves_task_from_queue_to_done() -> Result<()> {
        use tempfile::TempDir;

        // Create a temp directory to hold queue and done files
        let temp_dir = TempDir::new()?;
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        std::fs::write(&queue_path, queue_json)?;

        let now = "2026-01-20T12:00:00Z";
        complete_task(
            &queue_path,
            &done_path,
            "RQ-0001",
            TaskStatus::Done,
            now,
            &["Test note".to_string()],
            "RQ",
            4,
        )?;

        // Verify task was removed from queue
        let queue_content = std::fs::read_to_string(&queue_path)?;
        let queue: QueueFile = serde_json::from_str(&queue_content)?;
        assert_eq!(queue.tasks.len(), 0);

        // Verify task was added to done with correct status
        let done_content = std::fs::read_to_string(&done_path)?;
        let done: QueueFile = serde_json::from_str(&done_content)?;
        assert_eq!(done.tasks.len(), 1);
        assert_eq!(done.tasks[0].id, "RQ-0001");
        assert_eq!(done.tasks[0].status, TaskStatus::Done);
        assert_eq!(done.tasks[0].completed_at.as_deref(), Some(now));
        assert_eq!(done.tasks[0].updated_at.as_deref(), Some(now));
        assert_eq!(done.tasks[0].notes, vec!["Test note"]);

        Ok(())
    }

    #[test]
    fn complete_task_rejects_non_terminal_status() -> Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut queue_file = NamedTempFile::new()?;
        let done_file = NamedTempFile::new()?;

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "doing",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        queue_file.write_all(queue_json.as_bytes())?;
        queue_file.flush()?;

        let now = "2026-01-20T12:00:00Z";
        let err = complete_task(
            queue_file.path(),
            done_file.path(),
            "RQ-0001",
            TaskStatus::Todo, // Invalid - not a terminal status
            now,
            &[],
            "RQ",
            4,
        )
        .unwrap_err();
        assert!(format!("{err}")
            .to_lowercase()
            .contains("invalid completion status"));

        Ok(())
    }

    #[test]
    fn complete_task_rejects_task_already_terminal() -> Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut queue_file = NamedTempFile::new()?;
        let done_file = NamedTempFile::new()?;

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0001",
                    "status": "done",
                    "title": "Test task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "completed_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        queue_file.write_all(queue_json.as_bytes())?;
        queue_file.flush()?;

        let now = "2026-01-20T12:00:00Z";
        let err = complete_task(
            queue_file.path(),
            done_file.path(),
            "RQ-0001",
            TaskStatus::Done,
            now,
            &[],
            "RQ",
            4,
        )
        .unwrap_err();
        assert!(format!("{err}")
            .to_lowercase()
            .contains("already in a terminal state"));

        Ok(())
    }

    #[test]
    fn complete_task_rejects_nonexistent_task() -> Result<()> {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut queue_file = NamedTempFile::new()?;
        let done_file = NamedTempFile::new()?;

        let queue_json = r#"{
            "version": 1,
            "tasks": [
                {
                    "id": "RQ-0002",
                    "status": "todo",
                    "title": "Other task",
                    "priority": "medium",
                    "tags": ["test"],
                    "scope": ["crates/ralph"],
                    "evidence": ["evidence"],
                    "plan": ["plan"],
                    "notes": [],
                    "request": "test request",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "depends_on": [],
                    "custom_fields": {}
                }
            ]
        }"#;
        queue_file.write_all(queue_json.as_bytes())?;
        queue_file.flush()?;

        let now = "2026-01-20T12:00:00Z";
        let err = complete_task(
            queue_file.path(),
            done_file.path(),
            "RQ-0001", // Does not exist
            TaskStatus::Done,
            now,
            &[],
            "RQ",
            4,
        )
        .unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("task not found"));

        Ok(())
    }

    #[test]
    fn prune_by_age_only() {
        // now = 2026-01-20T12:00:00Z
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // 19 days ago
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"), // 10 days ago
            done_task_with_completed("RQ-0003", "2026-01-19T12:00:00Z"), // 1 day ago
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(15),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // RQ-0001 (19 days old) should be pruned
        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);
        assert_eq!(report.kept_ids.len(), 2);
        assert!(report.kept_ids.contains(&"RQ-0002".to_string()));
        assert!(report.kept_ids.contains(&"RQ-0003".to_string()));
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_by_status_only() {
        let mut tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            task_with("RQ-0003", TaskStatus::Rejected, vec!["done".to_string()]),
        ];
        // Set completed_at for rejected task
        tasks[2].completed_at = Some("2026-01-15T12:00:00Z".to_string());

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: None,
            statuses: vec![TaskStatus::Rejected].into_iter().collect(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // Only RQ-0003 (rejected) should be pruned
        assert_eq!(report.pruned_ids, vec!["RQ-0003"]);
        assert_eq!(report.kept_ids.len(), 2);
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_keep_last_protects_recent() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // Oldest
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"),
            done_task_with_completed("RQ-0003", "2026-01-15T12:00:00Z"),
            done_task_with_completed("RQ-0004", "2026-01-19T12:00:00Z"), // Most recent
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: None, // No age filter
            statuses: HashSet::new(),
            keep_last: Some(2), // Keep 2 most recent
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // RQ-0003 and RQ-0004 (2 most recent) should be kept
        // RQ-0001 and RQ-0002 should be pruned
        assert_eq!(report.kept_ids.len(), 2);
        assert!(report.kept_ids.contains(&"RQ-0003".to_string()));
        assert!(report.kept_ids.contains(&"RQ-0004".to_string()));
        assert_eq!(report.pruned_ids.len(), 2);
        assert!(report.pruned_ids.contains(&"RQ-0001".to_string()));
        assert!(report.pruned_ids.contains(&"RQ-0002".to_string()));
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_combined_age_and_status() {
        let mut tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // Old Done
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"), // Recent Done
            task_with("RQ-0003", TaskStatus::Rejected, vec!["done".to_string()]),
            task_with("RQ-0004", TaskStatus::Rejected, vec!["done".to_string()]),
        ];
        tasks[2].completed_at = Some("2026-01-05T12:00:00Z".to_string()); // Old Rejected
        tasks[3].completed_at = Some("2026-01-15T12:00:00Z".to_string()); // Recent Rejected

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(10),
            statuses: vec![TaskStatus::Rejected].into_iter().collect(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // Only RQ-0003 (old rejected) should be pruned
        assert_eq!(report.pruned_ids, vec!["RQ-0003"]);
        assert_eq!(report.kept_ids.len(), 3);
        assert_eq!(done.tasks.len(), 3);
    }

    #[test]
    fn prune_missing_completed_at_kept_for_safety() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // 19 days ago
            done_task_missing_completed("RQ-0002"),                      // Missing completed_at
            done_task_with_completed("RQ-0003", "2026-01-18T12:00:00Z"), // 2 days ago - recent
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5), // Should prune tasks older than 5 days
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // RQ-0001 should be pruned (old with valid timestamp)
        // RQ-0002 should be KEPT (missing completed_at - safety)
        // RQ-0003 should be KEPT (recent - only 2 days old)
        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);
        assert_eq!(report.kept_ids.len(), 2);
        assert!(report.kept_ids.contains(&"RQ-0002".to_string()));
        assert!(report.kept_ids.contains(&"RQ-0003".to_string()));
        assert_eq!(done.tasks.len(), 2);
    }

    #[test]
    fn prune_dry_run_does_not_write_to_disk() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // 19 days ago - should be pruned
            done_task_with_completed("RQ-0002", "2026-01-18T12:00:00Z"), // 2 days ago - should be kept
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5), // Prune older than 5 days
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: true, // Dry run
        };

        let report = prune_done_tasks(&done_path, options).unwrap();

        // Should report what would be pruned
        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);

        // But file should be unchanged
        let done_after = load_queue(&done_path).unwrap();
        assert_eq!(done_after.tasks.len(), 2); // Still has both tasks
    }

    #[test]
    fn prune_preserves_original_order() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // 19 days ago - should be pruned
            done_task_with_completed("RQ-0002", "2026-01-16T12:00:00Z"), // 4 days ago - should be kept
            done_task_with_completed("RQ-0003", "2026-01-18T12:00:00Z"), // 2 days ago - should be kept
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5), // Prune older than 5 days
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        prune_done_tasks(&done_path, options).unwrap();

        let done_after = load_queue(&done_path).unwrap();
        assert_eq!(done_after.tasks.len(), 2);
        // Remaining tasks should preserve original order
        assert_eq!(done_after.tasks[0].id, "RQ-0002");
        assert_eq!(done_after.tasks[1].id, "RQ-0003");
    }

    #[test]
    fn prune_with_keep_last_and_age_combines_filters() {
        let tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"), // Very old
            done_task_with_completed("RQ-0002", "2026-01-10T12:00:00Z"), // Recent
            done_task_with_completed("RQ-0003", "2026-01-15T12:00:00Z"), // Very recent
        ];

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5), // Prune older than 5 days
            statuses: HashSet::new(),
            keep_last: Some(1), // But always keep 1 most recent
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // RQ-0001 should be pruned (old)
        // RQ-0002 should be pruned (old, not protected by keep-last)
        // RQ-0003 should be kept (most recent, protected by keep-last)
        assert_eq!(report.pruned_ids.len(), 2);
        assert!(report.pruned_ids.contains(&"RQ-0001".to_string()));
        assert!(report.pruned_ids.contains(&"RQ-0002".to_string()));
        assert_eq!(report.kept_ids, vec!["RQ-0003"]);
        assert_eq!(done.tasks.len(), 1);
    }

    #[test]
    fn prune_invalid_completed_at_kept_for_safety() {
        let mut tasks = vec![
            done_task_with_completed("RQ-0001", "2026-01-01T12:00:00Z"),
            task_with("RQ-0002", TaskStatus::Done, vec!["done".to_string()]),
        ];
        // Set invalid timestamp
        tasks[1].completed_at = Some("not-a-valid-timestamp".to_string());

        let temp_dir = TempDir::new().unwrap();
        let done_path = temp_dir.path().join("done.json");
        let queue_file = QueueFile {
            version: 1,
            tasks: tasks.clone(),
        };
        save_queue(&done_path, &queue_file).unwrap();

        let options = PruneOptions {
            age_days: Some(5),
            statuses: HashSet::new(),
            keep_last: None,
            dry_run: false,
        };

        let mut done = load_queue(&done_path).unwrap();
        let report = prune_done_queue(&mut done.tasks, &options).unwrap();

        // RQ-0001 should be pruned (old with valid timestamp)
        // RQ-0002 should be KEPT (invalid timestamp - safety)
        assert_eq!(report.pruned_ids, vec!["RQ-0001"]);
        assert_eq!(report.kept_ids, vec!["RQ-0002"]);
        assert_eq!(done.tasks.len(), 1);
    }
}
