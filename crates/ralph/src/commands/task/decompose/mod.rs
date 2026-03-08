//! Task decomposition planning and queue materialization helpers.
//!
//! Responsibilities:
//! - Resolve decomposition sources and attach targets.
//! - Run the planner prompt and turn JSON into durable queue tasks.
//! - Enforce child-policy and sibling-dependency semantics for preview/write flows.
//!
//! Not handled here:
//! - CLI parsing or terminal formatting details.
//! - Direct queue editing by runners.
//!
//! Invariants/assumptions:
//! - Preview stays side-effect free with respect to queue/done files.
//! - Write mode re-checks queue state under lock before mutating.

mod support;
#[cfg(test)]
mod tests;
mod tree;

use super::resolve_task_runner_settings;
use crate::commands::run::PhaseType;
use crate::contracts::{Model, ProjectType, QueueFile, ReasoningEffort, Runner, Task, TaskStatus};
use crate::{config, prompts, queue, runner, runutil, timeutil};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use support::{
    allocate_sequential_ids, annotate_parent, created_node_count, descendant_ids_for_parent,
    done_queue_ref, ensure_subtree_is_replaceable, insertion_index, kind_for_source,
    looks_like_task_id, materialize_children, materialize_node, request_context,
};
use tree::normalize_response;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecompositionChildPolicy {
    Fail,
    Append,
    Replace,
}

#[derive(Debug, Clone)]
pub struct TaskDecomposeOptions {
    pub source_input: String,
    pub attach_to_task_id: Option<String>,
    pub max_depth: u8,
    pub max_children: usize,
    pub max_nodes: usize,
    pub status: TaskStatus,
    pub child_policy: DecompositionChildPolicy,
    pub with_dependencies: bool,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch,
    pub repoprompt_tool_injection: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecompositionSource {
    Freeform { request: String },
    ExistingTask { task: Box<Task> },
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompositionAttachTarget {
    pub task: Box<Task>,
    pub has_existing_children: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompositionPreview {
    pub source: DecompositionSource,
    pub attach_target: Option<DecompositionAttachTarget>,
    pub plan: DecompositionPlan,
    pub write_blockers: Vec<String>,
    pub child_status: TaskStatus,
    pub child_policy: DecompositionChildPolicy,
    pub with_dependencies: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompositionPlan {
    pub root: PlannedNode,
    pub warnings: Vec<String>,
    pub total_nodes: usize,
    pub leaf_nodes: usize,
    pub dependency_edges: Vec<DependencyEdgePreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyEdgePreview {
    pub task_title: String,
    pub depends_on_title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDecomposeWriteResult {
    pub root_task_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub created_ids: Vec<String>,
    pub replaced_ids: Vec<String>,
    pub parent_annotated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDecompositionResponse {
    #[serde(default)]
    warnings: Vec<String>,
    tree: RawPlannedNode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlannedNode {
    #[serde(default)]
    key: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    plan: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    scope: Vec<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    children: Vec<RawPlannedNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedNode {
    pub planner_key: String,
    pub title: String,
    pub description: Option<String>,
    pub plan: Vec<String>,
    pub tags: Vec<String>,
    pub scope: Vec<String>,
    pub depends_on_keys: Vec<String>,
    pub children: Vec<PlannedNode>,
    #[serde(skip_serializing)]
    dependency_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Freeform,
    ExistingTask,
}

struct PlannerState {
    remaining_nodes: usize,
    warnings: Vec<String>,
    with_dependencies: bool,
}

pub fn plan_task_decomposition(
    resolved: &config::Resolved,
    opts: &TaskDecomposeOptions,
) -> Result<DecompositionPreview> {
    let (active, done) = queue::load_and_validate_queues(resolved, true)?;
    let source = resolve_source(resolved, &active, done.as_ref(), opts.source_input.trim())?;
    let attach_target = resolve_attach_target(
        resolved,
        &active,
        done.as_ref(),
        opts.attach_to_task_id.as_deref(),
        &source,
    )?;

    let template = prompts::load_task_decompose_prompt(&resolved.repo_root)?;
    let prompt = build_planner_prompt(resolved, opts, &source, attach_target.as_ref(), &template)?;
    let settings = resolve_task_runner_settings(
        resolved,
        opts.runner_override.clone(),
        opts.model_override.clone(),
        opts.reasoning_effort_override,
        &opts.runner_cli_overrides,
    )?;
    let bins = runner::resolve_binaries(&resolved.config.agent);
    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    let output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: settings.runner,
            bins,
            model: settings.model,
            reasoning_effort: settings.reasoning_effort,
            runner_cli: settings.runner_cli,
            prompt: &prompt,
            timeout: None,
            permission_mode: settings.permission_mode,
            revert_on_error: false,
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(crate::contracts::GitRevertMode::Ask),
            output_handler: None,
            output_stream: runner::OutputStream::Terminal,
            revert_prompt: None,
            phase_type: PhaseType::SinglePhase,
            session_id: None,
            retry_policy,
        },
        runutil::RunnerErrorMessages {
            log_label: "task decompose planner",
            interrupted_msg: "Task decomposition interrupted: the planner run was canceled.",
            timeout_msg: "Task decomposition timed out before a plan was returned.",
            terminated_msg: "Task decomposition terminated: the planner was stopped by a signal.",
            non_zero_msg: |code| {
                format!(
                    "Task decomposition failed: the planner exited with a non-zero code ({code})."
                )
            },
            other_msg: |err| {
                format!(
                    "Task decomposition failed: the planner could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    let planner_text = extract_planner_text(&output.stdout).context(
        "Task decomposition planner did not produce a final assistant response containing JSON.",
    )?;
    let raw = parse_planner_response(&planner_text)?;
    let default_root_title = match &source {
        DecompositionSource::Freeform { request } => request.clone(),
        DecompositionSource::ExistingTask { task } => task.title.clone(),
    };
    let plan = normalize_response(raw, kind_for_source(&source), opts, &default_root_title)?;
    let write_blockers = compute_write_blockers(
        &active,
        done.as_ref(),
        &source,
        attach_target.as_ref(),
        opts.child_policy,
    )?;

    Ok(DecompositionPreview {
        source,
        attach_target,
        plan,
        write_blockers,
        child_status: opts.status,
        child_policy: opts.child_policy,
        with_dependencies: opts.with_dependencies,
    })
}

pub fn write_task_decomposition(
    resolved: &config::Resolved,
    preview: &DecompositionPreview,
    force: bool,
) -> Result<TaskDecomposeWriteResult> {
    if !preview.write_blockers.is_empty() {
        bail!(preview.write_blockers.join("\n"));
    }

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task decompose", force)?;
    let mut active = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = done_queue_ref(&done, &resolved.done_path);
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before task decompose write")?;

    let effective_parent = resolve_effective_parent_for_write(&active, done_ref, preview)?;
    let existing_descendant_ids = effective_parent
        .as_ref()
        .map(|task| descendant_ids_for_parent(&active, task.id.as_str()))
        .transpose()?
        .unwrap_or_default();

    match preview.child_policy {
        DecompositionChildPolicy::Fail => {
            if !existing_descendant_ids.is_empty() {
                let parent_id = effective_parent
                    .as_ref()
                    .map(|task| task.id.as_str())
                    .unwrap_or("");
                bail!(
                    "Task {} already has child tasks. Refusing write for `ralph task decompose --child-policy fail`.",
                    parent_id
                );
            }
        }
        DecompositionChildPolicy::Replace => {
            if !existing_descendant_ids.is_empty() {
                ensure_subtree_is_replaceable(&active, done_ref, &existing_descendant_ids)?;
            }
        }
        DecompositionChildPolicy::Append => {}
    }

    crate::undo::create_undo_snapshot(
        resolved,
        &match (&preview.source, preview.attach_target.as_ref()) {
            (DecompositionSource::Freeform { request }, None) => {
                format!("task decompose write for request '{request}'")
            }
            (DecompositionSource::Freeform { request }, Some(parent)) => {
                format!(
                    "task decompose attach request '{}' under {}",
                    request, parent.task.id
                )
            }
            (DecompositionSource::ExistingTask { task }, None) => {
                format!("task decompose {} into child tasks", task.id)
            }
            (DecompositionSource::ExistingTask { task }, Some(parent)) => {
                format!(
                    "task decompose {} attached under {}",
                    task.id, parent.task.id
                )
            }
        },
    )?;

    let created_count = created_node_count(preview);
    if created_count == 0 {
        bail!("Task decomposition produced no child tasks to write.");
    }

    let ids = allocate_sequential_ids(
        &active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
        created_count,
    )?;
    let now = timeutil::now_utc_rfc3339()?;
    let request_context = request_context(preview);
    let mut next_id_index = 0usize;

    let mut created_tasks = match (&preview.source, effective_parent.as_ref()) {
        (DecompositionSource::ExistingTask { task: _ }, Some(parent))
            if preview.attach_target.is_none() =>
        {
            materialize_children(
                &preview.plan.root.children,
                Some(parent.id.as_str()),
                &ids,
                &mut next_id_index,
                preview.child_status,
                &request_context,
                &now,
            )?
        }
        (_, Some(parent)) => {
            let root_task = materialize_node(
                &preview.plan.root,
                Some(parent.id.as_str()),
                &ids,
                &mut next_id_index,
                preview.child_status,
                &request_context,
                &now,
            )?;
            let root_id = root_task.id.clone();
            let mut tasks = vec![root_task];
            tasks.extend(materialize_children(
                &preview.plan.root.children,
                Some(root_id.as_str()),
                &ids,
                &mut next_id_index,
                preview.child_status,
                &request_context,
                &now,
            )?);
            tasks
        }
        (_, None) => {
            let root_task = materialize_node(
                &preview.plan.root,
                None,
                &ids,
                &mut next_id_index,
                preview.child_status,
                &request_context,
                &now,
            )?;
            let root_id = root_task.id.clone();
            let mut tasks = vec![root_task];
            tasks.extend(materialize_children(
                &preview.plan.root.children,
                Some(root_id.as_str()),
                &ids,
                &mut next_id_index,
                preview.child_status,
                &request_context,
                &now,
            )?);
            tasks
        }
    };

    let root_task_id = match (&preview.source, preview.attach_target.as_ref()) {
        (DecompositionSource::ExistingTask { .. }, None) => None,
        _ => created_tasks.first().map(|task| task.id.clone()),
    };
    let parent_task_id = effective_parent.as_ref().map(|task| task.id.clone());
    let created_ids = created_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let replaced_ids = if preview.child_policy == DecompositionChildPolicy::Replace {
        existing_descendant_ids.iter().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let removed_ids = existing_descendant_ids;
    if !removed_ids.is_empty() && preview.child_policy == DecompositionChildPolicy::Replace {
        active
            .tasks
            .retain(|task| !removed_ids.contains(task.id.as_str()));
    }

    let insert_at = insertion_index(
        &active,
        effective_parent.as_ref(),
        &removed_ids,
        preview.child_policy,
    )?;

    if let Some(parent) = effective_parent {
        annotate_parent(
            &mut active,
            &parent.id,
            &preview.source,
            preview.attach_target.as_ref(),
            &created_tasks,
            &now,
        )?;
    }

    for (offset, task) in created_tasks.drain(..).enumerate() {
        active.tasks.insert(insert_at + offset, task);
    }

    queue::validate_queue_set(
        &active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after task decompose write")?;
    queue::save_queue(&resolved.queue_path, &active)?;

    Ok(TaskDecomposeWriteResult {
        root_task_id,
        parent_task_id,
        created_ids,
        replaced_ids,
        parent_annotated: preview.attach_target.is_some()
            || matches!(preview.source, DecompositionSource::ExistingTask { .. }),
    })
}

fn resolve_source(
    resolved: &config::Resolved,
    active: &QueueFile,
    done: Option<&QueueFile>,
    source_input: &str,
) -> Result<DecompositionSource> {
    if source_input.is_empty() {
        bail!("Missing source: task decompose requires a task ID or freeform request.");
    }
    if looks_like_task_id(source_input, &resolved.id_prefix, resolved.id_width) {
        let task = queue::operations::find_task_across(active, done, source_input)
            .with_context(|| format!("Unknown task ID '{source_input}' for task decomposition."))?;
        if done.is_some_and(|done_file| {
            queue::operations::find_task(done_file, source_input).is_some()
        }) {
            bail!(
                "Task {} is in the done archive. `ralph task decompose` only supports active tasks unless explicitly overridden.",
                source_input
            );
        }
        ensure_existing_task_is_supported(task)?;
        return Ok(DecompositionSource::ExistingTask {
            task: Box::new(task.clone()),
        });
    }

    Ok(DecompositionSource::Freeform {
        request: source_input.to_string(),
    })
}

fn resolve_attach_target(
    resolved: &config::Resolved,
    active: &QueueFile,
    done: Option<&QueueFile>,
    attach_to: Option<&str>,
    source: &DecompositionSource,
) -> Result<Option<DecompositionAttachTarget>> {
    let Some(attach_to) = attach_to.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if !looks_like_task_id(attach_to, &resolved.id_prefix, resolved.id_width) {
        bail!(
            "Invalid attach target '{}': expected a task ID like {}-0001.",
            attach_to,
            queue::normalize_prefix(&resolved.id_prefix)
        );
    }
    if matches!(source, DecompositionSource::ExistingTask { .. }) {
        bail!(
            "`ralph task decompose --attach-to` only supports freeform request sources. Use either an existing task source or --attach-to, not both."
        );
    }
    let task = queue::operations::find_task_across(active, done, attach_to)
        .with_context(|| format!("Unknown attach target '{attach_to}' for task decomposition."))?;
    if done.is_some_and(|done_file| queue::operations::find_task(done_file, attach_to).is_some()) {
        bail!(
            "Task {} is in the done archive and cannot be used as an attach target.",
            attach_to
        );
    }
    ensure_existing_task_is_supported(task)?;
    let hierarchy = queue::hierarchy::HierarchyIndex::build(active, done);
    Ok(Some(DecompositionAttachTarget {
        task: Box::new(task.clone()),
        has_existing_children: !hierarchy.children_of(&task.id).is_empty(),
    }))
}

fn resolve_effective_parent_for_write(
    active: &QueueFile,
    done: Option<&QueueFile>,
    preview: &DecompositionPreview,
) -> Result<Option<Task>> {
    if let Some(attach_target) = &preview.attach_target {
        let task =
            queue::operations::find_task(active, &attach_target.task.id).with_context(|| {
                crate::error_messages::source_task_not_found(&attach_target.task.id, false)
            })?;
        ensure_existing_task_is_supported(task)?;
        return Ok(Some(task.clone()));
    }
    match &preview.source {
        DecompositionSource::Freeform { .. } => Ok(None),
        DecompositionSource::ExistingTask { task } => {
            let active_task = queue::operations::find_task(active, &task.id)
                .with_context(|| crate::error_messages::source_task_not_found(&task.id, false))?;
            if done.is_some_and(|done_file| {
                queue::operations::find_task(done_file, &task.id).is_some()
            }) {
                bail!(
                    "Task {} is in the done archive and cannot be decomposed in-place.",
                    task.id
                );
            }
            ensure_existing_task_is_supported(active_task)?;
            Ok(Some(active_task.clone()))
        }
    }
}

fn ensure_existing_task_is_supported(task: &Task) -> Result<()> {
    if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
        bail!(
            "Task {} has terminal status {} and cannot be decomposed without an explicit override.",
            task.id,
            task.status
        );
    }
    Ok(())
}

fn compute_write_blockers(
    active: &QueueFile,
    done: Option<&QueueFile>,
    source: &DecompositionSource,
    attach_target: Option<&DecompositionAttachTarget>,
    child_policy: DecompositionChildPolicy,
) -> Result<Vec<String>> {
    let mut write_blockers = Vec::new();
    let effective_parent_id = attach_target
        .map(|target| target.task.id.clone())
        .or_else(|| match source {
            DecompositionSource::ExistingTask { task } => Some(task.id.clone()),
            DecompositionSource::Freeform { .. } => None,
        });

    if let Some(parent_id) = effective_parent_id {
        let descendant_ids = descendant_ids_for_parent(active, parent_id.as_str())?;
        let has_existing_children = !descendant_ids.is_empty();
        match child_policy {
            DecompositionChildPolicy::Fail if has_existing_children => {
                write_blockers.push(format!(
                    "Write blocked: task {} already has child tasks and --child-policy is set to fail.",
                    parent_id
                ));
            }
            DecompositionChildPolicy::Replace if has_existing_children => {
                if let Err(err) = ensure_subtree_is_replaceable(active, done, &descendant_ids) {
                    write_blockers.push(err.to_string());
                }
            }
            _ => {}
        }
    }
    Ok(write_blockers)
}

fn build_planner_prompt(
    resolved: &config::Resolved,
    opts: &TaskDecomposeOptions,
    source: &DecompositionSource,
    attach_target: Option<&DecompositionAttachTarget>,
    template: &str,
) -> Result<String> {
    let (source_mode, source_request, source_task_json) = match source {
        DecompositionSource::Freeform { request } => ("freeform", request.clone(), String::new()),
        DecompositionSource::ExistingTask { task } => (
            "existing_task",
            task.request.clone().unwrap_or_else(|| task.title.clone()),
            serde_json::to_string_pretty(task)
                .context("serialize source task for decomposition")?,
        ),
    };
    let attach_target_json = attach_target
        .map(|target| {
            serde_json::to_string_pretty(&target.task)
                .context("serialize attach target for decomposition")
        })
        .transpose()?
        .unwrap_or_default();
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut prompt = prompts::render_task_decompose_prompt(
        template,
        source_mode,
        &source_request,
        &source_task_json,
        &attach_target_json,
        opts.max_depth,
        opts.max_children,
        opts.max_nodes,
        opts.child_policy,
        opts.with_dependencies,
        project_type,
        &resolved.config,
    )?;
    prompt = prompts::wrap_with_repoprompt_requirement(&prompt, opts.repoprompt_tool_injection);
    prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)
}

fn extract_planner_text(stdout: &str) -> Option<String> {
    runner::extract_final_assistant_response(stdout).or_else(|| {
        let trimmed = stdout.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn parse_planner_response(raw_text: &str) -> Result<RawDecompositionResponse> {
    let stripped = strip_code_fences(raw_text.trim());
    serde_json::from_str::<RawDecompositionResponse>(stripped)
        .or_else(|_| match extract_json_object(stripped) {
            Some(candidate) => serde_json::from_str::<RawDecompositionResponse>(&candidate),
            None => Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no JSON object found in planner response",
            ))),
        })
        .context("parse task decomposition planner JSON")
}

fn strip_code_fences(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("```")
        && let Some(end) = inner.rfind("```")
    {
        let body = &inner[..end];
        if let Some(after_language) = body.find('\n') {
            return body[after_language + 1..].trim();
        }
        return body.trim();
    }
    trimmed
}

fn extract_json_object(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (start < end).then(|| raw[start..=end].to_string())
}
