//! Shared task materialization and normalization helpers for queue growth flows.
//!
//! Purpose:
//! - Provide one canonical queue-owned path for turning normalized task specs into validated queue tasks.
//!
//! Responsibilities:
//! - Allocate contiguous task IDs across active and done queues.
//! - Validate local task keys, local dependency references, and parent linkage.
//! - Materialize normalized task specs into durable `Task` records with stamped timestamps.
//! - Apply canonical insertion strategies for top-level inserts, parent attachment, subtree append, and subtree replacement.
//! - Normalize runner-created tasks with canonical insertion and missing-field defaults.
//!
//! Non-scope:
//! - Parsing runner/planner/proposal output into normalized specs.
//! - Queue persistence, lock management, or undo orchestration.
//! - Feature-specific annotations or provenance decisions outside the created task specs.
//!
//! Usage:
//! - Used by follow-up application, task decomposition writes, and runner-backed task build normalization.
//!
//! Invariants/assumptions:
//! - Input specs are already ordered in the desired creation/insertion order.
//! - Local keys are unique within one materialization request.
//! - Parent-local and dependency-local references point only within the same request.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;
use anyhow::{Context, Result, anyhow, bail};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct MaterializedTaskSpec {
    pub local_key: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub tags: Vec<String>,
    pub scope: Vec<String>,
    pub evidence: Vec<String>,
    pub plan: Vec<String>,
    pub notes: Vec<String>,
    pub request: Option<String>,
    pub relates_to: Vec<String>,
    pub parent_local_key: Option<String>,
    pub parent_task_id: Option<String>,
    pub depends_on_local_keys: Vec<String>,
    pub estimated_minutes: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum MaterializeInsertion {
    QueueDefaultTop,
    AfterParent {
        parent_task_id: String,
    },
    AppendUnderParent {
        parent_task_id: String,
        existing_subtree_task_ids: Vec<String>,
    },
    ReplaceSubtree {
        parent_task_id: String,
        removed_subtree_task_ids: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct MaterializeTaskGraphOptions<'a> {
    pub now_rfc3339: &'a str,
    pub id_prefix: &'a str,
    pub id_width: usize,
    pub max_dependency_depth: u8,
    pub insertion: MaterializeInsertion,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct MaterializeTaskGraphReport {
    pub created_tasks: Vec<Task>,
    pub local_key_to_id: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CreatedTaskNormalization<'a> {
    pub insert_at: usize,
    pub default_request: &'a str,
    pub now_rfc3339: &'a str,
    pub estimated_minutes: Option<u32>,
}

pub fn apply_materialized_task_graph(
    active: &mut QueueFile,
    done: Option<&QueueFile>,
    specs: &[MaterializedTaskSpec],
    options: &MaterializeTaskGraphOptions<'_>,
) -> Result<MaterializeTaskGraphReport> {
    validate_specs(specs)?;
    let local_key_to_id = allocate_ids(
        active,
        done,
        specs,
        options.id_prefix,
        options.id_width,
        options.max_dependency_depth,
    )?;
    let created_tasks = materialize_tasks(specs, &local_key_to_id, options.now_rfc3339)?;

    let mut preview = active.clone();
    let insert_at = prepare_preview_for_insertion(&mut preview, done, &options.insertion)?;
    for (offset, task) in created_tasks.iter().cloned().enumerate() {
        preview.tasks.insert(insert_at + offset, task);
    }

    let warnings = queue::validate_queue_set(
        &preview,
        done,
        options.id_prefix,
        options.id_width,
        options.max_dependency_depth,
    )
    .context("validate queue after materializing tasks")?;
    queue::log_warnings(&warnings);

    if !options.dry_run {
        *active = preview;
    }

    Ok(MaterializeTaskGraphReport {
        created_tasks,
        local_key_to_id,
    })
}

pub fn normalize_created_tasks(
    queue_file: &mut QueueFile,
    new_task_ids: &[String],
    defaults: &CreatedTaskNormalization<'_>,
) {
    queue::reposition_new_tasks(queue_file, new_task_ids, defaults.insert_at);
    queue::backfill_missing_fields(
        queue_file,
        new_task_ids,
        defaults.default_request,
        defaults.now_rfc3339,
    );

    if let Some(estimated_minutes) = defaults.estimated_minutes {
        let new_task_set: HashSet<&str> = new_task_ids.iter().map(|id| id.as_str()).collect();
        for task in &mut queue_file.tasks {
            if new_task_set.contains(task.id.as_str()) {
                task.estimated_minutes = Some(estimated_minutes);
            }
        }
    }
}

pub fn ensure_subtree_is_replaceable(
    active: &QueueFile,
    done: Option<&QueueFile>,
    removed_ids: &HashSet<String>,
) -> Result<()> {
    let mut references = Vec::new();
    for task in active
        .tasks
        .iter()
        .chain(done.into_iter().flat_map(|queue| queue.tasks.iter()))
    {
        if removed_ids.contains(&task.id) {
            continue;
        }
        for dep in &task.depends_on {
            if removed_ids.contains(dep) {
                references.push(format!("{} depends_on {}", task.id, dep));
            }
        }
        for blocked in &task.blocks {
            if removed_ids.contains(blocked) {
                references.push(format!("{} blocks {}", task.id, blocked));
            }
        }
        for related in &task.relates_to {
            if removed_ids.contains(related) {
                references.push(format!("{} relates_to {}", task.id, related));
            }
        }
        if let Some(duplicate_id) = &task.duplicates
            && removed_ids.contains(duplicate_id)
        {
            references.push(format!("{} duplicates {}", task.id, duplicate_id));
        }
        if let Some(parent_id) = &task.parent_id
            && removed_ids.contains(parent_id)
        {
            references.push(format!("{} parent_id {}", task.id, parent_id));
        }
    }
    if !references.is_empty() {
        let sample = references
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "Cannot replace the existing child subtree because other tasks still reference it: {}{}",
            sample,
            if references.len() > 5 {
                format!(" (and {} more)", references.len() - 5)
            } else {
                String::new()
            }
        );
    }
    Ok(())
}

fn validate_specs(specs: &[MaterializedTaskSpec]) -> Result<()> {
    let mut keys = HashSet::with_capacity(specs.len());
    for spec in specs {
        let key = normalize_required(spec.local_key.as_str(), "task local_key")?;
        if !keys.insert(key.to_string()) {
            bail!("duplicate task local_key: {key}");
        }

        normalize_required(spec.title.as_str(), "task title")?;
        if let Some(description) = &spec.description {
            normalize_required(description.as_str(), "task description")?;
        }

        if spec.parent_local_key.is_some() && spec.parent_task_id.is_some() {
            bail!("task local_key {key} cannot set both parent_local_key and parent_task_id");
        }
    }

    for spec in specs {
        let key = normalize_required(spec.local_key.as_str(), "task local_key")?;
        if let Some(parent_local_key) = &spec.parent_local_key {
            let parent_local_key = normalize_required(parent_local_key, "parent_local_key")?;
            if parent_local_key == key {
                bail!("task local_key {key} cannot use itself as parent_local_key");
            }
            if !keys.contains(parent_local_key) {
                bail!("unknown parent local key: {parent_local_key}");
            }
        }
        if let Some(parent_task_id) = &spec.parent_task_id {
            normalize_required(parent_task_id, "parent_task_id")?;
        }

        for dependency_key in &spec.depends_on_local_keys {
            let dependency_key = normalize_required(dependency_key, "depends_on local key")?;
            if dependency_key == key {
                bail!("task local_key {key} depends on itself");
            }
            if !keys.contains(dependency_key) {
                bail!("unknown local dependency key: {dependency_key}");
            }
        }
    }

    Ok(())
}

fn allocate_ids(
    active: &QueueFile,
    done: Option<&QueueFile>,
    specs: &[MaterializedTaskSpec],
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<HashMap<String, String>> {
    let mut local_key_to_id = HashMap::with_capacity(specs.len());
    if specs.is_empty() {
        return Ok(local_key_to_id);
    }

    let first_id = queue::next_id_across(active, done, id_prefix, id_width, max_dependency_depth)?;
    let first_number = id_number(first_id.as_str(), id_prefix)?;
    let prefix = queue::normalize_prefix(id_prefix);

    for (offset, spec) in specs.iter().enumerate() {
        local_key_to_id.insert(
            normalize_required(spec.local_key.as_str(), "task local_key")?.to_string(),
            queue::format_id(&prefix, first_number + offset as u32, id_width),
        );
    }

    Ok(local_key_to_id)
}

fn materialize_tasks(
    specs: &[MaterializedTaskSpec],
    local_key_to_id: &HashMap<String, String>,
    now_rfc3339: &str,
) -> Result<Vec<Task>> {
    let now = normalize_required(now_rfc3339, "now_rfc3339")?;
    specs
        .iter()
        .map(|spec| {
            let local_key = normalize_required(spec.local_key.as_str(), "task local_key")?;
            let id = local_key_to_id
                .get(local_key)
                .cloned()
                .ok_or_else(|| anyhow!("missing allocated task id for local key {local_key}"))?;
            let parent_id = match (&spec.parent_local_key, &spec.parent_task_id) {
                (Some(parent_local_key), None) => Some(
                    local_key_to_id
                        .get(normalize_required(parent_local_key, "parent_local_key")?)
                        .cloned()
                        .ok_or_else(|| {
                            anyhow!(
                                "missing allocated parent task id for local key {parent_local_key}"
                            )
                        })?,
                ),
                (None, Some(parent_task_id)) => {
                    Some(normalize_required(parent_task_id, "parent_task_id")?.to_string())
                }
                (None, None) => None,
                (Some(_), Some(_)) => unreachable!("validated earlier"),
            };
            let depends_on =
                spec.depends_on_local_keys
                    .iter()
                    .map(|dependency_key| {
                        let dependency_key =
                            normalize_required(dependency_key, "depends_on local key")?;
                        local_key_to_id.get(dependency_key).cloned().ok_or_else(|| {
                            anyhow!("unknown local dependency key: {dependency_key}")
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

            Ok(Task {
                id,
                status: spec.status,
                title: normalize_required(spec.title.as_str(), "task title")?.to_string(),
                description: spec.description.clone(),
                priority: spec.priority,
                tags: spec.tags.clone(),
                scope: spec.scope.clone(),
                evidence: spec.evidence.clone(),
                plan: spec.plan.clone(),
                notes: spec.notes.clone(),
                request: spec.request.clone(),
                created_at: Some(now.to_string()),
                updated_at: Some(now.to_string()),
                depends_on,
                relates_to: spec.relates_to.clone(),
                parent_id,
                estimated_minutes: spec.estimated_minutes,
                ..Task::default()
            })
        })
        .collect()
}

fn prepare_preview_for_insertion(
    preview: &mut QueueFile,
    done: Option<&QueueFile>,
    insertion: &MaterializeInsertion,
) -> Result<usize> {
    match insertion {
        MaterializeInsertion::QueueDefaultTop => Ok(queue::suggest_new_task_insert_index(preview)),
        MaterializeInsertion::AfterParent { parent_task_id } => {
            parent_insert_index(preview, parent_task_id)
        }
        MaterializeInsertion::AppendUnderParent {
            parent_task_id,
            existing_subtree_task_ids,
        } => append_under_parent_index(preview, parent_task_id, existing_subtree_task_ids),
        MaterializeInsertion::ReplaceSubtree {
            parent_task_id,
            removed_subtree_task_ids,
        } => {
            let removed_ids = removed_subtree_task_ids
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            ensure_subtree_is_replaceable(preview, done, &removed_ids)?;
            preview.tasks.retain(|task| !removed_ids.contains(&task.id));
            parent_insert_index(preview, parent_task_id)
        }
    }
}

fn parent_insert_index(queue_file: &QueueFile, parent_task_id: &str) -> Result<usize> {
    let parent_task_id = normalize_required(parent_task_id, "parent_task_id")?;
    queue_file
        .tasks
        .iter()
        .position(|task| task.id == parent_task_id)
        .map(|index| index + 1)
        .with_context(|| crate::error_messages::source_task_not_found(parent_task_id, false))
}

fn append_under_parent_index(
    queue_file: &QueueFile,
    parent_task_id: &str,
    existing_subtree_task_ids: &[String],
) -> Result<usize> {
    let parent_task_id = normalize_required(parent_task_id, "parent_task_id")?;
    let parent_index = queue_file
        .tasks
        .iter()
        .position(|task| task.id == parent_task_id)
        .with_context(|| crate::error_messages::source_task_not_found(parent_task_id, false))?;
    if existing_subtree_task_ids.is_empty() {
        return Ok(parent_index + 1);
    }

    let subtree_ids = existing_subtree_task_ids.iter().collect::<HashSet<_>>();
    let mut max_index = parent_index;
    for (index, task) in queue_file.tasks.iter().enumerate() {
        if subtree_ids.contains(&task.id) && index > max_index {
            max_index = index;
        }
    }
    Ok(max_index + 1)
}

fn id_number(id: &str, id_prefix: &str) -> Result<u32> {
    let prefix = queue::normalize_prefix(id_prefix);
    let expected = format!("{prefix}-");
    let suffix = id
        .trim()
        .strip_prefix(expected.as_str())
        .ok_or_else(|| anyhow!("allocated task id {} does not use prefix {}", id, prefix))?;
    suffix
        .parse::<u32>()
        .with_context(|| format!("parse allocated task id number from {id}"))
}

fn normalize_required<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{label} must be non-empty");
    }
    Ok(trimmed)
}
