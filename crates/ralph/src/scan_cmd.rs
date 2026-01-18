use crate::contracts::{Model, ProjectType, QueueFile, ReasoningEffort, Runner};
use crate::{config, gitutil, outpututil, prompts, queue, redaction, runner};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;

pub struct ScanOptions {
    pub focus: String,
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
}

pub fn run_scan(resolved: &config::Resolved, opts: ScanOptions) -> Result<()> {
    // Prevents catastrophic data loss if scan fails and reverts uncommitted changes.
    gitutil::require_clean_repo(&resolved.repo_root)?;

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;
    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    if let Err(err) =
        queue::validate_queue_set(&before, done_ref, &resolved.id_prefix, resolved.id_width)
            .context("validate queue set before scan")
    {
        gitutil::revert_uncommitted(&resolved.repo_root)?;
        return Err(err);
    }
    let before_ids = task_id_set(&before);

    let template = prompts::load_scan_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt = prompts::render_scan_prompt(&template, &opts.focus, project_type)?;

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
                "scan runner failed to execute; reverted uncommitted changes: {:#}",
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
    .context("validate queue set after scan")
    {
        gitutil::revert_uncommitted(&resolved.repo_root)?;
        return Err(err);
    }

    if output.success() {
        let added = added_tasks(&before_ids, &after);
        if added.is_empty() {
            println!(">> [RALPH] Scan completed. No new tasks detected.");
        } else {
            println!(">> [RALPH] Scan added {} task(s):", added.len());
            for (id, title) in added.iter().take(15) {
                println!("- {}: {}", id, title);
            }
            if added.len() > 15 {
                println!("...and {} more.", added.len() - 15);
            }
        }
        return Ok(());
    }

    let exit_reason = match output.status.code() {
        Some(code) => format!("scan runner exited non-zero (code={code})"),
        None => "scan runner terminated by signal".to_string(),
    };

    let combined = output.combined();
    let redacted = redaction::redact_text(&combined);
    let tail = outpututil::tail_lines(
        &redacted,
        outpututil::OUTPUT_TAIL_LINES,
        outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
    );
    if !tail.is_empty() {
        eprintln!(">> [RALPH] scan runner output (tail):");
        for line in tail {
            eprintln!(">> [RALPH] scan runner: {line}");
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
