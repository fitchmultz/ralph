use crate::contracts::{Model, ProjectType, QueueFile, ReasoningEffort, Runner};
use crate::{config, gitutil, outpututil, prompts, queue, redaction, runner};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::io::Read;

// TaskBuildOptions controls runner-driven task creation via ralph/prompts/task_builder.md.
pub struct TaskBuildOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
}

// read_request_from_args_or_stdin joins any positional args, otherwise reads stdin.
pub fn read_request_from_args_or_stdin(args: &[String]) -> Result<String> {
    if !args.is_empty() {
        let joined = args.join(" ");
        let trimmed = joined.trim();
        if trimmed.is_empty() {
            bail!("request text required");
        }
        return Ok(trimmed.to_string());
    }

    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("read stdin")?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        bail!("request text required (pass arguments or pipe input)");
    }
    Ok(trimmed.to_string())
}

pub fn build_task(resolved: &config::Resolved, opts: TaskBuildOptions) -> Result<()> {
    // Enforce the "repo is clean before any agent run" assumption.
    gitutil::require_clean_repo(&resolved.repo_root)?;

    if opts.request.trim().is_empty() {
        bail!("request text required");
    }

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;
    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    queue::validate_queue_set(&before, done_ref, &resolved.id_prefix, resolved.id_width)
        .context("validate queue set before task build")?;
    let before_ids = task_id_set(&before);

    let template = prompts::load_task_builder_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt = prompts::render_task_builder_prompt(
        &template,
        &opts.request,
        &opts.hint_tags,
        &opts.hint_scope,
        project_type,
    )?;

    let codex_bin = resolved
        .config
        .agent
        .codex_bin
        .as_deref()
        .unwrap_or("codex");
    let opencode_bin = resolved
        .config
        .agent
        .opencode_bin
        .as_deref()
        .unwrap_or("opencode");

    let output = match runner::run_prompt(
        opts.runner,
        &resolved.repo_root,
        codex_bin,
        opencode_bin,
        opts.model,
        opts.reasoning_effort,
        &prompt,
    ) {
        Ok(output) => output,
        Err(err) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!(
                "task builder runner failed to execute; reverted uncommitted changes: {:#}",
                err
            );
        }
    };

    let after = match queue::load_queue_with_repair(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok((queue, repaired)) => {
            if repaired {
                eprintln!(
                    ">> [RALPH] Repaired invalid YAML scalars in {}",
                    resolved.queue_path.display()
                );
            }
            queue
        }
        Err(err) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            return Err(err);
        }
    };

    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };
    if let Err(err) = queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .context("validate queue set after task build")
    {
        gitutil::revert_uncommitted(&resolved.repo_root)?;
        return Err(err);
    }

    if output.success() {
        let added = added_tasks(&before_ids, &after);
        if added.is_empty() {
            println!(">> [RALPH] Task builder completed. No new tasks detected.");
        } else {
            println!(">> [RALPH] Task builder added {} task(s):", added.len());
            for (id, title) in added.iter().take(10) {
                println!("- {}: {}", id, title);
            }
            if added.len() > 10 {
                println!("...and {} more.", added.len() - 10);
            }
        }
        return Ok(());
    }

    let exit_reason = match output.status.code() {
        Some(code) => format!("task builder runner exited non-zero (code={code})"),
        None => "task builder runner terminated by signal".to_string(),
    };

    let combined = output.combined();
    let redacted = redaction::redact_text(&combined);
    let tail = outpututil::tail_lines(
        &redacted,
        outpututil::OUTPUT_TAIL_LINES,
        outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
    );
    if !tail.is_empty() {
        eprintln!(">> [RALPH] task builder output (tail):");
        for line in tail {
            eprintln!(">> [RALPH] task builder: {line}");
        }
    }

    gitutil::revert_uncommitted(&resolved.repo_root)?;
    bail!(
        "{}; reverted uncommitted changes; rerun is recommended",
        exit_reason
    )
}

fn task_id_set(queue: &QueueFile) -> HashSet<String> {
    let mut set = HashSet::new();
    for task in &queue.tasks {
        let id = task.id.trim();
        if id.is_empty() {
            continue;
        }
        set.insert(id.to_string());
    }
    set
}

fn added_tasks(before: &HashSet<String>, after: &QueueFile) -> Vec<(String, String)> {
    let mut added = Vec::new();
    for task in &after.tasks {
        let id = task.id.trim();
        if id.is_empty() {
            continue;
        }
        if before.contains(id) {
            continue;
        }
        added.push((id.to_string(), task.title.trim().to_string()));
    }
    added
}
