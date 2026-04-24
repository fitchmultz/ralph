//! Task decomposition command handler for `ralph task decompose`.
//!
//! Purpose:
//! - Task decomposition command handler for `ralph task decompose`.
//!
//! Responsibilities:
//! - Read source text from CLI args or stdin.
//! - Resolve runner overrides and delegate planning/materialization to command helpers.
//! - Render deterministic preview and write summaries for users or JSON consumers.
//! - Narrate decomposition as a continuation workflow with explicit next steps from the shared machine document.
//!
//! Not handled here:
//! - Planner prompt rendering or runner execution details.
//! - Queue mutation internals.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview is always shown before any optional write summary.
//! - `--write` is the only mutating mode.

use anyhow::Result;

use crate::agent;
use crate::cli::task::args::{
    TaskDecomposeArgs, TaskDecomposeChildPolicyArg, TaskDecomposeFormatArg,
};
use crate::commands::task as task_cmd;
use crate::config;
use crate::contracts::MachineDecomposeDocument;

pub fn handle(args: &TaskDecomposeArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let source_input = task_cmd::read_request_from_args_or_stdin(&args.source)?;
    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
        runner_cli: args.runner_cli.clone(),
    })?;

    let preview = task_cmd::plan_task_decomposition(
        resolved,
        &task_cmd::TaskDecomposeOptions {
            source_input,
            attach_to_task_id: args.attach_to.clone(),
            max_depth: args.max_depth,
            max_children: usize::from(args.max_children),
            max_nodes: usize::from(args.max_nodes),
            status: args.status.into(),
            child_policy: child_policy(args.child_policy),
            with_dependencies: args.with_dependencies,
            runner_override: overrides.runner,
            model_override: overrides.model,
            reasoning_effort_override: overrides.reasoning_effort,
            runner_cli_overrides: overrides.runner_cli,
            repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, resolved),
        },
    )?;

    let write_result = if args.write {
        Some(task_cmd::write_task_decomposition(
            resolved, &preview, force,
        )?)
    } else {
        None
    };
    let document =
        crate::cli::machine::build_task_decompose_document(&preview, write_result.as_ref());

    match args.format {
        TaskDecomposeFormatArg::Text => {
            print_text_output(&preview, write_result.as_ref(), &document)
        }
        TaskDecomposeFormatArg::Json => println!("{}", serde_json::to_string_pretty(&document)?),
    }

    Ok(())
}

fn child_policy(value: TaskDecomposeChildPolicyArg) -> task_cmd::DecompositionChildPolicy {
    match value {
        TaskDecomposeChildPolicyArg::Fail => task_cmd::DecompositionChildPolicy::Fail,
        TaskDecomposeChildPolicyArg::Append => task_cmd::DecompositionChildPolicy::Append,
        TaskDecomposeChildPolicyArg::Replace => task_cmd::DecompositionChildPolicy::Replace,
    }
}

fn print_text_output(
    preview: &task_cmd::DecompositionPreview,
    write_result: Option<&task_cmd::TaskDecomposeWriteResult>,
    document: &MachineDecomposeDocument,
) {
    match &preview.source {
        task_cmd::DecompositionSource::Freeform { request } => {
            println!("Decompose preview for new request:");
            println!("  {}", request);
        }
        task_cmd::DecompositionSource::ExistingTask { task } => {
            println!("Decompose preview for existing task {}:", task.id);
            println!("  {}", task.title);
        }
    }

    if let Some(attach_target) = &preview.attach_target {
        println!("Attach target:");
        println!("  {}: {}", attach_target.task.id, attach_target.task.title);
        if attach_target.has_existing_children {
            println!(
                "  Existing child tasks detected; policy {:?} will govern write behavior.",
                preview.child_policy
            );
        }
    }

    println!();
    print_node(&preview.plan.root, 0);
    println!();
    println!(
        "Stats: {} node(s), {} leaf node(s).",
        preview.plan.total_nodes, preview.plan.leaf_nodes
    );
    println!(
        "Planner options: child policy {:?}, sibling dependencies {}.",
        preview.child_policy,
        if preview.with_dependencies {
            "enabled"
        } else {
            "disabled"
        }
    );
    if !preview.plan.dependency_edges.is_empty() {
        println!("Dependency edges:");
        for edge in &preview.plan.dependency_edges {
            println!(
                "  - {} depends on {}",
                edge.task_title, edge.depends_on_title
            );
        }
    }
    if !preview.plan.warnings.is_empty() {
        println!("Warnings:");
        for warning in &preview.plan.warnings {
            println!("  - {}", warning);
        }
    }

    if let Some(result) = write_result {
        println!();
        if let Some(root_id) = &result.root_task_id {
            println!("Wrote decomposition rooted at {}.", root_id);
        } else if let Some(parent_id) = &result.parent_task_id {
            println!("Wrote decomposition under existing parent {}.", parent_id);
        }
        println!("Created {} task(s):", result.created_ids.len());
        for id in &result.created_ids {
            println!("  - {}", id);
        }
        if !result.replaced_ids.is_empty() {
            println!(
                "Replaced {} prior child task(s):",
                result.replaced_ids.len()
            );
            for id in &result.replaced_ids {
                println!("  - {}", id);
            }
        }
        if result.parent_annotated
            && let Some(parent_id) = &result.parent_task_id
        {
            println!(
                "Annotated parent task {} with a decomposition note.",
                parent_id
            );
        }
    }

    println!();
    println!("{}", document.continuation.headline);
    println!("{}", document.continuation.detail);
    if let Some(blocking) = document
        .blocking
        .as_ref()
        .or(document.continuation.blocking.as_ref())
    {
        println!();
        println!(
            "Operator state: {}",
            format!("{:?}", blocking.status).to_lowercase()
        );
        println!("{}", blocking.message);
        if !blocking.detail.is_empty() {
            println!("{}", blocking.detail);
        }
    }
    if !document.continuation.next_steps.is_empty() {
        println!();
        println!("Next:");
        for (index, step) in document.continuation.next_steps.iter().enumerate() {
            println!("  {}. {} — {}", index + 1, step.command, step.detail);
        }
    }
}

fn print_node(node: &task_cmd::PlannedNode, depth: usize) {
    let deps = if node.depends_on_keys.is_empty() {
        String::new()
    } else {
        format!(" [depends_on: {}]", node.depends_on_keys.join(", "))
    };
    println!(
        "{}- {} ({}){}",
        "  ".repeat(depth),
        node.title,
        node.planner_key,
        deps
    );
    for child in &node.children {
        print_node(child, depth + 1);
    }
}
