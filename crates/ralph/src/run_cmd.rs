use crate::config;
use crate::contracts::{Model, QueueFile, ReasoningEffort, Runner, TaskStatus};
use crate::{gitutil, prompts, queue, runner, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

const OUTPUT_TAIL_LINES: usize = 20;
const OUTPUT_TAIL_LINE_MAX_CHARS: usize = 200;

pub enum RunOutcome {
    NoTodo,
    Ran { task_id: String },
}

pub struct RunLoopOptions {
    /// 0 means "no limit"
    pub max_tasks: u32,
}

pub fn run_loop(resolved: &config::Resolved, opts: RunLoopOptions) -> Result<()> {
    let mut completed = 0u32;
    loop {
        if opts.max_tasks != 0 && completed >= opts.max_tasks {
            println!(">> [RALPH] Reached max task limit ({completed}).");
            return Ok(());
        }

        match run_one(resolved)? {
            RunOutcome::NoTodo => return Ok(()),
            RunOutcome::Ran { task_id } => {
                completed += 1;
                println!(">> [RALPH] Completed {task_id}.");
            }
        }
    }
}

pub fn run_one(resolved: &config::Resolved) -> Result<RunOutcome> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )?;

    let idx = match queue_file
        .tasks
        .iter()
        .position(|t| t.status == TaskStatus::Todo)
    {
        Some(idx) => idx,
        None => {
            println!(">> [RALPH] No todo tasks found.");
            return Ok(RunOutcome::NoTodo);
        }
    };

    let task = queue_file.tasks[idx].clone();
    let task_id = task.id.trim().to_string();
    if task_id.is_empty() {
        bail!("selected task has empty id");
    }

    // Require a clean repo before we invoke the runner.
    // This prevents accidental destruction of unrelated user work on failure recovery.
    gitutil::require_clean_repo(&resolved.repo_root)?;

    let task_agent = task.agent.as_ref();

    let runner_kind: Runner = task_agent
        .and_then(|agent| agent.runner)
        .or(resolved.config.agent.runner)
        .unwrap_or_default();

    let model: Model = task_agent
        .and_then(|agent| agent.model)
        .or(resolved.config.agent.model)
        .unwrap_or_default();

    let reasoning_effort: Option<ReasoningEffort> = task_agent
        .and_then(|agent| agent.reasoning_effort)
        .or(resolved.config.agent.reasoning_effort);

    runner::validate_model_for_runner(runner_kind, model)?;

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

    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let prompt = prompts::render_worker_prompt(&template, &task)?;

    let output = match runner::run_prompt(
        runner_kind,
        &resolved.repo_root,
        codex_bin,
        opencode_bin,
        model,
        reasoning_effort,
        &prompt,
    ) {
        Ok(output) => output,
        Err(err) => {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!(
				"runner invocation failed; reverted uncommitted changes; rerun is recommended: {:#}",
				err
			);
        }
    };

    if !output.success() {
        let exit_reason = match output.status.code() {
            Some(code) => format!("runner exited non-zero (code={code})"),
            None => "runner terminated by signal".to_string(),
        };

        let combined = output.combined();
        let tail = tail_lines(&combined, OUTPUT_TAIL_LINES, OUTPUT_TAIL_LINE_MAX_CHARS);
        if !tail.is_empty() {
            eprintln!(">> [RALPH] runner output (tail):");
            for line in tail {
                eprintln!(">> [RALPH] runner: {line}");
            }
        }

        gitutil::revert_uncommitted(&resolved.repo_root)?;
        bail!("runner failed ({exit_reason}); reverted uncommitted changes; rerun is recommended");
    }

    println!(">> [RALPH] Runner completed successfully for {task_id}.");

    post_run_supervise(resolved, &task_id)?;
    Ok(RunOutcome::Ran { task_id })
}

fn post_run_supervise(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let status = gitutil::status_porcelain(&resolved.repo_root)?;
    let is_dirty = !status.trim().is_empty();

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let mut done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )?;

    let (mut task_status, task_title, mut in_done) =
        find_task_status(&queue_file, &done_file, task_id)
            .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;

    if task_status == TaskStatus::Blocked {
        if is_dirty {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("task {task_id} was marked blocked; reverted uncommitted changes");
        }
        bail!("task {task_id} was marked blocked; cannot auto-revert committed changes");
    }

    if is_dirty {
        if let Err(err) = run_make_ci(&resolved.repo_root) {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("make ci failed; reverted uncommitted changes: {:#}", err);
        }

        queue_file = queue::load_queue(&resolved.queue_path)?;
        done_file = queue::load_queue_or_default(&resolved.done_path)?;
        let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
            None
        } else {
            Some(&done_file)
        };
        queue::validate_queue_set(
            &queue_file,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
        )?;

        let (status_after, _title_after, in_done_after) =
            find_task_status(&queue_file, &done_file, task_id)
                .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;
        task_status = status_after;
        in_done = in_done_after;

        if task_status == TaskStatus::Blocked {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("task {task_id} was marked blocked; reverted uncommitted changes");
        }

        if task_status != TaskStatus::Done {
            if in_done {
                gitutil::revert_uncommitted(&resolved.repo_root)?;
                bail!("task {task_id} is archived but not done");
            }
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(&mut queue_file, task_id, TaskStatus::Done, &now, None, None)?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
        }

        queue::archive_done_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?;

        let commit_message = format_task_commit_message(task_id, &task_title);
        gitutil::commit_all(&resolved.repo_root, &commit_message)?;
        if gitutil::is_ahead_of_upstream(&resolved.repo_root)? {
            gitutil::push_upstream(&resolved.repo_root)?;
        }
        gitutil::require_clean_repo(&resolved.repo_root)?;
        return Ok(());
    }

    if task_status == TaskStatus::Done && in_done {
        if gitutil::is_ahead_of_upstream(&resolved.repo_root)? {
            gitutil::push_upstream(&resolved.repo_root)?;
        }
        return Ok(());
    }

    let mut changed = false;
    if task_status != TaskStatus::Done {
        if in_done {
            bail!("task {task_id} is archived but not done");
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(&mut queue_file, task_id, TaskStatus::Done, &now, None, None)?;
        queue::save_queue(&resolved.queue_path, &queue_file)?;
        changed = true;
    }

    let report = queue::archive_done_tasks(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    if !report.moved_ids.is_empty() || !report.skipped_ids.is_empty() {
        changed = true;
    }

    if !changed {
        return Ok(());
    }

    let commit_message = format_task_commit_message(task_id, &task_title);
    gitutil::commit_all(&resolved.repo_root, &commit_message)?;
    if gitutil::is_ahead_of_upstream(&resolved.repo_root)? {
        gitutil::push_upstream(&resolved.repo_root)?;
    }
    gitutil::require_clean_repo(&resolved.repo_root)?;
    Ok(())
}

fn find_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Option<(TaskStatus, String, bool)> {
    let needle = task_id.trim();
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), false));
    }
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), true));
    }
    None
}

fn run_make_ci(repo_root: &Path) -> Result<()> {
    let status = Command::new("make")
        .arg("ci")
        .current_dir(repo_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("run make ci in {}", repo_root.display()))?;

    if status.success() {
        return Ok(());
    }

    bail!("make ci failed with exit code {:?}", status.code())
}

fn format_task_commit_message(task_id: &str, title: &str) -> String {
    let mut raw = format!("{task_id}: {title}");
    raw = raw.replace(['\n', '\r', '\t'], " ");
    let squashed = raw.split_whitespace().collect::<Vec<&str>>().join(" ");
    truncate_chars(&squashed, 100)
}

fn tail_lines(text: &str, max_lines: usize, max_chars: usize) -> Vec<String> {
    if max_lines == 0 || text.trim().is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect();

    if lines.len() > max_lines {
        lines = lines[lines.len() - max_lines..].to_vec();
    }

    lines
        .into_iter()
        .map(|line| truncate_chars(line.trim(), max_chars))
        .collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }
    if chars.next().is_none() {
        return out;
    }
    if max_chars <= 3 {
        return out;
    }
    out.truncate(max_chars - 3);
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_adds_ellipsis() {
        let value = "abcdefghijklmnopqrstuvwxyz";
        let truncated = truncate_chars(value, 10);
        assert_eq!(truncated, "abcdefg...");
    }
}
