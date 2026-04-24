//! Prompt CLI routing.
//!
//! Purpose:
//! - Prompt CLI routing.
//!
//! Responsibilities:
//! - Resolve config and dispatch `ralph prompt` subcommands to command-layer helpers.
//! - Keep CLI-specific defaults and stdin convenience behavior out of clap types.
//!
//! Not handled here:
//! - Prompt rendering internals.
//! - Template storage mechanics.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - RepoPrompt flag resolution mirrors runtime behavior.

use anyhow::Result;

use crate::{
    agent, commands::prompt as prompt_cmd, commands::task as task_cmd, config, promptflow,
};

use super::args::{PromptArgs, PromptCommand};

pub fn handle_prompt(args: PromptArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        PromptCommand::Worker(prompt_args) => {
            let repoprompt_flags =
                agent::resolve_repoprompt_flags(prompt_args.repo_prompt, &resolved);
            let mode = resolve_worker_mode(&resolved, prompt_args.single, prompt_args.phase);
            let prompt = prompt_cmd::build_worker_prompt(
                &resolved,
                prompt_cmd::WorkerPromptOptions {
                    task_id: prompt_args.task_id,
                    mode,
                    repoprompt_plan_required: repoprompt_flags.plan_required,
                    repoprompt_tool_injection: repoprompt_flags.tool_injection,
                    iterations: prompt_args.iterations,
                    iteration_index: prompt_args.iteration_index,
                    plan_file: prompt_args.plan_file,
                    plan_text: prompt_args.plan_text,
                    explain: prompt_args.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::Scan(prompt_args) => {
            let prompt = prompt_cmd::build_scan_prompt(
                &resolved,
                prompt_cmd::ScanPromptOptions {
                    focus: prompt_args.focus,
                    mode: prompt_args.mode,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        prompt_args.repo_prompt,
                        &resolved,
                    ),
                    explain: prompt_args.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::TaskBuilder(prompt_args) => {
            let request = if let Some(request) = prompt_args.request {
                request
            } else {
                task_cmd::read_request_from_args_or_stdin(&[])?
            };
            let prompt = prompt_cmd::build_task_builder_prompt(
                &resolved,
                prompt_cmd::TaskBuilderPromptOptions {
                    request,
                    hint_tags: prompt_args.tags,
                    hint_scope: prompt_args.scope,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        prompt_args.repo_prompt,
                        &resolved,
                    ),
                    explain: prompt_args.explain,
                },
            )?;
            print!("{prompt}");
        }
        PromptCommand::List => prompt_cmd::list_prompts(&resolved.repo_root)?,
        PromptCommand::Show(prompt_args) => {
            prompt_cmd::show_prompt(&resolved.repo_root, &prompt_args.name, prompt_args.raw)?
        }
        PromptCommand::Export(prompt_args) => prompt_cmd::export_prompts(
            &resolved.repo_root,
            prompt_args.name.as_deref(),
            prompt_args.force,
        )?,
        PromptCommand::Sync(prompt_args) => {
            prompt_cmd::sync_prompts(&resolved.repo_root, prompt_args.dry_run, prompt_args.force)?
        }
        PromptCommand::Diff(prompt_args) => {
            prompt_cmd::diff_prompt(&resolved.repo_root, &prompt_args.name)?
        }
    }

    Ok(())
}

fn resolve_worker_mode(
    resolved: &config::Resolved,
    single: bool,
    phase: Option<promptflow::RunPhase>,
) -> prompt_cmd::WorkerMode {
    if single {
        return prompt_cmd::WorkerMode::Single;
    }

    if let Some(phase) = phase {
        return match phase {
            promptflow::RunPhase::Phase1 => prompt_cmd::WorkerMode::Phase1,
            promptflow::RunPhase::Phase2 => prompt_cmd::WorkerMode::Phase2,
            promptflow::RunPhase::Phase3 => prompt_cmd::WorkerMode::Phase3,
        };
    }

    if resolved.config.agent.phases.unwrap_or(2) > 1 {
        prompt_cmd::WorkerMode::Phase1
    } else {
        prompt_cmd::WorkerMode::Single
    }
}
