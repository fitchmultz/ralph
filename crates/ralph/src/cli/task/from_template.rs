//! Handler for `ralph task from template` command.
//!
//! Purpose:
//! - Handler for `ralph task from template` command.
//!
//! Responsibilities:
//! - Parse template name and variable overrides from CLI arguments.
//! - Load template with variable substitution using the template system.
//! - Delegate to task builder with merged options from template and CLI.
//!
//! Not handled here:
//! - Template loading logic (see `crate::template::loader`).
//! - Task creation logic (see `crate::commands::task::build`).
//! - Variable substitution implementation (see `crate::template::variables`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The `--title` argument is required and becomes the task request.
//! - Variables from `--set` are parsed as VAR=value pairs.
//! - The `target` variable can be set via `--set target=...` and is used for template substitution.

use anyhow::{Result, bail};
use std::collections::HashMap;

use crate::agent;
use crate::cli::task::args::TaskFromTemplateArgs;
use crate::commands::task::{TaskBuildOptions, TaskBuildOutputTarget, build_task};
use crate::config;
use crate::template::load_template_with_context;

/// Handle `ralph task from template` command.
pub fn handle(resolved: &config::Resolved, args: &TaskFromTemplateArgs, force: bool) -> Result<()> {
    // Parse variable overrides from --set flags
    let variables = parse_variable_overrides(&args.variables)?;

    // Build target from variables or use first scope item
    let target = variables.get("target").cloned();

    // Load template with context
    let loaded = load_template_with_context(
        &args.template,
        &resolved.repo_root,
        target.as_deref(),
        args.strict_templates,
    )?;

    // Print any warnings
    for warning in &loaded.warnings {
        log::warn!("Template '{}': {}", args.template, warning);
    }

    // Build the request from title
    let request = args.title.clone();

    // Resolve agent overrides
    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
        runner_cli: args.runner_cli.clone(),
    })?;

    // Merge template tags with user-provided tags
    let hint_tags = args.tags.clone().unwrap_or_default();

    // Build options
    let opts = TaskBuildOptions {
        request,
        hint_tags,
        hint_scope: String::new(),
        runner_override: overrides.runner,
        model_override: overrides.model,
        reasoning_effort_override: overrides.reasoning_effort,
        runner_cli_overrides: overrides.runner_cli,
        force,
        repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, resolved),
        output: TaskBuildOutputTarget::Terminal,
        template_hint: Some(args.template.clone()),
        template_target: target,
        strict_templates: args.strict_templates,
        estimated_minutes: None,
    };

    if args.dry_run {
        // For dry-run, we just show what would be created
        println!("Would create task from template '{}'", args.template);
        println!("Request: {}", opts.request);
        if !args.tags.as_deref().unwrap_or("").is_empty() {
            println!("Additional tags: {}", args.tags.as_deref().unwrap_or(""));
        }
        if let Some(ref t) = opts.template_target {
            println!("Target: {}", t);
        }
        println!("(Dry run - no task created)");
        Ok(())
    } else {
        build_task(resolved, opts)
    }
}

/// Parse variable overrides from --set VAR=value format.
fn parse_variable_overrides(overrides: &[String]) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();
    for override_str in overrides {
        let parts: Vec<&str> = override_str.splitn(2, '=').collect();
        if parts.len() != 2 {
            bail!(
                "Invalid --set format: '{}'. Expected VAR=value (e.g., target=src/main.rs)",
                override_str
            );
        }
        vars.insert(parts[0].to_string(), parts[1].to_string());
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_variable_overrides_single() {
        let overrides = vec!["target=src/main.rs".to_string()];
        let vars = parse_variable_overrides(&overrides).unwrap();
        assert_eq!(vars.get("target"), Some(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_parse_variable_overrides_multiple() {
        let overrides = vec![
            "target=src/main.rs".to_string(),
            "component=auth".to_string(),
        ];
        let vars = parse_variable_overrides(&overrides).unwrap();
        assert_eq!(vars.get("target"), Some(&"src/main.rs".to_string()));
        assert_eq!(vars.get("component"), Some(&"auth".to_string()));
    }

    #[test]
    fn test_parse_variable_overrides_with_equals_in_value() {
        let overrides = vec!["target=src/path=with=equals".to_string()];
        let vars = parse_variable_overrides(&overrides).unwrap();
        assert_eq!(
            vars.get("target"),
            Some(&"src/path=with=equals".to_string())
        );
    }

    #[test]
    fn test_parse_variable_overrides_invalid_format() {
        let overrides = vec!["invalidformat".to_string()];
        let result = parse_variable_overrides(&overrides);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid --set format")
        );
    }

    #[test]
    fn test_parse_variable_overrides_empty() {
        let overrides: Vec<String> = vec![];
        let vars = parse_variable_overrides(&overrides).unwrap();
        assert!(vars.is_empty());
    }
}
