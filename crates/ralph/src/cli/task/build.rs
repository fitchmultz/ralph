//! Task building command handler for `ralph task` and `ralph task build`.
//!
//! Responsibilities:
//! - Handle the default task command (when no subcommand is given).
//! - Handle explicit `build` subcommand.
//! - Interactive template selection in TTY mode.
//!
//! Not handled here:
//! - Template management (see `template.rs`).
//! - Task updates or edits (see `edit.rs`).
//!
//! Invariants/assumptions:
//! - Reads request from args or stdin.
//! - Template selection only prompts in TTY mode.

use std::io::IsTerminal;

use anyhow::Result;

use crate::agent;
use crate::cli::task::args::TaskBuildArgs;
use crate::commands::task as task_cmd;
use crate::config;

/// Handle the build command (default when no subcommand given).
pub fn handle(args: &TaskBuildArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;

    // Interactive template selection if no template specified and running in TTY
    let (template_hint, template_target) =
        if args.template.is_none() && std::io::stdin().is_terminal() {
            match prompt_template_selection(&resolved.repo_root)? {
                Some((name, target)) => (Some(name), target),
                None => (args.template.clone(), args.target.clone()),
            }
        } else {
            (args.template.clone(), args.target.clone())
        };

    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
        runner_cli: args.runner_cli.clone(),
    })?;

    task_cmd::build_task(
        resolved,
        task_cmd::TaskBuildOptions {
            request,
            hint_tags: args.tags.clone(),
            hint_scope: args.scope.clone(),
            runner_override: overrides.runner,
            model_override: overrides.model,
            reasoning_effort_override: overrides.reasoning_effort,
            runner_cli_overrides: overrides.runner_cli,
            force,
            repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, resolved),
            template_hint,
            template_target,
        },
    )
}

/// Prompt user to select a template interactively.
///
/// Returns Some((template_name, target_path)) if a template was selected,
/// None if the user chose to skip.
pub fn prompt_template_selection(
    repo_root: &std::path::Path,
) -> Result<Option<(String, Option<String>)>> {
    use std::io::Write;

    let templates = crate::template::list_templates(repo_root);

    println!("\nAvailable templates:");
    println!();
    for (i, template) in templates.iter().enumerate() {
        let source_label = match template.source {
            crate::template::TemplateSource::Custom(_) => "(custom)",
            crate::template::TemplateSource::Builtin(_) => "(built-in)",
        };
        println!(
            "  {}. {:12} {:10} {}",
            i + 1,
            template.name,
            source_label,
            template.description
        );
    }
    println!();
    println!("Enter number to select a template, or press Enter to skip:");
    print!(">> ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        return Ok(None);
    }

    // Parse selection
    match input.parse::<usize>() {
        Ok(num) if num > 0 && num <= templates.len() => {
            let selected = &templates[num - 1];
            let template_name = selected.name.clone();

            // Ask for target if template supports variables
            let needs_target = matches!(
                template_name.as_str(),
                "add-tests"
                    | "refactor-performance"
                    | "fix-error-handling"
                    | "add-docs"
                    | "security-audit"
            );

            if needs_target {
                println!();
                println!("Enter target file/path for template variables (or press Enter to skip):");
                print!(">> ");
                std::io::stdout().flush()?;

                let mut target_input = String::new();
                std::io::stdin().read_line(&mut target_input)?;
                let target = target_input.trim();

                if target.is_empty() {
                    Ok(Some((template_name, None)))
                } else {
                    Ok(Some((template_name, Some(target.to_string()))))
                }
            } else {
                Ok(Some((template_name, None)))
            }
        }
        _ => {
            println!("Invalid selection, skipping template.");
            Ok(None)
        }
    }
}
