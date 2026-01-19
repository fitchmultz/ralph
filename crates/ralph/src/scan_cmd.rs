use crate::contracts::{Model, ProjectType, QueueFile, ReasoningEffort, Runner};
use crate::{config, gitutil, outpututil, prompts, queue, redaction, runner};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;

pub struct ScanOptions {
    pub focus: String,
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub force: bool,
}

pub fn run_scan(resolved: &config::Resolved, opts: ScanOptions) -> Result<()> {
    // Prevents catastrophic data loss if scan fails and reverts uncommitted changes.
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        opts.force,
        &[".ralph/queue.yaml", ".ralph/done.yaml"],
    )?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "scan", opts.force)?;

    let before = match queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok((queue, repaired)) => {
            if repaired {
                log::warn!(
                    "Repaired queue YAML format issues in {}",
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
    let (done, repaired_done) = queue::load_queue_or_default_with_repair(
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    queue::warn_if_repaired(&resolved.done_path, repaired_done);
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
    let gemini_bin = resolved
        .config
        .agent
        .gemini_bin
        .as_deref()
        .unwrap_or("gemini");
    let bins = runner::RunnerBinaries {
        codex: codex_bin,
        opencode: opencode_bin,
        gemini: gemini_bin,
    };

    let _output = match runner::run_prompt(
        opts.runner,
        &resolved.repo_root,
        bins,
        opts.model,
        opts.reasoning_effort,
        &prompt,
        None,
    ) {
        Ok(output) => output,
        Err(runner::RunnerError::Interrupted) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("scan runner interrupted; reverted uncommitted changes");
        }
        Err(runner::RunnerError::Timeout) => {
            bail!("scan runner timed out; changes in the working tree were NOT reverted");
        }
        Err(runner::RunnerError::NonZeroExit {
            code,
            stdout: _,
            stderr,
        }) => {
            let redacted = redaction::redact_text(&stderr);
            let tail = outpututil::tail_lines(
                &redacted,
                outpututil::OUTPUT_TAIL_LINES,
                outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
            );
            if !tail.is_empty() {
                log::error!("scan runner stderr (tail):");
                for line in tail {
                    log::info!("scan runner: {line}");
                }
            }
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("scan runner exited non-zero (code={code}); reverted uncommitted changes; rerun is recommended");
        }
        Err(runner::RunnerError::TerminatedBySignal { stdout: _, stderr }) => {
            let redacted = redaction::redact_text(&stderr);
            let tail = outpututil::tail_lines(
                &redacted,
                outpututil::OUTPUT_TAIL_LINES,
                outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
            );
            if !tail.is_empty() {
                log::error!("scan runner stderr (tail):");
                for line in tail {
                    log::info!("scan runner: {line}");
                }
            }
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("scan runner terminated by signal; reverted uncommitted changes; rerun is recommended");
        }
        Err(err) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!(
                "scan runner failed to execute; reverted uncommitted changes: {:#}",
                err
            );
        }
    };

    let mut after = match queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok((queue, repaired)) => {
            if repaired {
                log::warn!(
                    "Repaired queue YAML format issues in {}",
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

    let (done_after, repaired_done_after) = queue::load_queue_or_default_with_repair(
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    queue::warn_if_repaired(&resolved.done_path, repaired_done_after);
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

    let added = added_tasks(&before_ids, &after);
    if !added.is_empty() {
        let added_ids: Vec<String> = added.iter().map(|(id, _)| id.clone()).collect();
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "2026-01-18T00:00:00Z".to_string());
        let default_request = format!("scan: {}", opts.focus);
        queue::backfill_missing_fields(&mut after, &added_ids, &default_request, &now);
        queue::save_queue(&resolved.queue_path, &after)
            .context("save queue with backfilled fields")?;
    }
    if added.is_empty() {
        log::info!("Scan completed. No new tasks detected.");
    } else {
        log::info!("Scan added {} task(s):", added.len());
        for (id, title) in added.iter().take(15) {
            log::info!("- {}: {}", id, title);
        }
        if added.len() > 15 {
            log::info!("...and {} more.", added.len() - 15);
        }
    }
    Ok(())
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
