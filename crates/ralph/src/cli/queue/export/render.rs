//! Output rendering helpers for queue export formats.
//!
//! Purpose:
//! - Output rendering helpers for queue export formats.
//!
//! Responsibilities:
//! - Render filtered tasks as CSV, TSV, JSON, Markdown, or GitHub issue markdown.
//! - Keep format-specific escaping and deterministic ordering centralized.
//! - Provide the shared GitHub issue body renderer used by queue issue flows.
//!
//! Not handled here:
//! - Queue loading or filter selection.
//! - CLI file/stdout IO orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Markdown and GitHub exports sort by task ID for deterministic output.
//! - CSV/TSV fields are escaped when delimiters, quotes, or newlines are present.

use anyhow::{Context, Result};

use crate::cli::queue::QueueExportFormat;
use crate::contracts::Task;

pub(super) fn render_export(format: QueueExportFormat, tasks: &[&Task]) -> Result<String> {
    match format {
        QueueExportFormat::Csv => export_csv(tasks, ','),
        QueueExportFormat::Tsv => export_csv(tasks, '\t'),
        QueueExportFormat::Json => export_json(tasks),
        QueueExportFormat::Md => export_markdown_table(tasks),
        QueueExportFormat::Gh => export_github_issue(tasks),
    }
}

fn export_csv(tasks: &[&Task], delimiter: char) -> Result<String> {
    let mut output = String::new();
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
        output.push_str(&csv_row(task, delimiter));
        output.push('\n');
    }

    Ok(output)
}

fn csv_row(task: &Task, delimiter: char) -> String {
    let tags = task.tags.join(",");
    let scope = task.scope.join(",");
    let evidence = task.evidence.join("; ");
    let plan = task.plan.join("; ");
    let notes = task.notes.join("; ");
    let depends_on = task.depends_on.join(",");
    let custom_fields = task
        .custom_fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");

    [
        escape_csv_field(&task.id, delimiter),
        escape_csv_field(&task.title, delimiter),
        task.status.as_str().to_string(),
        task.priority.as_str().to_string(),
        escape_csv_field(&tags, delimiter),
        escape_csv_field(&scope, delimiter),
        escape_csv_field(&evidence, delimiter),
        escape_csv_field(&plan, delimiter),
        escape_csv_field(&notes, delimiter),
        escape_csv_field(task.request.as_deref().unwrap_or_default(), delimiter),
        escape_csv_field(task.created_at.as_deref().unwrap_or_default(), delimiter),
        escape_csv_field(task.updated_at.as_deref().unwrap_or_default(), delimiter),
        escape_csv_field(task.completed_at.as_deref().unwrap_or_default(), delimiter),
        escape_csv_field(&depends_on, delimiter),
        escape_csv_field(&custom_fields, delimiter),
        escape_csv_field(task.parent_id.as_deref().unwrap_or_default(), delimiter),
    ]
    .join(&delimiter.to_string())
}

fn escape_csv_field(field: &str, delimiter: char) -> String {
    let delimiter_str = delimiter.to_string();
    if field.contains(&delimiter_str) || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn export_json(tasks: &[&Task]) -> Result<String> {
    let owned_tasks: Vec<Task> = tasks.iter().map(|task| (*task).clone()).collect();
    serde_json::to_string_pretty(&owned_tasks).context("Failed to serialize tasks to JSON")
}

fn export_markdown_table(tasks: &[&Task]) -> Result<String> {
    let mut output = String::new();
    output.push_str("| ID | Status | Priority | Title | Tags | Scope | Created |\n");
    output.push_str("|---|---|---|---|---|---|---|\n");

    for task in sorted_tasks(tasks) {
        let title = escape_markdown_table_cell(&task.title);
        let created = task.created_at.as_deref().unwrap_or("-");
        let date_part = created.split('T').next().unwrap_or(created);

        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            task.id,
            task.status.as_str(),
            task.priority.as_str(),
            title,
            render_tag_list(&task.tags),
            render_scope_list(&task.scope),
            date_part,
        ));
    }

    Ok(output)
}

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

    push_markdown_list_section(&mut out, "Plan", &task.plan);
    push_markdown_list_section(&mut out, "Evidence", &task.evidence);

    if !task.scope.is_empty() {
        out.push('\n');
        out.push_str("### Scope\n\n");
        for item in &task.scope {
            out.push_str("- `");
            out.push_str(item);
            out.push_str("`\n");
        }
    }

    push_markdown_list_section(&mut out, "Notes", &task.notes);

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

    out.push('\n');
    out.push_str(&format!("<!-- ralph_task_id: {} -->\n", task.id));
    out
}

fn export_github_issue(tasks: &[&Task]) -> Result<String> {
    let mut output = String::new();

    for (index, task) in sorted_tasks(tasks).into_iter().enumerate() {
        if index > 0 {
            output.push_str("\n---\n\n");
        }

        output.push_str(&format!("## {}: {}\n\n", task.id, task.title));
        output.push_str(trim_marker(
            &render_task_as_github_issue_body(task),
            &task.id,
        ));
        output.push('\n');
    }

    Ok(output)
}

fn sorted_tasks<'a>(tasks: &[&'a Task]) -> Vec<&'a Task> {
    let mut sorted_tasks = tasks.to_vec();
    sorted_tasks.sort_by(|left, right| left.id.cmp(&right.id));
    sorted_tasks
}

fn render_tag_list(tags: &[String]) -> String {
    if tags.is_empty() {
        String::new()
    } else {
        format!("`{}`", tags.join("`, `"))
    }
}

fn render_scope_list(scope: &[String]) -> String {
    if scope.is_empty() {
        String::new()
    } else if scope.len() > 2 {
        format!("`{}` (+{})", scope[0], scope.len() - 1)
    } else {
        format!("`{}`", scope.join("`, `"))
    }
}

fn push_markdown_list_section(out: &mut String, heading: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    out.push('\n');
    out.push_str(&format!("### {heading}\n\n"));
    for item in items {
        out.push_str("- ");
        out.push_str(item);
        out.push('\n');
    }
}

fn trim_marker<'a>(body: &'a str, task_id: &str) -> &'a str {
    body.trim_end()
        .trim_end_matches(&format!("<!-- ralph_task_id: {task_id} -->"))
        .trim_end()
}

fn escape_markdown_table_cell(text: &str) -> String {
    text.replace('|', "\\|")
}
