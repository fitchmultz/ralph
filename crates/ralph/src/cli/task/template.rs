//! Task template command handlers for `ralph task template` subcommand.
//!
//! Purpose:
//! - Task template command handlers for `ralph task template` subcommand.
//!
//! Responsibilities:
//! - Handle `template list` command.
//! - Handle `template show` command.
//! - Handle `template build` command.
//!
//! Not handled here:
//! - Interactive template selection (see `build.rs`).
//! - Task building without templates (see `build.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Templates can be built-in or custom (from .ralph/templates/).
//! - Template build merges template defaults with user-provided values.

use anyhow::Result;

use crate::agent;
use crate::cli::task::args::{TaskTemplateArgs, TaskTemplateCommand};
use crate::commands::task as task_cmd;
use crate::config;

/// Handle template subcommands.
pub fn handle(resolved: &config::Resolved, args: &TaskTemplateArgs) -> Result<()> {
    use crate::template::{list_templates, load_template};

    match &args.command {
        TaskTemplateCommand::List => {
            let templates = list_templates(&resolved.repo_root);
            println!("Available task templates:");
            println!();
            for template in templates {
                let source_label = match template.source {
                    crate::template::TemplateSource::Custom(_) => "(custom)",
                    crate::template::TemplateSource::Builtin(_) => "(built-in)",
                };
                println!(
                    "  {:12} {:10} {}",
                    template.name, source_label, template.description
                );
            }
            println!();
            println!("Use 'ralph task template show <name>' to view template details.");
            println!("Use 'ralph task template build <name> \"request\"' to create from template.");
            Ok(())
        }
        TaskTemplateCommand::Show(show_args) => {
            let (task, source) = load_template(&show_args.name, &resolved.repo_root)?;

            let source_label = match source {
                crate::template::TemplateSource::Custom(path) => {
                    format!("custom ({})", path.display())
                }
                crate::template::TemplateSource::Builtin(_) => "built-in".to_string(),
            };

            println!("Template: {} ({})", show_args.name, source_label);
            println!();

            if !task.tags.is_empty() {
                println!("Tags: {}", task.tags.join(", "));
            }
            if !task.scope.is_empty() {
                println!("Scope: {}", task.scope.join(", "));
            }
            println!("Priority: {}", task.priority);
            println!("Status: {}", task.status);

            if !task.plan.is_empty() {
                println!();
                println!("Plan:");
                for (i, step) in task.plan.iter().enumerate() {
                    println!("  {}. {}", i + 1, step);
                }
            }

            if !task.evidence.is_empty() {
                println!();
                println!("Evidence: {}", task.evidence.join(", "));
            }

            Ok(())
        }
        TaskTemplateCommand::Build(build_args) => {
            let request = task_cmd::read_request_from_args_or_stdin(&build_args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: build_args.runner.clone(),
                model: build_args.model.clone(),
                effort: build_args.effort.clone(),
                repo_prompt: build_args.repo_prompt,
                runner_cli: build_args.runner_cli.clone(),
            })?;

            // Merge template tags and scope with user-provided values
            let hint_tags = build_args.tags.clone().unwrap_or_default();
            let hint_scope = build_args.scope.clone().unwrap_or_default();

            task_cmd::build_task(
                resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags,
                    hint_scope,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force: false,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        build_args.repo_prompt,
                        resolved,
                    ),
                    output: task_cmd::TaskBuildOutputTarget::Terminal,
                    template_hint: Some(build_args.template.clone()),
                    template_target: build_args.target.clone(),
                    strict_templates: build_args.strict_templates,
                    estimated_minutes: None,
                },
            )
        }
    }
}
