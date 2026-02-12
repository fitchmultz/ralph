//! Task queue persistence, validation, and pruning.
//!
//! Responsibilities:
//! - Load, save, and validate queue files in JSON format.
//! - Provide operations for moving completed tasks and pruning history.
//! - Own queue-level helpers such as ID generation and validation.
//!
//! Not handled here:
//! - Directory lock acquisition (see `crate::lock`).
//! - CLI parsing or user interaction.
//! - Runner integration or external command execution.
//!
//! Invariants/assumptions:
//! - Queue files conform to the schema in `crate::contracts`.
//! - Callers hold locks when mutating queue state on disk.

use crate::config::Resolved;
use crate::constants::limits::MAX_QUEUE_BACKUP_FILES;
use crate::contracts::{QueueFile, TaskStatus};
use crate::{fsutil, lock};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static SINGLE_QUOTED_STRING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^a-zA-Z0-9])'([^']*?)'([^a-zA-Z0-9]|$)").unwrap());

static UNQUOTED_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([{,]\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:").unwrap());

static TRAILING_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",(\s*[}\]])").unwrap());

static TRAILING_COMMA_NEWLINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r",(\s*)\n(\s*[}\]])").unwrap());

pub mod graph;
pub mod hierarchy;
pub mod operations;
pub mod prune;
pub mod repair;
pub mod search;
pub mod size_check;
pub mod validation;

pub use graph::*;
pub use operations::*;
pub use prune::{PruneOptions, PruneReport, prune_done_tasks};
pub use repair::*;
pub use search::{
    SearchOptions, filter_tasks, fuzzy_search_tasks, search_tasks, search_tasks_with_options,
};
pub use size_check::{
    SizeCheckResult, check_queue_size, count_threshold_or_default, print_size_warning_if_needed,
    size_threshold_or_default,
};
pub use validation::{ValidationWarning, log_warnings, validate_queue, validate_queue_set};

const QUEUE_BACKUP_PREFIX: &str = "queue.json.backup.";

// Pruning types live in `queue::prune` (re-exported from this module).

pub fn acquire_queue_lock(repo_root: &Path, label: &str, force: bool) -> Result<lock::DirLock> {
    let lock_dir = lock::queue_lock_dir(repo_root);
    lock::acquire_dir_lock(&lock_dir, label, force)
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
    let queue = crate::jsonc::parse_jsonc::<QueueFile>(&raw, &format!("queue {}", path.display()))?;
    Ok(queue)
}

/// Load queue with automatic repair for common JSON errors.
/// Attempts to fix trailing commas and other common agent-induced mistakes.
pub fn load_queue_with_repair(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;

    // Try JSONC parsing first (handles both valid JSON and JSONC with comments)
    match crate::jsonc::parse_jsonc::<QueueFile>(&raw, &format!("queue {}", path.display())) {
        Ok(queue) => Ok(queue),
        Err(parse_err) => {
            // Attempt to repair common JSON errors
            log::warn!("Queue JSON parse error, attempting repair: {}", parse_err);

            if let Some(repaired) = attempt_json_repair(&raw) {
                match crate::jsonc::parse_jsonc::<QueueFile>(
                    &repaired,
                    &format!("repaired queue {}", path.display()),
                ) {
                    Ok(queue) => {
                        log::info!("Successfully repaired queue JSON");
                        Ok(queue)
                    }
                    Err(repair_err) => {
                        // Repair failed, return original error with context
                        Err(parse_err).with_context(|| {
                            format!(
                                "parse queue {} as JSON/JSONC (repair also failed: {})",
                                path.display(),
                                repair_err
                            )
                        })?
                    }
                }
            } else {
                // No repair possible, return original error
                Err(parse_err)
            }
        }
    }
}

/// Load queue with repair and semantic validation.
///
/// JSON repair is followed by semantic validation via `validate_queue_set`. Callers
/// should log warnings if needed. This ensures repaired-but-invalid queues fail
/// early with descriptive errors.
///
/// Returns the queue file and any validation warnings (non-blocking issues).
pub fn load_queue_with_repair_and_validate(
    path: &Path,
    done: Option<&crate::contracts::QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<(QueueFile, Vec<ValidationWarning>)> {
    let queue = load_queue_with_repair(path)?;

    let warnings = if let Some(d) = done {
        validate_queue_set(&queue, Some(d), id_prefix, id_width, max_dependency_depth)
            .with_context(|| format!("validate repaired queue {}", path.display()))?
    } else {
        validate_queue(&queue, id_prefix, id_width)
            .with_context(|| format!("validate repaired queue {}", path.display()))?;
        Vec::new()
    };

    Ok((queue, warnings))
}

/// Attempt to repair common JSON errors induced by agents.
/// Returns Some(repaired_json) if repairs were made, None if no repairs possible.
pub fn attempt_json_repair(raw: &str) -> Option<String> {
    let mut repaired = raw.to_string();
    let original = raw.to_string();

    // Repair 1: Convert single-quoted strings to double-quoted
    // Pattern: 'value' (but not apostrophes within words like "don't")
    // We match single quotes that appear to be string delimiters
    // Match '...' where the content doesn't contain ' and is not preceded/followed by alphanumeric
    // Use ^ or non-alphanumeric before, and non-alphanumeric or $ after
    if SINGLE_QUOTED_STRING_RE.is_match(&repaired) {
        log::debug!("JSON repair: converting single-quoted strings to double-quoted");
        repaired = SINGLE_QUOTED_STRING_RE
            .replace_all(&repaired, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let content = &caps[2];
                let suffix = &caps[3];
                let escaped = content.replace('"', "\\\"");
                format!("{}\"{}\"{}", prefix, escaped, suffix)
            })
            .to_string();
    }

    // Repair 2: Add missing quotes around unquoted object keys
    // Pattern: {[ or , followed by whitespace, then identifier followed by colon
    // Matches: {key: or ,key: or { key: or , key:
    if UNQUOTED_KEY_RE.is_match(&repaired) {
        log::debug!("JSON repair: adding quotes around unquoted object keys");
        repaired = UNQUOTED_KEY_RE
            .replace_all(&repaired, "$1\"$2\":")
            .to_string();
    }

    // Repair 3: Fix unescaped newlines within string values
    // This is a common error when agents paste multi-line content
    // We need to find newlines that are inside string contexts and escape them
    repaired = repair_unescaped_newlines(&repaired);

    // Repair 4: Fix unescaped quotes within string values
    // Find quotes inside strings that aren't escaped and escape them
    repaired = repair_unescaped_quotes(&repaired);

    // Repair 5: Remove trailing commas before ] or }
    // Pattern: ,\s*] or ,\s*}
    if TRAILING_COMMA_RE.is_match(&repaired) {
        log::debug!("JSON repair: removing trailing commas");
        repaired = TRAILING_COMMA_RE.replace_all(&repaired, "$1").to_string();
    }

    // Repair 6: Remove trailing commas at end of arrays/objects (more aggressive)
    // This handles cases where there might be newlines between comma and bracket
    // Pattern: ,(\s*)\n(\s*[}\]])
    if TRAILING_COMMA_NEWLINE_RE.is_match(&repaired) {
        log::debug!("JSON repair: removing trailing commas before newlines");
        repaired = TRAILING_COMMA_NEWLINE_RE
            .replace_all(&repaired, "$1\n$2")
            .to_string();
    }

    // Repair 7: Fix missing closing bracket at end of file
    let open_brackets = repaired.matches('[').count();
    let close_brackets = repaired.matches(']').count();
    let open_braces = repaired.matches('{').count();
    let close_braces = repaired.matches('}').count();

    if open_brackets > close_brackets {
        log::debug!(
            "JSON repair: adding {} missing closing bracket(s)",
            open_brackets - close_brackets
        );
        repaired.push_str(&"]".repeat(open_brackets - close_brackets));
    }
    if open_braces > close_braces {
        log::debug!(
            "JSON repair: adding {} missing closing brace(s)",
            open_braces - close_braces
        );
        repaired.push_str(&"}".repeat(open_braces - close_braces));
    }

    if repaired != original {
        Some(repaired)
    } else {
        None
    }
}

/// Fix unescaped newlines within JSON string values.
/// Uses a simple state machine to track whether we're inside a string.
fn repair_unescaped_newlines(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            // Previous char was backslash, this char is escaped
            result.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => {
                escaped = true;
                result.push(ch);
            }
            '"' => {
                in_string = !in_string;
                result.push(ch);
            }
            '\n' if in_string => {
                // Newline inside string - escape it
                log::trace!("JSON repair: escaping unescaped newline in string");
                result.push_str("\\n");
            }
            '\r' if in_string => {
                // Carriage return inside string - escape it
                log::trace!("JSON repair: escaping unescaped carriage return in string");
                result.push_str("\\r");
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result
}

/// Placeholder for future unescaped quote repair within JSON string values.
///
/// Currently tracks string state but does not modify quotes. Properly escaping
/// internal quotes requires look-ahead heuristics to distinguish between:
/// - Quotes that close a string (followed by structural chars like `:`, `,`, `}`, `]`)
/// - Quotes that are content and need escaping (followed by other chars)
///
/// This is a complex repair that risks over-escaping. For now, this function
/// passes through unchanged to avoid making valid JSON invalid.
fn repair_unescaped_quotes(raw: &str) -> String {
    // Future implementation: use look-ahead to determine if a quote inside
    // a string should be escaped or is closing the string.
    raw.to_string()
}

/// Create a backup of the queue file before modification.
/// Returns the path to the backup file.
pub fn backup_queue(path: &Path, backup_dir: &Path) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(backup_dir)?;
    let timestamp = crate::timeutil::now_utc_rfc3339_or_fallback().replace([':', '.'], "-");
    let backup_name = format!("{QUEUE_BACKUP_PREFIX}{timestamp}");
    let backup_path = backup_dir.join(backup_name);

    std::fs::copy(path, &backup_path)
        .with_context(|| format!("backup queue to {}", backup_path.display()))?;

    match cleanup_queue_backups(backup_dir, MAX_QUEUE_BACKUP_FILES) {
        Ok(removed) if removed > 0 => {
            log::debug!(
                "pruned {} stale queue backup(s); retaining latest {}",
                removed,
                MAX_QUEUE_BACKUP_FILES
            );
        }
        Ok(_) => {}
        Err(err) => {
            log::warn!(
                "failed to prune queue backups in {}: {:#}",
                backup_dir.display(),
                err
            );
        }
    }

    Ok(backup_path)
}

fn cleanup_queue_backups(backup_dir: &Path, max_backups: usize) -> Result<usize> {
    if max_backups == 0 || !backup_dir.exists() {
        return Ok(0);
    }

    let mut backup_paths: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(backup_dir)
        .with_context(|| format!("read backup directory {}", backup_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("read backup directory entry in {}", backup_dir.display()))?;

        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with(QUEUE_BACKUP_PREFIX) {
            backup_paths.push(entry.path());
        }
    }

    if backup_paths.len() <= max_backups {
        return Ok(0);
    }

    backup_paths.sort_unstable_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });

    let mut removed = 0usize;
    let to_remove = backup_paths.len().saturating_sub(max_backups);
    for backup_path in backup_paths.into_iter().take(to_remove) {
        std::fs::remove_file(&backup_path)
            .with_context(|| format!("remove queue backup {}", backup_path.display()))?;
        removed += 1;
    }

    Ok(removed)
}

/// Load the active queue and optionally the done queue, validating both.
pub fn load_and_validate_queues(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let queue_file = load_queue(&resolved.queue_path)?;

    let done_file = if include_done {
        Some(load_queue_or_default(&resolved.done_path)?)
    } else {
        None
    };

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    if let Some(d) = done_ref {
        let warnings = validate_queue_set(
            &queue_file,
            Some(d),
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )?;
        log_warnings(&warnings);
    } else {
        validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
    }

    Ok((queue_file, done_file))
}

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(())
}

pub fn next_id_across(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<String> {
    let warnings = validate_queue_set(active, done, id_prefix, id_width, max_dependency_depth)?;
    log_warnings(&warnings);
    let expected_prefix = normalize_prefix(id_prefix);

    let mut max_value: u32 = 0;
    for (idx, task) in active.tasks.iter().enumerate() {
        let value = validation::validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
        if task.status == TaskStatus::Rejected {
            continue;
        }
        if value > max_value {
            max_value = value;
        }
    }
    if let Some(done) = done {
        for (idx, task) in done.tasks.iter().enumerate() {
            let value = validation::validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
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

pub(crate) fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

pub(crate) fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

// Pruning implementation moved to `queue::prune` (re-exported from this module).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    // Pruning test helpers moved to `queue/prune.rs`.

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
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
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    fn task_with_timestamps(
        id: &str,
        status: TaskStatus,
        tags: Vec<String>,
        created_at: Option<&str>,
        updated_at: Option<&str>,
    ) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: created_at.map(|s| s.to_string()),
            updated_at: updated_at.map(|s| s.to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn next_id_across_includes_done() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0002")],
        };
        let mut done_task = task_with("RQ-0009", TaskStatus::Done, vec!["tag".to_string()]);
        done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let done = QueueFile {
            version: 1,
            tasks: vec![done_task],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4, 10)?;
        assert_eq!(next, "RQ-0010");
        Ok(())
    }

    #[test]
    fn load_and_validate_queues_allows_missing_done_file() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;
        let done_path = ralph_dir.join("done.json");

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let (queue, done) = load_and_validate_queues(&resolved, true)?;
        assert_eq!(queue.tasks.len(), 1);
        assert!(done.is_some());
        assert!(done.unwrap().tasks.is_empty());
        Ok(())
    }

    #[test]
    fn load_and_validate_queues_rejects_duplicate_ids_across_done() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;
        let done_path = ralph_dir.join("done.json");
        let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
        done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        save_queue(
            &done_path,
            &QueueFile {
                version: 1,
                tasks: vec![done_task],
            },
        )?;

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err =
            load_and_validate_queues(&resolved, true).expect_err("expected duplicate id error");
        assert!(
            err.to_string()
                .contains("Duplicate task ID detected across queue and done")
        );
        Ok(())
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
    fn next_id_across_ignores_rejected() -> Result<()> {
        let mut t_rejected = task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]);
        t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
                t_rejected,
            ],
        };
        let next = next_id_across(&active, None, "RQ", 4, 10)?;
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
        let mut t_done = task_with("RQ-0005", TaskStatus::Done, vec!["tag".to_string()]);
        t_done.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let mut t_rejected = task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]);
        t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let done = QueueFile {
            version: 1,
            tasks: vec![t_done, t_rejected],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4, 10)?;
        assert_eq!(next, "RQ-0006");
        Ok(())
    }

    // Pruning tests moved to `queue/prune.rs`.

    #[test]
    fn attempt_json_repair_fixes_trailing_comma_in_array() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "tags": ["a", "b",]}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"tags\": [\"a\", \"b\"]"));
        assert!(!repaired.contains("\"b\","));
    }

    #[test]
    fn attempt_json_repair_fixes_trailing_comma_in_object() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Test",}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"title\": \"Test\"}"));
        assert!(!repaired.contains("\"Test\","));
    }

    #[test]
    fn attempt_json_repair_returns_none_for_valid_json() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
        assert!(attempt_json_repair(input).is_none());
    }

    #[test]
    fn attempt_json_repair_fixes_multiple_trailing_commas() {
        // Test with a complete valid task structure that includes all required fields
        let input = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test", "status": "todo", "tags": ["a", "b",], "scope": ["file",],}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
    }

    #[test]
    fn load_queue_with_repair_fixes_malformed_json() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with trailing comma
        let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test", "status": "todo", "tags": ["bug",],}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should load with repair
        let queue = load_queue_with_repair(&queue_path)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].tags, vec!["bug"]);

        Ok(())
    }

    // Tests for enhanced JSON repair (RQ-0362)

    #[test]
    fn attempt_json_repair_fixes_single_quoted_strings() {
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        // Check specific conversions
        assert!(repaired.contains("\"version\""));
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"id\""));
        assert!(repaired.contains("\"RQ-0001\""));
        assert!(repaired.contains("\"title\""));
        assert!(repaired.contains("\"Test\""));
    }

    #[test]
    fn attempt_json_repair_preserves_apostrophes_in_words() {
        // Apostrophes within words (like "don't") should not be converted
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Don't break this"}]}"#;
        // This is valid JSON, so no repair needed
        assert!(attempt_json_repair(input).is_none());
    }

    #[test]
    fn attempt_json_repair_fixes_unquoted_object_keys() {
        let input = r#"{version: 1, tasks: [{id: "RQ-0001", title: "Test"}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        // Check keys are quoted
        assert!(repaired.contains("\"version\""));
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"id\""));
        assert!(repaired.contains("\"title\""));
    }

    #[test]
    fn attempt_json_repair_fixes_unquoted_keys_after_comma() {
        let input =
            r#"{"version": 1, tasks: [{"id": "RQ-0001", "title": "Test", status: "todo"}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"status\""));
    }

    #[test]
    fn attempt_json_repair_fixes_unescaped_newlines_in_strings() {
        // Agent pastes multi-line content without escaping
        let input = "{\"version\": 1, \"tasks\": [{\"id\": \"RQ-0001\", \"title\": \"Line one\nLine two\"}]}";
        let repaired = attempt_json_repair(input).expect("should repair");
        // Newlines should be escaped
        assert!(repaired.contains("Line one\\nLine two"));
        assert!(!repaired.contains("Line one\nLine two"));
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
    }

    #[test]
    fn attempt_json_repair_fixes_unescaped_carriage_returns_in_strings() {
        let input = "{\"version\": 1, \"tasks\": [{\"id\": \"RQ-0001\", \"title\": \"Line one\rLine two\"}]}";
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("Line one\\rLine two"));
        assert!(!repaired.contains("Line one\rLine two"));
    }

    #[test]
    fn attempt_json_repair_handles_multiple_errors() {
        // Combine multiple errors: single quotes, unquoted keys, trailing comma
        let input = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test', 'status': 'todo', 'tags': [], 'scope': [], 'evidence': [], 'plan': [],}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert!(repaired.contains("\"version\""));
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"id\""));
        assert!(repaired.contains("\"RQ-0001\""));
    }

    #[test]
    fn load_queue_with_repair_fixes_complex_malformed_json() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with multiple issues
        let malformed = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',],}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should load with repair
        let queue = load_queue_with_repair(&queue_path)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].title, "Test task");
        assert_eq!(queue.tasks[0].tags, vec!["bug"]);

        Ok(())
    }

    #[test]
    fn attempt_json_repair_escapes_double_quotes_in_single_quoted_strings() {
        // Single-quoted string containing double quotes should escape them
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Say "hello"'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"Say \\\"hello\\\"\""));
    }

    #[test]
    fn attempt_json_repair_handles_empty_single_quoted_string() {
        let input = r#"{'version': 1, 'tasks': [{'id': '', 'title': ''}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert!(repaired.contains("\"id\": \"\""));
    }

    #[test]
    fn backup_queue_creates_backup_file() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let backup_dir = temp.path().join("backups");

        // Create initial queue
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;

        // Create backup
        let backup_path = backup_queue(&queue_path, &backup_dir)?;

        // Verify backup exists and contains valid JSON
        assert!(backup_path.exists());
        let backup_queue: QueueFile =
            serde_json::from_str(&std::fs::read_to_string(&backup_path)?)?;
        assert_eq!(backup_queue.tasks.len(), 1);
        assert_eq!(backup_queue.tasks[0].id, "RQ-0001");

        Ok(())
    }

    #[test]
    fn cleanup_queue_backups_removes_oldest_files() -> Result<()> {
        let temp = TempDir::new()?;
        let backup_dir = temp.path().join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        for suffix in ["0001", "0002", "0003"] {
            let backup_path = backup_dir.join(format!("{QUEUE_BACKUP_PREFIX}{suffix}"));
            std::fs::write(backup_path, "{}")?;
        }

        let removed = cleanup_queue_backups(&backup_dir, 2)?;
        assert_eq!(removed, 1);
        assert!(
            !backup_dir
                .join(format!("{QUEUE_BACKUP_PREFIX}0001"))
                .exists()
        );
        assert!(
            backup_dir
                .join(format!("{QUEUE_BACKUP_PREFIX}0002"))
                .exists()
        );
        assert!(
            backup_dir
                .join(format!("{QUEUE_BACKUP_PREFIX}0003"))
                .exists()
        );

        Ok(())
    }

    #[test]
    fn backup_queue_prunes_backups_to_retention_limit() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let backup_dir = temp.path().join("backups");
        std::fs::create_dir_all(&backup_dir)?;

        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;

        for idx in 0..(MAX_QUEUE_BACKUP_FILES + 2) {
            let backup_path = backup_dir.join(format!("{QUEUE_BACKUP_PREFIX}0000-{idx:04}"));
            std::fs::write(backup_path, "{}")?;
        }

        let _backup_path = backup_queue(&queue_path, &backup_dir)?;

        let backup_count = std::fs::read_dir(&backup_dir)?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(QUEUE_BACKUP_PREFIX))
            .count();

        assert_eq!(backup_count, MAX_QUEUE_BACKUP_FILES);

        Ok(())
    }

    #[test]
    fn load_queue_accepts_scalar_custom_fields_and_save_normalizes_to_strings() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write queue with numeric and boolean custom_fields values
        std::fs::write(
            &queue_path,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","title":"t","created_at":"2026-01-18T00:00:00Z","updated_at":"2026-01-18T00:00:00Z","custom_fields":{"n":1411,"b":false}}]}"#,
        )?;

        // Load queue - should accept numeric/boolean values and coerce to strings
        let queue = crate::queue::load_queue(&queue_path)?;
        assert_eq!(
            queue.tasks[0].custom_fields.get("n").map(String::as_str),
            Some("1411")
        );
        assert_eq!(
            queue.tasks[0].custom_fields.get("b").map(String::as_str),
            Some("false")
        );

        // Save queue - should serialize as strings
        crate::queue::save_queue(&queue_path, &queue)?;
        let rendered = std::fs::read_to_string(&queue_path)?;
        assert!(rendered.contains("\"n\": \"1411\""));
        assert!(rendered.contains("\"b\": \"false\""));

        Ok(())
    }

    // Tests for load_queue_with_repair_and_validate (RQ-0502)

    #[test]
    fn load_queue_with_repair_and_validate_rejects_missing_timestamps() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with trailing comma but missing required timestamps
        let malformed = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': [], 'plan': [],}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should fail validation due to missing created_at/updated_at
        let result = load_queue_with_repair_and_validate(&queue_path, None, "RQ", 4, 10);

        let err = result.expect_err("should fail validation due to missing timestamps");
        // Traverse the error chain to find the root cause
        let err_msg = err
            .chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            err_msg.contains("created_at") || err_msg.contains("updated_at"),
            "Error should mention missing timestamp: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn load_queue_with_repair_and_validate_accepts_valid_repair() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with trailing commas but all required fields present
        let malformed = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': ['observed',], 'plan': ['do thing',], 'created_at': '2026-01-18T00:00:00Z', 'updated_at': '2026-01-18T00:00:00Z',}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should load with repair and pass validation
        let (queue, warnings) =
            load_queue_with_repair_and_validate(&queue_path, None, "RQ", 4, 10)?;

        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].title, "Test task");
        assert_eq!(queue.tasks[0].tags, vec!["bug"]);
        assert!(warnings.is_empty());

        Ok(())
    }

    #[test]
    fn load_queue_with_repair_and_validate_detects_done_queue_issues() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let done_path = temp.path().join("done.json");

        // Active queue: valid but with dependency on done task
        let active_malformed = r#"{'version': 1, tasks: [{'id': 'RQ-0002', 'title': 'Second task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': [], 'plan': [], 'created_at': '2026-01-18T00:00:00Z', 'updated_at': '2026-01-18T00:00:00Z', 'depends_on': ['RQ-0001',],}]}"#;
        std::fs::write(&queue_path, active_malformed)?;

        // Done queue: contains the dependency target
        let done_queue = QueueFile {
            version: 1,
            tasks: vec![{
                let mut t = task_with_timestamps(
                    "RQ-0001",
                    TaskStatus::Done,
                    vec!["done".to_string()],
                    Some("2026-01-18T00:00:00Z"),
                    Some("2026-01-18T00:00:00Z"),
                );
                t.completed_at = Some("2026-01-18T00:00:00Z".to_string());
                t
            }],
        };
        save_queue(&done_path, &done_queue)?;

        // Should load with repair and validate successfully
        let (queue, warnings) =
            load_queue_with_repair_and_validate(&queue_path, Some(&done_queue), "RQ", 4, 10)?;

        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0002");
        assert!(warnings.is_empty());

        Ok(())
    }

    // Tests for cached regex performance (RQ-0810)

    #[test]
    fn attempt_json_repair_preserves_single_quote_then_unquoted_key_order() {
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test', 'status': 'todo', 'tags': [], 'scope': [], 'evidence': [], 'plan': [], 'created_at': '2026-01-01T00:00:00Z', 'updated_at': '2026-01-01T00:00:00Z'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains(r#""tasks""#));
        assert!(repaired.contains(r#""id": "RQ-0001""#));
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should parse as JSON");
    }

    #[test]
    fn attempt_json_repair_handles_multiple_ordered_errors() {
        let input = r#"{'version': 1, tasks: [{id: 'RQ-0001', title: 'A', status: 'todo', tags: ['bug',], scope: [], evidence: [], plan: [], created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _parsed: QueueFile =
            serde_json::from_str(&repaired).expect("repaired should parse as JSON");
        assert!(repaired.contains(r#""version""#));
        assert!(repaired.contains(r#""tasks""#));
        assert!(repaired.contains(r#""title""#));
        assert!(repaired.contains(r#""tags": ["bug"]"#));
    }

    #[test]
    #[ignore = "perf-smoke: run manually when tuning hot-path: cargo test -p ralph queue::tests::attempt_json_repair_perf_smoke -- --ignored"]
    fn attempt_json_repair_perf_smoke() {
        let input = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'A', 'status': 'todo', 'scope': ['x',], 'evidence': ['a',], 'plan': ['x',], 'created_at': '2026-01-01T00:00:00Z', 'updated_at': '2026-01-01T00:00:00Z'}]}"#;
        let start = std::time::Instant::now();
        for _ in 0..20_000 {
            let _ = attempt_json_repair(input);
        }
        let _elapsed = start.elapsed();
    }
}
