//! Refactoring task generation for `ralph task refactor` subcommand.
//!
//! Purpose:
//! - Refactoring task generation for `ralph task refactor` subcommand.
//!
//! Responsibilities:
//! - Handle `refactor` and `build-refactor` commands.
//! - Scan repository for large files exceeding threshold.
//! - Generate refactoring tasks for flagged files.
//!
//! Not handled here:
//! - Task building from natural language (see `build.rs`).
//! - Template-based task creation (see `template.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Threshold defaults to 1000 LOC per AGENTS.md guidelines.
//! - Supports batching modes for grouping related files.
//! - Dry-run mode previews tasks without inserting into queue.

use anyhow::Result;

use crate::agent;
use crate::cli::task::args::TaskBuildRefactorArgs;
use crate::commands::task as task_cmd;
use crate::config;

/// Handle the `refactor` and `build-refactor` commands.
pub fn handle(
    args: &TaskBuildRefactorArgs,
    force: bool,
    resolved: &config::Resolved,
) -> Result<()> {
    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
        runner_cli: args.runner_cli.clone(),
    })?;

    task_cmd::build_refactor_tasks(
        resolved,
        task_cmd::TaskBuildRefactorOptions {
            threshold: args.threshold,
            path: args.path.clone(),
            dry_run: args.dry_run,
            batch: args.batch.into(),
            extra_tags: args.tags.clone().unwrap_or_default(),
            runner_override: overrides.runner,
            model_override: overrides.model,
            reasoning_effort_override: overrides.reasoning_effort,
            runner_cli_overrides: overrides.runner_cli,
            force,
            repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, resolved),
        },
    )
}
