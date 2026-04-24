//! PRD task generation.
//!
//! Purpose:
//! - PRD task generation.
//!
//! Responsibilities:
//! - Turn parsed PRD structures into single-task or multi-task queue entries.
//! - Apply common tags, dependency chaining, and request/body construction.
//!
//! Not handled here:
//! - Markdown parsing.
//! - Queue load/save orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Generated task IDs come from queue-aware helpers.
//! - Multi-task mode chains stories sequentially via `depends_on`.

use std::collections::HashMap;

use anyhow::Result;

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;

use super::parse::ParsedPrd;

#[allow(clippy::too_many_arguments)]
pub(super) fn generate_single_task(
    parsed: &ParsedPrd,
    now: &str,
    priority: TaskPriority,
    status: TaskStatus,
    extra_tags: &[String],
    queue_file: &QueueFile,
    done_file: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<Task> {
    let id = queue::next_id_across(
        queue_file,
        done_file,
        id_prefix,
        id_width,
        max_dependency_depth,
    )?;

    let mut plan = parsed.functional_requirements.clone();
    for story in &parsed.user_stories {
        if !story.acceptance_criteria.is_empty() {
            plan.push(format!("{}: {}", story.id, story.title));
            for criterion in &story.acceptance_criteria {
                plan.push(format!("  - {}", criterion));
            }
        }
    }

    let request = if parsed.introduction.is_empty() {
        format!("Created from PRD: {}", parsed.title)
    } else {
        format!(
            "{}\n\nCreated from PRD: {}",
            parsed.introduction, parsed.title
        )
    };

    Ok(Task {
        id,
        title: parsed.title.clone(),
        description: None,
        status,
        priority,
        tags: build_tags(extra_tags, &["prd"]),
        scope: Vec::new(),
        evidence: Vec::new(),
        plan,
        notes: parsed.non_goals.clone(),
        request: Some(request),
        agent: None,
        created_at: Some(now.to_string()),
        updated_at: Some(now.to_string()),
        completed_at: None,
        started_at: None,
        estimated_minutes: None,
        actual_minutes: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn generate_multi_tasks(
    parsed: &ParsedPrd,
    now: &str,
    priority: TaskPriority,
    status: TaskStatus,
    extra_tags: &[String],
    queue_file: &QueueFile,
    done_file: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<Vec<Task>> {
    if parsed.user_stories.is_empty() {
        return Ok(vec![generate_single_task(
            parsed,
            now,
            priority,
            status,
            extra_tags,
            queue_file,
            done_file,
            id_prefix,
            id_width,
            max_dependency_depth,
        )?]);
    }

    let mut tasks: Vec<Task> = Vec::new();
    let mut previous_ids = Vec::new();

    for (index, story) in parsed.user_stories.iter().enumerate() {
        let mut temp_queue = queue_file.clone();
        for task in &tasks {
            temp_queue.tasks.push(task.clone());
        }

        let id = queue::next_id_across(
            &temp_queue,
            done_file,
            id_prefix,
            id_width,
            max_dependency_depth,
        )?;
        let title = if parsed.title.is_empty() {
            story.title.clone()
        } else {
            format!("[{}] {}", parsed.title, story.title)
        };
        let request = if story.description.is_empty() {
            format!("User story {} from PRD: {}", story.id, parsed.title)
        } else {
            story.description.clone()
        };
        let depends_on = if index > 0 {
            previous_ids.last().cloned().into_iter().collect()
        } else {
            Vec::new()
        };

        previous_ids.push(id.clone());
        tasks.push(Task {
            id,
            title,
            description: None,
            status,
            priority,
            tags: build_tags(extra_tags, &["prd", "user-story"]),
            scope: Vec::new(),
            evidence: Vec::new(),
            plan: story.acceptance_criteria.clone(),
            notes: Vec::new(),
            request: Some(request),
            agent: None,
            created_at: Some(now.to_string()),
            updated_at: Some(now.to_string()),
            completed_at: None,
            started_at: None,
            estimated_minutes: None,
            actual_minutes: None,
            scheduled_start: None,
            depends_on,
            blocks: Vec::new(),
            relates_to: Vec::new(),
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        });
    }

    Ok(tasks)
}

fn build_tags(extra_tags: &[String], defaults: &[&str]) -> Vec<String> {
    let mut tags = defaults
        .iter()
        .map(|tag| (*tag).to_string())
        .collect::<Vec<_>>();
    for tag in extra_tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    tags
}
