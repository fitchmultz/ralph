//! Task-oriented machine command handlers.
//!
//! Responsibilities:
//! - Implement `ralph machine task ...` operations.
//! - Parse machine task-create/mutate/decompose inputs and emit versioned JSON documents.
//! - Keep machine task writes aligned with queue locking and undo semantics.
//!
//! Not handled here:
//! - Queue read/graph/dashboard commands.
//! - Machine run event streaming.
//! - Clap argument definitions or top-level routing.
//!
//! Invariants/assumptions:
//! - Machine task requests stay versioned and JSON-only.
//! - Task writes preserve queue locking, undo snapshots, and validation behavior.
//! - Status and child-policy parsing remain strict.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};

use crate::agent;
use crate::cli::machine::args::{MachineTaskArgs, MachineTaskCommand};
use crate::cli::machine::common::{done_queue_ref, queue_max_dependency_depth};
use crate::cli::machine::io::{print_json, read_json_input};
use crate::commands::task as task_cmd;
use crate::config;
use crate::contracts::{
    MACHINE_DECOMPOSE_VERSION, MACHINE_TASK_CREATE_VERSION, MACHINE_TASK_MUTATION_VERSION,
    MachineDecomposeDocument, MachineTaskCreateDocument, MachineTaskCreateRequest,
    MachineTaskMutationDocument, RunnerCliOptionsPatch, Task, TaskStatus,
};
use crate::queue;
use crate::timeutil;

pub(super) fn handle_task(args: MachineTaskArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match args.command {
        MachineTaskCommand::Create(args) => {
            let raw = read_json_input(args.input.as_deref())?;
            let request: MachineTaskCreateRequest =
                serde_json::from_str(&raw).context("parse machine task create request")?;
            let task = create_task(&resolved, &request, force)?;
            print_json(&MachineTaskCreateDocument {
                version: MACHINE_TASK_CREATE_VERSION,
                task,
            })
        }
        MachineTaskCommand::Mutate(args) => {
            let raw = read_json_input(args.input.as_deref())?;
            let request = serde_json::from_str::<queue::operations::TaskMutationRequest>(&raw)
                .context("parse machine task mutation request")?;

            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "machine task mutate", force)?;
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done_file = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = done_queue_ref(&done_file, &resolved.done_path);
            let now = timeutil::now_utc_rfc3339()?;
            let mut working = queue_file.clone();
            let report = queue::operations::apply_task_mutation_request(
                &mut working,
                done_ref,
                &request,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                queue_max_dependency_depth(&resolved),
            )?;
            if !args.dry_run {
                crate::undo::create_undo_snapshot(
                    &resolved,
                    &format!("machine task mutate [{} task(s)]", report.tasks.len()),
                )?;
                queue::save_queue(&resolved.queue_path, &working)?;
            }
            print_json(&MachineTaskMutationDocument {
                version: MACHINE_TASK_MUTATION_VERSION,
                report: serde_json::to_value(report)?,
            })
        }
        MachineTaskCommand::Decompose(args) => {
            let source_input = task_cmd::read_request_from_args_or_stdin(&args.source)?;
            let overrides = agent::resolve_agent_overrides(&args.agent)?;
            let preview = task_cmd::plan_task_decomposition(
                &resolved,
                &task_cmd::TaskDecomposeOptions {
                    source_input,
                    attach_to_task_id: args.attach_to,
                    max_depth: args.max_depth,
                    max_children: usize::from(args.max_children),
                    max_nodes: usize::from(args.max_nodes),
                    status: parse_task_status(&args.status)?,
                    child_policy: parse_child_policy(&args.child_policy)?,
                    with_dependencies: args.with_dependencies,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.agent.repo_prompt,
                        &resolved,
                    ),
                },
            )?;
            let write = if args.write {
                Some(task_cmd::write_task_decomposition(
                    &resolved, &preview, force,
                )?)
            } else {
                None
            };
            print_json(&MachineDecomposeDocument {
                version: MACHINE_DECOMPOSE_VERSION,
                result: serde_json::json!({
                    "version": 1,
                    "mode": if write.is_some() { "write" } else { "preview" },
                    "preview": preview,
                    "write": write,
                }),
            })
        }
    }
}

fn create_task(
    resolved: &config::Resolved,
    request: &MachineTaskCreateRequest,
    force: bool,
) -> Result<Task> {
    if request.version != MACHINE_TASK_CREATE_VERSION {
        bail!(
            "Unsupported machine task create request version {}",
            request.version
        );
    }
    if request.title.trim().is_empty() {
        bail!("Task title cannot be empty");
    }

    let queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "machine task create", force)?;
    let active = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = done_queue_ref(&done, &resolved.done_path);
    let predicted_id = queue::next_id_across(
        &active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        queue_max_dependency_depth(resolved),
    )?;

    if let Some(template) = &request.template {
        let _loaded = crate::template::load_template_with_context(
            template,
            &resolved.repo_root,
            request.target.as_deref(),
            false,
        )?;
        let options = task_cmd::TaskBuildOptions {
            request: request.title.clone(),
            hint_tags: request.tags.join(","),
            hint_scope: request.scope.join(","),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force,
            repoprompt_tool_injection: false,
            template_hint: Some(template.clone()),
            template_target: request.target.clone(),
            strict_templates: false,
            estimated_minutes: None,
        };
        drop(queue_lock);
        task_cmd::build_task(resolved, options)?;
        let queue_after = queue::load_queue(&resolved.queue_path)?;
        return queue_after
            .tasks
            .into_iter()
            .find(|task| task.id == predicted_id)
            .ok_or_else(|| {
                anyhow!(
                    "Created template task {} not found after write",
                    predicted_id
                )
            });
    }

    let now = timeutil::now_utc_rfc3339()?;
    let priority = request.priority.parse::<crate::contracts::TaskPriority>()?;
    let task = Task {
        id: predicted_id,
        status: TaskStatus::Todo,
        title: request.title.trim().to_string(),
        description: request
            .description
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        priority,
        tags: request.tags.clone(),
        scope: request.scope.clone(),
        evidence: Vec::new(),
        plan: Vec::new(),
        notes: Vec::new(),
        request: None,
        agent: None,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        estimated_minutes: None,
        actual_minutes: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    };

    let mut working = active;
    working.tasks.push(task.clone());
    crate::undo::create_undo_snapshot(resolved, &format!("machine task create [{}]", task.id))?;
    queue::save_queue(&resolved.queue_path, &working)?;
    Ok(task)
}

fn parse_task_status(value: &str) -> Result<TaskStatus> {
    match value.trim().to_ascii_lowercase().as_str() {
        "draft" => Ok(TaskStatus::Draft),
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "done" => Ok(TaskStatus::Done),
        "rejected" => Ok(TaskStatus::Rejected),
        other => bail!("Unsupported task status '{}'", other),
    }
}

fn parse_child_policy(value: &str) -> Result<task_cmd::DecompositionChildPolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fail" => Ok(task_cmd::DecompositionChildPolicy::Fail),
        "append" => Ok(task_cmd::DecompositionChildPolicy::Append),
        "replace" => Ok(task_cmd::DecompositionChildPolicy::Replace),
        other => bail!("Unsupported decomposition child policy '{}'", other),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_child_policy, parse_task_status};
    use crate::commands::task::DecompositionChildPolicy;
    use crate::contracts::TaskStatus;

    #[test]
    fn parse_task_status_accepts_supported_values_case_insensitively() {
        assert_eq!(
            parse_task_status("TODO").expect("todo status"),
            TaskStatus::Todo
        );
        assert_eq!(
            parse_task_status("done").expect("done status"),
            TaskStatus::Done
        );
    }

    #[test]
    fn parse_task_status_rejects_unknown_values() {
        assert!(parse_task_status("later").is_err());
    }

    #[test]
    fn parse_child_policy_accepts_supported_values_case_insensitively() {
        assert_eq!(
            parse_child_policy("Append").expect("append child policy"),
            DecompositionChildPolicy::Append
        );
    }

    #[test]
    fn parse_child_policy_rejects_unknown_values() {
        assert!(parse_child_policy("merge").is_err());
    }
}
