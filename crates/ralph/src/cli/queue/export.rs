//! Queue export subcommand for exporting task data to various formats.
//!
//! Responsibilities:
//! - Export task data from queue and done archive to CSV, TSV, JSON, Markdown, or GitHub formats.
//! - Support filtering by status, tags, scope, ID patterns, and date ranges.
//! - Write output to file or stdout.
//!
//! Not handled here:
//! - Queue mutation or task modification (see `crate::queue::operations`).
//! - Complex data transformations or aggregation (see `crate::reports`).
//!
//! Invariants/assumptions:
//! - CSV/TSV output flattens arrays (tags, scope, etc.) into delimited strings.
//! - Markdown output produces GitHub-flavored Markdown tables with stable column ordering.
//! - GitHub format outputs one Markdown block per task optimized for issue bodies.
//! - Date filters expect RFC3339 or YYYY-MM-DD format and compare against created_at.
//! - Output encoding is UTF-8.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::contracts::{Task, TaskStatus};
use crate::queue;

use super::{QueueExportFormat, StatusArg};

/// Arguments for `ralph queue export`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue export\n  ralph queue export --format csv --output tasks.csv\n  ralph queue export --format json --status done\n  ralph queue export --format tsv --tag rust --tag cli\n  ralph queue export --include-archive --format csv\n  ralph queue export --format csv --created-after 2026-01-01\n  ralph queue export --format md --status todo\n  ralph queue export --format gh --status doing"
)]
pub struct QueueExportArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueExportFormat::Csv)]
    pub format: QueueExportFormat,

    /// Output file path (default: stdout).
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    pub tag: Vec<String>,

    /// Filter by scope token (repeatable, case-insensitive; substring match).
    #[arg(long)]
    pub scope: Vec<String>,

    /// Filter by task ID pattern (substring match).
    #[arg(long)]
    pub id_pattern: Option<String>,

    /// Filter tasks created after this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    pub created_after: Option<String>,

    /// Filter tasks created before this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    pub created_before: Option<String>,

    /// Include tasks from .ralph/done.json archive.
    #[arg(long)]
    pub include_archive: bool,

    /// Only export tasks from .ralph/done.json (ignores active queue).
    #[arg(long)]
    pub only_archive: bool,

    /// Suppress size warning output.
    #[arg(long, short)]
    pub quiet: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueExportArgs) -> Result<()> {
    // Validate conflicting flags
    if args.include_archive && args.only_archive {
        bail!(
            "Conflicting flags: --include-archive and --only-archive are mutually exclusive. Choose either to include archive tasks or to only show archive tasks."
        );
    }

    // Parse date filters
    let created_after = args
        .created_after
        .as_ref()
        .map(|d| parse_date_filter(d))
        .transpose()?;
    let created_before = args
        .created_before
        .as_ref()
        .map(|d| parse_date_filter(d))
        .transpose()?;

    // Load queue and optionally done file
    let (queue_file, done_file) =
        load_and_validate_queues(resolved, args.include_archive || args.only_archive)?;

    // Check queue size and print warning if needed
    if !args.quiet {
        let size_threshold =
            queue::size_threshold_or_default(resolved.config.queue.size_warning_threshold_kb);
        let count_threshold =
            queue::count_threshold_or_default(resolved.config.queue.task_count_warning_threshold);
        if let Ok(result) = queue::check_queue_size(
            &resolved.queue_path,
            queue_file.tasks.len(),
            size_threshold,
            count_threshold,
        ) {
            queue::print_size_warning_if_needed(&result, args.quiet);
        }
    }

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Collect tasks from appropriate sources
    let statuses: Vec<TaskStatus> = args.status.into_iter().map(|s| s.into()).collect();
    let mut tasks: Vec<&Task> = Vec::new();

    if !args.only_archive {
        tasks.extend(queue::filter_tasks(
            &queue_file,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }

    if (args.include_archive || args.only_archive)
        && let Some(done_ref) = done_ref
    {
        tasks.extend(queue::filter_tasks(
            done_ref,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }

    // Apply ID pattern filter if specified
    let tasks = if let Some(ref pattern) = args.id_pattern {
        let pattern_lower = pattern.to_lowercase();
        tasks
            .into_iter()
            .filter(|t| t.id.to_lowercase().contains(&pattern_lower))
            .collect()
    } else {
        tasks
    };

    // Apply date filters
    let tasks: Vec<&Task> = tasks
        .into_iter()
        .filter(|t| {
            if let Some(ref after_date) = created_after {
                if let Some(ref created) = t.created_at {
                    if let Ok(created_ts) = parse_timestamp(created)
                        && created_ts < *after_date
                    {
                        return false;
                    }
                } else {
                    // Tasks without created_at are excluded when date filter is active
                    return false;
                }
            }
            if let Some(ref before_date) = created_before {
                if let Some(ref created) = t.created_at {
                    if let Ok(created_ts) = parse_timestamp(created)
                        && created_ts > *before_date
                    {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        })
        .collect();

    // Generate output
    let output = match args.format {
        QueueExportFormat::Csv => export_csv(&tasks, ',')?,
        QueueExportFormat::Tsv => export_csv(&tasks, '\t')?,
        QueueExportFormat::Json => export_json(&tasks)?,
        QueueExportFormat::Md => export_markdown_table(&tasks)?,
        QueueExportFormat::Gh => export_github_issue(&tasks)?,
    };

    // Write output
    if let Some(path) = args.output {
        std::fs::write(&path, output)
            .with_context(|| format!("Failed to write export to {}", path.display()))?;
    } else {
        std::io::stdout()
            .write_all(output.as_bytes())
            .context("Failed to write to stdout")?;
    }

    Ok(())
}

/// Parse a date filter string into a timestamp for comparison.
/// Accepts RFC3339 (2026-01-15T00:00:00Z) or YYYY-MM-DD format.
fn parse_date_filter(input: &str) -> Result<i64> {
    // Try RFC3339 first
    if let Ok(dt) =
        time::OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339)
    {
        return Ok(dt.unix_timestamp());
    }

    // Try YYYY-MM-DD
    let format = time::format_description::parse("[year]-[month]-[day]")
        .context("Failed to parse date format description")?;
    if let Ok(date) = time::Date::parse(input, &format) {
        let dt = time::OffsetDateTime::new_utc(date, time::Time::MIDNIGHT);
        return Ok(dt.unix_timestamp());
    }

    bail!(
        "Invalid date format: '{}'. Expected RFC3339 (2026-01-15T00:00:00Z) or YYYY-MM-DD",
        input
    )
}

/// Parse a task timestamp string into a unix timestamp for comparison.
fn parse_timestamp(input: &str) -> Result<i64> {
    // Try RFC3339 first
    if let Ok(dt) =
        time::OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339)
    {
        return Ok(dt.unix_timestamp());
    }

    // Try YYYY-MM-DD as fallback
    let format = time::format_description::parse("[year]-[month]-[day]")
        .context("Failed to parse date format description")?;
    if let Ok(date) = time::Date::parse(input, &format) {
        let dt = time::OffsetDateTime::new_utc(date, time::Time::MIDNIGHT);
        return Ok(dt.unix_timestamp());
    }

    bail!("Invalid timestamp format: '{}'", input)
}

/// Export tasks to CSV/TSV format.
fn export_csv(tasks: &[&Task], delimiter: char) -> Result<String> {
    let mut output = String::new();

    // Header
    let headers = [
        "id",
        "title",
        "status",
        "priority",
        "tags",
        "scope",
        "evidence",
        "plan",
        "notes",
        "request",
        "created_at",
        "updated_at",
        "completed_at",
        "depends_on",
        "custom_fields",
        "parent_id",
    ];
    output.push_str(&headers.join(&delimiter.to_string()));
    output.push('\n');

    for task in tasks {
        let tags = task.tags.join(",");
        let scope = task.scope.join(",");
        let evidence = task.evidence.join("; ");
        let plan = task.plan.join("; ");
        let notes = task.notes.join("; ");
        let depends_on = task.depends_on.join(",");
        let custom_fields = task
            .custom_fields
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");

        let fields = [
            escape_csv_field(&task.id, delimiter),
            escape_csv_field(&task.title, delimiter),
            task.status.as_str().to_string(),
            task.priority.as_str().to_string(),
            escape_csv_field(&tags, delimiter),
            escape_csv_field(&scope, delimiter),
            escape_csv_field(&evidence, delimiter),
            escape_csv_field(&plan, delimiter),
            escape_csv_field(&notes, delimiter),
            escape_csv_field(task.request.as_deref().unwrap_or(""), delimiter),
            escape_csv_field(task.created_at.as_deref().unwrap_or(""), delimiter),
            escape_csv_field(task.updated_at.as_deref().unwrap_or(""), delimiter),
            escape_csv_field(task.completed_at.as_deref().unwrap_or(""), delimiter),
            escape_csv_field(&depends_on, delimiter),
            escape_csv_field(&custom_fields, delimiter),
            escape_csv_field(task.parent_id.as_deref().unwrap_or(""), delimiter),
        ];
        let row = format!("{}\n", fields.join(&delimiter.to_string()));

        output.push_str(&row);
    }

    Ok(output)
}

/// Escape a field for CSV/TSV output.
/// Fields containing the delimiter, quotes, or newlines are quoted.
fn escape_csv_field(field: &str, delimiter: char) -> String {
    let delimiter_str = delimiter.to_string();
    if field.contains(&delimiter_str) || field.contains('"') || field.contains('\n') {
        // Double up quotes and wrap in quotes
        let escaped = field.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        field.to_string()
    }
}

/// Export tasks to JSON format.
fn export_json(tasks: &[&Task]) -> Result<String> {
    // Convert Vec<&Task> to Vec<Task> for serialization
    let owned_tasks: Vec<Task> = tasks.iter().map(|&t| t.clone()).collect();
    let output =
        serde_json::to_string_pretty(&owned_tasks).context("Failed to serialize tasks to JSON")?;
    Ok(output)
}

/// Export tasks to Markdown table format.
fn export_markdown_table(tasks: &[&Task]) -> Result<String> {
    // Sort tasks by ID for deterministic output
    let mut sorted_tasks: Vec<&Task> = tasks.to_vec();
    sorted_tasks.sort_by(|a, b| a.id.cmp(&b.id));

    let mut output = String::new();

    // Header
    output.push_str("| ID | Status | Priority | Title | Tags | Scope | Created |\n");
    output.push_str("|---|---|---|---|---|---|---|\n");

    // Rows
    for task in sorted_tasks {
        let tags = if task.tags.is_empty() {
            "".to_string()
        } else {
            format!("`{}`", task.tags.join("`, `"))
        };

        let scope = if task.scope.is_empty() {
            "".to_string()
        } else if task.scope.len() > 2 {
            format!("`{}` (+{})", task.scope[0], task.scope.len() - 1)
        } else {
            format!("`{}`", task.scope.join("`, `"))
        };

        let title = escape_markdown_table_cell(&task.title);
        let created = task.created_at.as_deref().unwrap_or("-");
        let date_part = created.split('T').next().unwrap_or(created);

        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            task.id,
            task.status.as_str(),
            task.priority.as_str(),
            title,
            tags,
            scope,
            date_part,
        ));
    }

    Ok(output)
}

/// Render a single task as a GitHub issue body (without the H2 title header).
///
/// This is used for publishing tasks to GitHub Issues. The title is omitted
/// because GitHub issues have their own title field.
pub(crate) fn render_task_as_github_issue_body(task: &Task) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "**Status:** `{}` | **Priority:** `{}`\n",
        task.status.as_str(),
        task.priority.as_str()
    ));

    if !task.tags.is_empty() {
        out.push('\n');
        out.push_str(&format!("**Tags:** `{}`\n", task.tags.join("`, `")));
    }

    // Plan
    if !task.plan.is_empty() {
        out.push('\n');
        out.push_str("### Plan\n\n");
        for item in &task.plan {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }

    // Evidence
    if !task.evidence.is_empty() {
        out.push('\n');
        out.push_str("### Evidence\n\n");
        for item in &task.evidence {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }

    // Scope
    if !task.scope.is_empty() {
        out.push('\n');
        out.push_str("### Scope\n\n");
        for item in &task.scope {
            out.push_str("- `");
            out.push_str(item);
            out.push_str("`\n");
        }
    }

    // Notes
    if !task.notes.is_empty() {
        out.push('\n');
        out.push_str("### Notes\n\n");
        for item in &task.notes {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }

    if !task.depends_on.is_empty() {
        out.push('\n');
        out.push_str(&format!("**Depends on:** {}\n", task.depends_on.join(", ")));
    }

    if let Some(ref request) = task.request {
        out.push('\n');
        out.push_str("### Original Request\n\n");
        out.push_str(request);
        out.push('\n');
    }

    // Marker for future automation/debugging
    out.push('\n');
    out.push_str(&format!("<!-- ralph_task_id: {} -->\n", task.id));

    out
}

/// Export tasks to GitHub issue format.
fn export_github_issue(tasks: &[&Task]) -> Result<String> {
    // Sort tasks by ID for deterministic output
    let mut sorted_tasks: Vec<&Task> = tasks.to_vec();
    sorted_tasks.sort_by(|a, b| a.id.cmp(&b.id));

    let mut output = String::new();

    for (i, task) in sorted_tasks.iter().enumerate() {
        if i > 0 {
            output.push('\n');
            output.push_str("---");
            output.push('\n');
            output.push('\n');
        }

        // Title as H2 (for export, we include the title header)
        output.push_str(&format!("## {}: {}\n\n", task.id, task.title));

        // Use the shared body renderer
        let body = render_task_as_github_issue_body(task);
        // Remove the marker line since it's not needed in export format
        let trimmed_body = body
            .trim_end()
            .trim_end_matches(&format!("<!-- ralph_task_id: {} -->", task.id))
            .trim_end();
        output.push_str(trimmed_body);
        output.push('\n');
    }

    Ok(output)
}

/// Escape Markdown special characters for table cells.
fn escape_markdown_table_cell(text: &str) -> String {
    // In Markdown tables, pipes break the table, so we escape them
    text.replace('|', "\\|")
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            status,
            priority: crate::contracts::TaskPriority::Medium,
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["evidence".to_string()],
            plan: vec!["step 1".to_string(), "step 2".to_string()],
            notes: vec!["note".to_string()],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-15T00:00:00Z".to_string()),
            updated_at: Some("2026-01-15T12:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec!["RQ-0001".to_string()],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn csv_export_includes_all_fields() {
        let task = create_test_task("RQ-0002", "Test Task", TaskStatus::Todo);
        let tasks = vec![&task];

        let csv = export_csv(&tasks, ',').unwrap();

        assert!(csv.contains("id,title,status,priority"));
        assert!(csv.contains("parent_id")); // new field
        assert!(csv.contains("RQ-0002"));
        assert!(csv.contains("Test Task"));
        assert!(csv.contains("todo"));
        assert!(csv.contains("medium"));
        assert!(csv.contains("test")); // tag
        assert!(csv.contains("crates/ralph")); // scope
    }

    #[test]
    fn tsv_export_uses_tab_delimiter() {
        let task = create_test_task("RQ-0001", "Test", TaskStatus::Done);
        let tasks = vec![&task];

        let tsv = export_csv(&tasks, '\t').unwrap();

        // Header should use tabs
        assert!(tsv.contains("id\ttitle\tstatus"));
        // Should not have commas in data rows (except within fields)
        assert!(!tsv.lines().nth(1).unwrap().contains(','));
    }

    #[test]
    fn json_export_produces_valid_json() {
        let task = create_test_task("RQ-0001", "Test Task", TaskStatus::Todo);
        let tasks = vec![&task];

        let json = export_json(&tasks).unwrap();

        // Should be valid JSON array
        assert!(json.starts_with('['));
        assert!(json.ends_with(']'));
        assert!(json.contains("RQ-0001"));
        assert!(json.contains("Test Task"));
    }

    #[test]
    fn escape_csv_field_handles_special_chars() {
        // Field with comma should be quoted
        let field1 = escape_csv_field("hello, world", ',');
        assert_eq!(field1, "\"hello, world\"");

        // Field with quote should have quotes doubled
        let field2 = escape_csv_field("say \"hello\"", ',');
        assert_eq!(field2, "\"say \"\"hello\"\"\"");

        // Field with newline should be quoted
        let field3 = escape_csv_field("line1\nline2", ',');
        assert_eq!(field3, "\"line1\nline2\"");

        // Normal field should not be quoted
        let field4 = escape_csv_field("simple", ',');
        assert_eq!(field4, "simple");
    }

    #[test]
    fn parse_date_filter_accepts_rfc3339() {
        let ts = parse_date_filter("2026-01-15T00:00:00Z").unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn parse_date_filter_accepts_ymd() {
        let ts = parse_date_filter("2026-01-15").unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn parse_date_filter_rejects_invalid() {
        let result = parse_date_filter("not-a-date");
        assert!(result.is_err());
    }

    #[test]
    fn markdown_export_produces_valid_table() {
        let task1 = create_test_task("RQ-0001", "First Task", TaskStatus::Todo);
        let task2 = create_test_task("RQ-0002", "Second Task", TaskStatus::Doing);
        let tasks = vec![&task1, &task2];

        let md = export_markdown_table(&tasks).unwrap();

        // Should have header row
        assert!(md.contains("| ID | Status | Priority | Title |"));
        // Should have separator row
        assert!(md.contains("|---|---|---"));
        // Should contain task data
        assert!(md.contains("RQ-0001"));
        assert!(md.contains("First Task"));
        assert!(md.contains("todo"));
        assert!(md.contains("RQ-0002"));
    }

    #[test]
    fn markdown_export_escapes_pipes() {
        let task = create_test_task("RQ-0001", "Task | With | Pipes", TaskStatus::Todo);
        let tasks = vec![&task];

        let md = export_markdown_table(&tasks).unwrap();

        // Pipes should be escaped to not break table
        assert!(md.contains("Task \\| With \\| Pipes"));
    }

    #[test]
    fn markdown_export_is_deterministic() {
        let task1 = create_test_task("RQ-0002", "Second", TaskStatus::Todo);
        let task2 = create_test_task("RQ-0001", "First", TaskStatus::Todo);
        let tasks = vec![&task1, &task2];

        let md1 = export_markdown_table(&tasks).unwrap();
        let md2 = export_markdown_table(&tasks).unwrap();

        assert_eq!(md1, md2);
        // Should be sorted by ID
        assert!(md1.find("RQ-0001").unwrap() < md1.find("RQ-0002").unwrap());
    }

    #[test]
    fn github_export_produces_valid_markdown() {
        let task = create_test_task("RQ-0001", "Test Task", TaskStatus::Todo);
        let tasks = vec![&task];

        let gh = export_github_issue(&tasks).unwrap();

        // Should have H2 title
        assert!(gh.contains("## RQ-0001: Test Task"));
        // Should have status
        assert!(gh.contains("**Status:**"));
        // Should have priority
        assert!(gh.contains("**Priority:**"));
        // Should have plan section
        assert!(gh.contains("### Plan"));
        // Should have evidence section
        assert!(gh.contains("### Evidence"));
    }

    #[test]
    fn github_export_omits_empty_sections() {
        let mut task = create_test_task("RQ-0001", "Test", TaskStatus::Todo);
        task.plan = vec![]; // Empty plan
        task.evidence = vec!["Some evidence".to_string()];
        let tasks = vec![&task];

        let gh = export_github_issue(&tasks).unwrap();

        // Should have evidence section
        assert!(gh.contains("### Evidence"));
        // Should NOT have plan section
        assert!(!gh.contains("### Plan\n\n"));
    }

    #[test]
    fn github_export_multiple_tasks_separates_with_hr() {
        let task1 = create_test_task("RQ-0001", "First", TaskStatus::Todo);
        let task2 = create_test_task("RQ-0002", "Second", TaskStatus::Todo);
        let tasks = vec![&task1, &task2];

        let gh = export_github_issue(&tasks).unwrap();

        // Should have horizontal rule between tasks
        assert!(gh.contains("\n---\n"));
    }
}
