//! Queue import subcommand for importing tasks from CSV, TSV, or JSON.
//!
//! Responsibilities:
//! - Parse input formats (CSV, TSV, JSON) into task structures.
//! - Normalize and backfill imported tasks (timestamps, IDs, list fields).
//! - Apply duplicate handling policy (fail, skip, rename).
//! - Merge imported tasks into the active queue with proper positioning.
//!
//! Not handled here:
//! - Export functionality (see `crate::cli::queue::export`).
//! - GUI-specific import workflows (this is a CLI command).
//! - Complex schema migration between versions.
//!
//! Invariants/assumptions:
//! - Always acquire queue lock before modifying queue files.
//! - Never write to disk on parse/validation failures.
//! - Undo snapshots are only created AFTER all validation succeeds (no orphaned snapshots on error).
//! - Always backfill required timestamps (created_at, updated_at, completed_at for terminal statuses).
//! - List fields are trimmed and empty items are dropped.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;

use super::QueueImportFormat;

/// Arguments for `ralph queue import`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue export --format json | ralph queue import --format json --dry-run\n  ralph queue import --format csv --input tasks.csv\n  ralph queue import --format tsv --input - --on-duplicate rename < tasks.tsv\n  ralph queue import --format json --input tasks.json --on-duplicate skip"
)]
pub struct QueueImportArgs {
    /// Input format.
    #[arg(long, value_enum)]
    pub format: QueueImportFormat,

    /// Input file path (default: stdin). Use '-' for stdin.
    #[arg(long, short)]
    pub input: Option<PathBuf>,

    /// Show what would change without writing to disk.
    #[arg(long)]
    pub dry_run: bool,

    /// What to do if an imported task ID already exists.
    #[arg(long, value_enum, default_value_t = OnDuplicate::Fail)]
    pub on_duplicate: OnDuplicate,
}

/// Policy for handling duplicate task IDs during import.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum OnDuplicate {
    /// Fail with an error if a duplicate ID is found.
    Fail,
    /// Skip duplicate tasks and continue importing others.
    Skip,
    /// Generate a new ID for duplicate tasks.
    Rename,
}

/// Summary of an import operation for logging.
struct ImportReport {
    parsed: usize,
    imported: usize,
    skipped_duplicates: usize,
    renamed: usize,
    rename_mappings: Vec<(String, String)>,
}

impl ImportReport {
    fn summary(&self) -> String {
        let mut parts = vec![format!("parsed {} task(s)", self.parsed)];
        if self.imported > 0 {
            parts.push(format!("imported {}", self.imported));
        }
        if self.skipped_duplicates > 0 {
            parts.push(format!("skipped {} duplicate(s)", self.skipped_duplicates));
        }
        if self.renamed > 0 {
            parts.push(format!("renamed {} task(s)", self.renamed));
            // Show up to 50 rename mappings
            let show_count = self.rename_mappings.len().min(50);
            for (old, new) in &self.rename_mappings[..show_count] {
                parts.push(format!("  {} -> {}", old, new));
            }
            if self.rename_mappings.len() > 50 {
                parts.push(format!(
                    "  ... and {} more",
                    self.rename_mappings.len() - 50
                ));
            }
        }
        parts.join("; ")
    }
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueImportArgs) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "queue import", force)?;

    let input = read_input(args.input.as_ref()).context("read import input")?;

    // Parse the input based on format
    let mut imported = match args.format {
        QueueImportFormat::Json => parse_json_tasks(&input)?,
        QueueImportFormat::Csv => parse_csv_tasks(&input, b',')?,
        QueueImportFormat::Tsv => parse_csv_tasks(&input, b'\t')?,
    };

    let now = crate::timeutil::now_utc_rfc3339_or_fallback();

    // Load existing queue + done for uniqueness checks
    let (mut queue_file, done_file) = crate::queue::load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Normalize and backfill imported tasks
    for task in &mut imported {
        normalize_task(task, &now);
    }

    // Merge imported tasks
    let report = merge_imported_tasks(
        &mut queue_file,
        done_ref,
        imported,
        &resolved.id_prefix,
        resolved.id_width,
        resolved.config.queue.max_dependency_depth.unwrap_or(10),
        &now,
        args.on_duplicate,
    )?;

    // Validate (including for dry-run). Dry-run should fail if the resulting queue would be invalid.
    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        resolved.config.queue.max_dependency_depth.unwrap_or(10),
    )?;
    queue::log_warnings(&warnings);

    // Create undo snapshot before mutation (only if not dry-run and validation passed).
    // This must happen AFTER all fallible operations (parsing, merging, validation) to avoid
    // leaving orphaned snapshots when the import operation itself fails.
    if !args.dry_run {
        crate::undo::create_undo_snapshot(resolved, "queue import")?;
    }

    if args.dry_run {
        log::info!("Dry run: no changes written. {}", report.summary());
        return Ok(());
    }

    queue::save_queue(&resolved.queue_path, &queue_file)?;
    log::info!("Imported tasks. {}", report.summary());

    Ok(())
}

/// Read input from file or stdin.
fn read_input(path: Option<&PathBuf>) -> Result<String> {
    let use_stdin = path.is_none() || path.is_some_and(|p| p.as_os_str() == "-");

    if use_stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("read from stdin")?;
        Ok(buffer)
    } else {
        let path = path.unwrap();
        std::fs::read_to_string(path)
            .with_context(|| format!("read import file {}", path.display()))
    }
}

/// Parse JSON tasks from input.
/// Accepts either a JSON array of tasks or a wrapper object { "version": 1, "tasks": [...] }.
fn parse_json_tasks(input: &str) -> Result<Vec<Task>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    // First try parsing as Vec<Task>
    match serde_json::from_str::<Vec<Task>>(trimmed) {
        Ok(tasks) => Ok(tasks),
        Err(arr_err) => {
            // Try parsing as wrapper object
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct TasksWrapper {
                #[serde(default)]
                version: Option<u32>,
                tasks: Vec<Task>,
            }

            match serde_json::from_str::<TasksWrapper>(trimmed) {
                Ok(wrapper) => {
                    if let Some(ver) = wrapper.version
                        && ver != 1
                    {
                        bail!(
                            "Unsupported wrapper version: {}. Only version 1 is supported.",
                            ver
                        );
                    }
                    Ok(wrapper.tasks)
                }
                Err(_) => {
                    // Return the original array parse error for clearer diagnostics
                    bail!(
                        "Invalid JSON format: {}. Expected array of tasks or {{\"version\": 1, \"tasks\": [...]}} wrapper.",
                        arr_err
                    )
                }
            }
        }
    }
}

/// Parse CSV/TSV tasks from input.
fn parse_csv_tasks(input: &str, delimiter: u8) -> Result<Vec<Task>> {
    let mut tasks = Vec::new();

    if input.trim().is_empty() {
        return Ok(tasks);
    }

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(input.as_bytes());

    let headers = reader
        .headers()?
        .iter()
        .map(|h| h.to_lowercase())
        .collect::<Vec<_>>();
    let header_map: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.clone(), i))
        .collect();

    // Check for required 'title' column
    if !header_map.contains_key("title") {
        bail!("CSV/TSV import requires a 'title' column");
    }

    for (row_idx, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("parse CSV row {}", row_idx + 1))?;

        let mut task = Task::default();

        // Required: title
        let title_idx = header_map["title"];
        task.title = record
            .get(title_idx)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if task.title.is_empty() {
            bail!("Row {}: title is required and cannot be empty", row_idx + 1);
        }

        // Optional: id
        if let Some(&idx) = header_map.get("id") {
            task.id = record
                .get(idx)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
        }

        // Optional: status
        if let Some(&idx) = header_map.get("status") {
            let status_str = record.get(idx).unwrap_or("").trim().to_lowercase();
            if !status_str.is_empty() {
                task.status = parse_status(&status_str)?;
            }
        }

        // Optional: priority
        if let Some(&idx) = header_map.get("priority") {
            let raw = record.get(idx).unwrap_or("");
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                task.priority = trimmed.parse()?;
            }
        }

        // Optional: tags (comma-separated)
        if let Some(&idx) = header_map.get("tags") {
            task.tags = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }

        // Optional: scope (comma-separated)
        if let Some(&idx) = header_map.get("scope") {
            task.scope = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }

        // Optional: evidence (semicolon-separated)
        if let Some(&idx) = header_map.get("evidence") {
            task.evidence = parse_list_field(record.get(idx).unwrap_or(""), ';');
        }

        // Optional: plan (semicolon-separated)
        if let Some(&idx) = header_map.get("plan") {
            task.plan = parse_list_field(record.get(idx).unwrap_or(""), ';');
        }

        // Optional: notes (semicolon-separated)
        if let Some(&idx) = header_map.get("notes") {
            task.notes = parse_list_field(record.get(idx).unwrap_or(""), ';');
        }

        // Optional: request
        if let Some(&idx) = header_map.get("request") {
            let req = record.get(idx).unwrap_or("").trim().to_string();
            task.request = if req.is_empty() { None } else { Some(req) };
        }

        // Optional: created_at
        if let Some(&idx) = header_map.get("created_at") {
            let ts = record.get(idx).unwrap_or("").trim().to_string();
            task.created_at = if ts.is_empty() { None } else { Some(ts) };
        }

        // Optional: updated_at
        if let Some(&idx) = header_map.get("updated_at") {
            let ts = record.get(idx).unwrap_or("").trim().to_string();
            task.updated_at = if ts.is_empty() { None } else { Some(ts) };
        }

        // Optional: completed_at
        if let Some(&idx) = header_map.get("completed_at") {
            let ts = record.get(idx).unwrap_or("").trim().to_string();
            task.completed_at = if ts.is_empty() { None } else { Some(ts) };
        }

        // Optional: depends_on (comma-separated)
        if let Some(&idx) = header_map.get("depends_on") {
            task.depends_on = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }

        // Optional: blocks (comma-separated)
        if let Some(&idx) = header_map.get("blocks") {
            task.blocks = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }

        // Optional: relates_to (comma-separated)
        if let Some(&idx) = header_map.get("relates_to") {
            task.relates_to = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }

        // Optional: duplicates
        if let Some(&idx) = header_map.get("duplicates") {
            let dup = record.get(idx).unwrap_or("").trim().to_string();
            task.duplicates = if dup.is_empty() { None } else { Some(dup) };
        }

        // Optional: custom_fields (k=v comma-separated)
        if let Some(&idx) = header_map.get("custom_fields") {
            task.custom_fields = parse_custom_fields(record.get(idx).unwrap_or(""))?;
        }

        // Optional: parent_id
        if let Some(&idx) = header_map.get("parent_id") {
            let pid = record.get(idx).unwrap_or("").trim().to_string();
            task.parent_id = if pid.is_empty() { None } else { Some(pid) };
        }

        tasks.push(task);
    }

    Ok(tasks)
}

/// Parse a list field by splitting on delimiter and trimming/dropping empty items.
fn parse_list_field(value: &str, delimiter: char) -> Vec<String> {
    value
        .split(delimiter)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse custom fields from "k=v,k2=v2" format.
fn parse_custom_fields(value: &str) -> Result<HashMap<String, String>> {
    let mut fields = HashMap::new();
    if value.trim().is_empty() {
        return Ok(fields);
    }

    for pair in value.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let parts: Vec<&str> = pair.splitn(2, '=').collect();
        if parts.len() != 2 {
            bail!(
                "Invalid custom field format: '{}'. Expected 'key=value'.",
                pair
            );
        }

        let key = parts[0].trim();
        let val = parts[1].trim();

        if key.is_empty() {
            bail!("Empty custom field key in '{}'", pair);
        }
        if key.chars().any(|c| c.is_whitespace()) {
            bail!("Custom field key cannot contain whitespace: '{}'", key);
        }

        fields.insert(key.to_string(), val.to_string());
    }

    Ok(fields)
}

/// Parse status case-insensitively.
fn parse_status(s: &str) -> Result<TaskStatus> {
    match s.to_lowercase().as_str() {
        "draft" => Ok(TaskStatus::Draft),
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "done" => Ok(TaskStatus::Done),
        "rejected" => Ok(TaskStatus::Rejected),
        _ => bail!(
            "Invalid status: '{}'. Expected: draft, todo, doing, done, rejected",
            s
        ),
    }
}

/// Normalize a task: trim fields, drop empty list items, backfill timestamps.
fn normalize_task(task: &mut Task, now: &str) {
    // Trim ID and title
    task.id = task.id.trim().to_string();
    task.title = task.title.trim().to_string();

    // Normalize list fields: trim and drop empty
    task.tags = normalize_list(&task.tags);
    task.scope = normalize_list(&task.scope);
    task.evidence = normalize_list(&task.evidence);
    task.plan = normalize_list(&task.plan);
    task.notes = normalize_list(&task.notes);
    task.depends_on = normalize_list(&task.depends_on);
    task.blocks = normalize_list(&task.blocks);
    task.relates_to = normalize_list(&task.relates_to);

    // Normalize custom field keys
    let mut normalized_fields = HashMap::new();
    for (k, v) in &task.custom_fields {
        let key = k.trim();
        if !key.is_empty() {
            normalized_fields.insert(key.to_string(), v.trim().to_string());
        }
    }
    task.custom_fields = normalized_fields;

    // Backfill timestamps
    if task.created_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
        task.created_at = Some(now.to_string());
    }
    if task.updated_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
        task.updated_at = Some(now.to_string());
    }
    if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected)
        && task
            .completed_at
            .as_ref()
            .is_none_or(|t| t.trim().is_empty())
    {
        task.completed_at = Some(now.to_string());
    }
}

/// Normalize a list: trim items and drop empty strings.
fn normalize_list(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Merge imported tasks into the queue with duplicate handling.
#[allow(clippy::too_many_arguments)]
fn merge_imported_tasks(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    imported: Vec<Task>,
    id_prefix: &str,
    id_width: usize,
    max_depth: u8,
    now: &str,
    on_duplicate: OnDuplicate,
) -> Result<ImportReport> {
    // Build set of existing IDs
    let mut existing_ids: HashSet<String> = queue.tasks.iter().map(|t| t.id.clone()).collect();
    if let Some(d) = done {
        existing_ids.extend(d.tasks.iter().map(|t| t.id.clone()));
    }

    let mut report = ImportReport {
        parsed: imported.len(),
        imported: 0,
        skipped_duplicates: 0,
        renamed: 0,
        rename_mappings: Vec::new(),
    };

    let mut tasks_to_add: Vec<Task> = Vec::new();
    struct NeedsId {
        idx: usize, // index into tasks_to_add
        old_id: Option<String>,
    }
    let mut needs_new_id: Vec<NeedsId> = Vec::new();

    // First pass: handle duplicates and collect tasks
    for mut task in imported {
        // Skip empty/whitespace IDs for duplicate check - they'll get new IDs
        let has_id = !task.id.is_empty();

        if has_id {
            let is_duplicate = existing_ids.contains(&task.id)
                || tasks_to_add.iter().any(|t: &Task| t.id == task.id);

            if is_duplicate {
                match on_duplicate {
                    OnDuplicate::Fail => {
                        bail!(
                            "Duplicate task ID detected: '{}'. Use --on-duplicate skip or rename to handle duplicates.",
                            task.id
                        );
                    }
                    OnDuplicate::Skip => {
                        report.skipped_duplicates += 1;
                        continue;
                    }
                    OnDuplicate::Rename => {
                        let old_id = task.id.clone();
                        task.id.clear(); // Will generate new ID
                        needs_new_id.push(NeedsId {
                            idx: tasks_to_add.len(),
                            old_id: Some(old_id),
                        });
                        tasks_to_add.push(task);
                        continue;
                    }
                }
            }
        } else {
            // No ID provided, needs new ID
            needs_new_id.push(NeedsId {
                idx: tasks_to_add.len(),
                old_id: None,
            });
        }

        tasks_to_add.push(task);
    }

    // Generate new IDs for tasks that need them
    if !needs_new_id.is_empty() {
        // Create a temporary queue for ID generation
        let mut temp_queue = queue.clone();

        for need in &needs_new_id {
            // Reserve a new ID by adding a placeholder task
            let new_id = queue::next_id_across(&temp_queue, done, id_prefix, id_width, max_depth)?;

            // Update the task with the new ID
            if need.idx < tasks_to_add.len() {
                let task = &mut tasks_to_add[need.idx];
                if let Some(old_id) = need.old_id.as_ref() {
                    report
                        .rename_mappings
                        .push((old_id.clone(), new_id.clone()));
                }
                task.id = new_id.clone();
            }

            // Add placeholder to temp_queue for next ID calculation
            temp_queue.tasks.push(create_placeholder_task(new_id, now));
        }
    }
    report.renamed = report.rename_mappings.len();

    // Collect IDs of tasks being added
    let new_task_ids: Vec<String> = tasks_to_add.iter().map(|t| t.id.clone()).collect();

    // Add tasks to queue
    queue.tasks.extend(tasks_to_add);
    report.imported = new_task_ids.len();

    // Reposition new tasks
    if !new_task_ids.is_empty() {
        let insert_at = crate::queue::operations::suggest_new_task_insert_index(queue);
        crate::queue::operations::reposition_new_tasks(queue, &new_task_ids, insert_at);
    }

    Ok(report)
}

/// Create a minimal placeholder task for ID reservation.
fn create_placeholder_task(id: String, now: &str) -> Task {
    Task {
        id,
        title: "__import_id_reservation__".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        created_at: Some(now.to_string()),
        updated_at: Some(now.to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_array_succeeds() {
        let json = r#"[{"id": "RQ-0001", "title": "Test task", "status": "todo"}]"#;
        let tasks = parse_json_tasks(json).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "RQ-0001");
        assert_eq!(tasks[0].title, "Test task");
    }

    #[test]
    fn parse_json_wrapper_succeeds() {
        let json = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
        let tasks = parse_json_tasks(json).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "RQ-0001");
    }

    #[test]
    fn parse_json_wrapper_wrong_version_fails() {
        let json = r#"{"version": 2, "tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
        let result = parse_json_tasks(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn parse_json_empty_input_returns_empty() {
        let tasks = parse_json_tasks("").unwrap();
        assert!(tasks.is_empty());
        let tasks = parse_json_tasks("   ").unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn parse_csv_basic_succeeds() {
        let csv = "id,title,status\nRQ-0001,Test task,todo\nRQ-0002,Another task,done";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "RQ-0001");
        assert_eq!(tasks[0].title, "Test task");
        assert_eq!(tasks[0].status, TaskStatus::Todo);
        assert_eq!(tasks[1].status, TaskStatus::Done);
    }

    #[test]
    fn parse_csv_missing_title_fails() {
        let csv = "id,status\nRQ-0001,todo";
        let result = parse_csv_tasks(csv, b',');
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("title"));
    }

    #[test]
    fn parse_csv_empty_title_fails() {
        let csv = "id,title\nRQ-0001,";
        let result = parse_csv_tasks(csv, b',');
        assert!(result.is_err());
    }

    #[test]
    fn parse_csv_list_fields_parsed() {
        let csv = "title,tags,scope,evidence,plan,notes\nTest,a,b,c,d,e";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert_eq!(tasks[0].tags, vec!["a"]);
        assert_eq!(tasks[0].scope, vec!["b"]);
        assert_eq!(tasks[0].evidence, vec!["c"]);
        assert_eq!(tasks[0].plan, vec!["d"]);
        assert_eq!(tasks[0].notes, vec!["e"]);
    }

    #[test]
    fn parse_csv_list_fields_drop_empty() {
        // Use semicolon delimiter for tags to test empty value handling without CSV quoting issues
        let csv = "title,evidence\nTest,a;;b;";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert_eq!(tasks[0].evidence, vec!["a", "b"]);
    }

    #[test]
    fn parse_csv_semicolon_fields_parsed() {
        let csv = "title,evidence,plan,notes\nTest,a;b,c;d,e;f;";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert_eq!(tasks[0].evidence, vec!["a", "b"]);
        assert_eq!(tasks[0].plan, vec!["c", "d"]);
        assert_eq!(tasks[0].notes, vec!["e", "f"]);
    }

    #[test]
    fn parse_csv_custom_fields_parsed() {
        // Quoted custom_fields value to handle comma within field
        let csv = "title,custom_fields\nTest,\"a=1,b=two\"";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert_eq!(tasks[0].custom_fields.get("a"), Some(&"1".to_string()));
        assert_eq!(tasks[0].custom_fields.get("b"), Some(&"two".to_string()));
    }

    #[test]
    fn parse_csv_custom_fields_invalid_fails() {
        let csv = "title,custom_fields\nTest,invalid_no_equals";
        let result = parse_csv_tasks(csv, b',');
        assert!(result.is_err());
    }

    #[test]
    fn parse_csv_empty_custom_fields_ok() {
        let csv = "title,custom_fields\nTest,";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert!(tasks[0].custom_fields.is_empty());
    }

    #[test]
    fn parse_csv_unknown_columns_ignored() {
        let csv = "id,title,unknown_col\nRQ-0001,Test,foo";
        let tasks = parse_csv_tasks(csv, b',').unwrap();
        assert_eq!(tasks[0].id, "RQ-0001");
        assert_eq!(tasks[0].title, "Test");
    }

    #[test]
    fn parse_tsv_succeeds() {
        let tsv = "id\ttitle\tstatus\nRQ-0001\tTest\ttodo";
        let tasks = parse_csv_tasks(tsv, b'\t').unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "RQ-0001");
    }

    #[test]
    fn parse_list_field_handles_delimiters() {
        let result = parse_list_field("a, b, , c", ',');
        assert_eq!(result, vec!["a", "b", "c"]);

        let result = parse_list_field("x; y; ; z", ';');
        assert_eq!(result, vec!["x", "y", "z"]);
    }

    #[test]
    fn parse_status_case_insensitive() {
        assert_eq!(parse_status("TODO").unwrap(), TaskStatus::Todo);
        assert_eq!(parse_status("Todo").unwrap(), TaskStatus::Todo);
        assert_eq!(parse_status("todo").unwrap(), TaskStatus::Todo);
        assert_eq!(parse_status("DONE").unwrap(), TaskStatus::Done);
        assert_eq!(parse_status("Rejected").unwrap(), TaskStatus::Rejected);
    }

    #[test]
    fn parse_csv_invalid_priority_uses_canonical_parser_error() {
        let csv = "title,priority\nTest,nope";
        let err = parse_csv_tasks(csv, b',').unwrap_err();

        let expected = "nope".parse::<TaskPriority>().unwrap_err().to_string();
        let msg = err.to_string();

        assert!(msg.contains(&expected), "err was: {msg}");
    }

    #[test]
    fn normalize_task_trims_fields() {
        let mut task = Task {
            id: "  RQ-0001  ".to_string(),
            title: "  Test  ".to_string(),
            description: None,
            tags: vec!["  a  ".to_string(), "".to_string(), "  b  ".to_string()],
            ..Default::default()
        };
        normalize_task(&mut task, "2026-01-01T00:00:00.000000000Z");
        assert_eq!(task.id, "RQ-0001");
        assert_eq!(task.title, "Test");
        assert_eq!(task.tags, vec!["a", "b"]);
    }

    #[test]
    fn normalize_task_backfills_timestamps() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "Test".to_string(),
            description: None,
            status: TaskStatus::Todo,
            ..Default::default()
        };
        let now = "2026-01-01T00:00:00.000000000Z";
        normalize_task(&mut task, now);
        assert_eq!(task.created_at, Some(now.to_string()));
        assert_eq!(task.updated_at, Some(now.to_string()));
        assert_eq!(task.completed_at, None);
    }

    #[test]
    fn normalize_task_backfills_completed_at_for_terminal() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "Test".to_string(),
            description: None,
            status: TaskStatus::Done,
            ..Default::default()
        };
        let now = "2026-01-01T00:00:00.000000000Z";
        normalize_task(&mut task, now);
        assert_eq!(task.completed_at, Some(now.to_string()));

        let mut task2 = Task {
            id: "RQ-0002".to_string(),
            title: "Test".to_string(),
            description: None,
            status: TaskStatus::Rejected,
            ..Default::default()
        };
        normalize_task(&mut task2, now);
        assert_eq!(task2.completed_at, Some(now.to_string()));
    }

    #[test]
    fn import_report_summary_format() {
        let report = ImportReport {
            parsed: 5,
            imported: 3,
            skipped_duplicates: 1,
            renamed: 1,
            rename_mappings: vec![("OLD-001".to_string(), "RQ-0001".to_string())],
        };
        let summary = report.summary();
        assert!(summary.contains("parsed 5"));
        assert!(summary.contains("imported 3"));
        assert!(summary.contains("skipped 1"));
        assert!(summary.contains("renamed 1"));
        assert!(summary.contains("OLD-001 -> RQ-0001"));
    }

    #[test]
    fn merge_imported_tasks_rename_records_mapping() {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![Task {
                id: "RQ-0001".to_string(),
                title: "Existing".to_string(),
                description: None,
                status: TaskStatus::Todo,
                created_at: Some("2026-01-01T00:00:00Z".to_string()),
                updated_at: Some("2026-01-01T00:00:00Z".to_string()),
                ..Default::default()
            }],
        };

        let imported = vec![Task {
            id: "RQ-0001".to_string(),
            title: "Duplicate".to_string(),
            description: None,
            status: TaskStatus::Todo,
            created_at: Some("2026-01-02T00:00:00Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00Z".to_string()),
            ..Default::default()
        }];

        let report = merge_imported_tasks(
            &mut queue,
            None,
            imported,
            "RQ",
            4,
            10,
            "2026-01-03T00:00:00Z",
            OnDuplicate::Rename,
        )
        .unwrap();

        assert_eq!(report.renamed, 1);
        assert_eq!(report.rename_mappings.len(), 1);
        assert_eq!(report.rename_mappings[0].0, "RQ-0001");
        assert!(report.rename_mappings[0].1.starts_with("RQ-"));
        assert_eq!(queue.tasks.len(), 2);
        assert!(queue.tasks.iter().any(|t| t.id == "RQ-0001"));
        let dup = queue
            .tasks
            .iter()
            .find(|t| t.title == "Duplicate")
            .expect("imported duplicate task");
        assert_ne!(dup.id, "RQ-0001");
    }
}
