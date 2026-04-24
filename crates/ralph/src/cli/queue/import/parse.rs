//! Queue import parsing helpers.
//!
//! Purpose:
//! - Queue import parsing helpers.
//!
//! Responsibilities:
//! - Parse JSON, CSV, and TSV payloads into `Task` values.
//! - Normalize field-level import syntax such as list and custom field columns.
//! - Keep parsing errors format-specific and actionable.
//!
//! Not handled here:
//! - Timestamp backfill or task normalization.
//! - Duplicate handling and queue mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - CSV/TSV inputs must provide a `title` column.
//! - JSON wrapper imports support only `version = 1`.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use crate::contracts::{Task, TaskStatus};

pub(super) fn parse_json_tasks(input: &str) -> Result<Vec<Task>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    match serde_json::from_str::<Vec<Task>>(trimmed) {
        Ok(tasks) => Ok(tasks),
        Err(array_error) => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct TasksWrapper {
                #[serde(default)]
                version: Option<u32>,
                tasks: Vec<Task>,
            }

            match serde_json::from_str::<TasksWrapper>(trimmed) {
                Ok(wrapper) => {
                    if let Some(version) = wrapper.version
                        && version != 1
                    {
                        bail!(
                            "Unsupported wrapper version: {}. Only version 1 is supported.",
                            version
                        );
                    }
                    Ok(wrapper.tasks)
                }
                Err(_) => bail!(
                    "Invalid JSON format: {}. Expected array of tasks or {{\"version\": 1, \"tasks\": [...]}} wrapper.",
                    array_error
                ),
            }
        }
    }
}

pub(super) fn parse_csv_tasks(input: &str, delimiter: u8) -> Result<Vec<Task>> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(input.as_bytes());

    let headers = reader
        .headers()?
        .iter()
        .map(|header| header.to_lowercase())
        .collect::<Vec<_>>();
    let header_map: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.clone(), index))
        .collect();

    if !header_map.contains_key("title") {
        bail!("CSV/TSV import requires a 'title' column");
    }

    let mut tasks = Vec::new();
    for (row_idx, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("parse CSV row {}", row_idx + 1))?;
        let mut task = Task::default();

        let title_idx = header_map["title"];
        task.title = record
            .get(title_idx)
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        if task.title.is_empty() {
            bail!("Row {}: title is required and cannot be empty", row_idx + 1);
        }

        if let Some(&idx) = header_map.get("id") {
            task.id = record
                .get(idx)
                .map(|value| value.trim().to_string())
                .unwrap_or_default();
        }
        if let Some(&idx) = header_map.get("status") {
            let status = record.get(idx).unwrap_or("").trim().to_lowercase();
            if !status.is_empty() {
                task.status = parse_status(&status)?;
            }
        }
        if let Some(&idx) = header_map.get("priority") {
            let trimmed = record.get(idx).unwrap_or("").trim();
            if !trimmed.is_empty() {
                task.priority = trimmed.parse()?;
            }
        }
        if let Some(&idx) = header_map.get("tags") {
            task.tags = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }
        if let Some(&idx) = header_map.get("scope") {
            task.scope = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }
        if let Some(&idx) = header_map.get("evidence") {
            task.evidence = parse_list_field(record.get(idx).unwrap_or(""), ';');
        }
        if let Some(&idx) = header_map.get("plan") {
            task.plan = parse_list_field(record.get(idx).unwrap_or(""), ';');
        }
        if let Some(&idx) = header_map.get("notes") {
            task.notes = parse_list_field(record.get(idx).unwrap_or(""), ';');
        }
        if let Some(&idx) = header_map.get("request") {
            let request = record.get(idx).unwrap_or("").trim().to_string();
            task.request = (!request.is_empty()).then_some(request);
        }
        if let Some(&idx) = header_map.get("created_at") {
            let created_at = record.get(idx).unwrap_or("").trim().to_string();
            task.created_at = (!created_at.is_empty()).then_some(created_at);
        }
        if let Some(&idx) = header_map.get("updated_at") {
            let updated_at = record.get(idx).unwrap_or("").trim().to_string();
            task.updated_at = (!updated_at.is_empty()).then_some(updated_at);
        }
        if let Some(&idx) = header_map.get("completed_at") {
            let completed_at = record.get(idx).unwrap_or("").trim().to_string();
            task.completed_at = (!completed_at.is_empty()).then_some(completed_at);
        }
        if let Some(&idx) = header_map.get("depends_on") {
            task.depends_on = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }
        if let Some(&idx) = header_map.get("blocks") {
            task.blocks = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }
        if let Some(&idx) = header_map.get("relates_to") {
            task.relates_to = parse_list_field(record.get(idx).unwrap_or(""), ',');
        }
        if let Some(&idx) = header_map.get("duplicates") {
            let duplicate = record.get(idx).unwrap_or("").trim().to_string();
            task.duplicates = (!duplicate.is_empty()).then_some(duplicate);
        }
        if let Some(&idx) = header_map.get("custom_fields") {
            task.custom_fields = parse_custom_fields(record.get(idx).unwrap_or(""))?;
        }
        if let Some(&idx) = header_map.get("parent_id") {
            let parent_id = record.get(idx).unwrap_or("").trim().to_string();
            task.parent_id = (!parent_id.is_empty()).then_some(parent_id);
        }

        tasks.push(task);
    }

    Ok(tasks)
}

pub(super) fn parse_list_field(value: &str, delimiter: char) -> Vec<String> {
    value
        .split(delimiter)
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(super) fn parse_custom_fields(value: &str) -> Result<HashMap<String, String>> {
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
        let value = parts[1].trim();
        if key.is_empty() {
            bail!("Empty custom field key in '{}'", pair);
        }
        if key.chars().any(|character| character.is_whitespace()) {
            bail!("Custom field key cannot contain whitespace: '{}'", key);
        }
        fields.insert(key.to_string(), value.to_string());
    }

    Ok(fields)
}

pub(super) fn parse_status(status: &str) -> Result<TaskStatus> {
    match status.to_lowercase().as_str() {
        "draft" => Ok(TaskStatus::Draft),
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "done" => Ok(TaskStatus::Done),
        "rejected" => Ok(TaskStatus::Rejected),
        _ => bail!(
            "Invalid status: '{}'. Expected: draft, todo, doing, done, rejected",
            status
        ),
    }
}
