//! `ralph queue issue` subcommand for publishing tasks to GitHub Issues.
//!
//! Responsibilities:
//! - Publish one or many queue tasks as GitHub Issues.
//! - Persist GitHub metadata (`github_issue_url`, `github_issue_number`, sync hash)
//!   back into task `custom_fields` for incremental sync.
//! - Provide bulk dry-run / execute behavior with filtering and confirmation.
//!
//! Not handled here:
//! - GitHub Projects automation.
//! - Queue schema migrations.
//! - GUI-specific issue publishing workflows.
//!
//! Invariants/assumptions:
//! - `gh` CLI must be available and authenticated for execute mode.
//! - Task IDs must exist in the active queue.
//! - Queue writes occur only while queue lock is held.

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use regex::Regex;
use std::collections::HashSet;
use std::io::{self, BufRead, IsTerminal, Write};

use crate::cli::load_and_validate_queues_read_only;
use crate::cli::queue::shared::StatusArg;
use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::git::{
    GITHUB_ISSUE_SYNC_HASH_KEY, check_gh_available, compute_issue_sync_hash, create_issue,
    edit_issue, normalize_issue_metadata_list, parse_issue_number,
};

const DEFAULT_PUBLISH_STATUSES: &[TaskStatus] = &[
    TaskStatus::Todo,
    TaskStatus::Doing,
    TaskStatus::Done,
    TaskStatus::Rejected,
];
const GITHUB_ISSUE_URL_KEY: &str = "github_issue_url";
const GITHUB_ISSUE_NUMBER_KEY: &str = "github_issue_number";

#[derive(Args)]
pub struct QueueIssueArgs {
    #[command(subcommand)]
    pub command: QueueIssueCommand,
}

#[derive(Subcommand)]
pub enum QueueIssueCommand {
    /// Publish (create or update) a single task as a GitHub issue.
    Publish(QueueIssuePublishArgs),

    /// Publish (create or update) many tasks as GitHub issues.
    PublishMany(QueueIssuePublishManyArgs),
}

/// Arguments for `ralph queue issue publish`.
#[derive(Args, Clone)]
#[command(after_long_help = "Examples:\n\
  # Preview rendered markdown for a task\n\
  ralph queue issue publish RQ-0655 --dry-run\n\
  # Create/update issue metadata and persist custom_fields\n\
  ralph queue issue publish RQ-0655\n\
  # Add labels/assignees\n\
  ralph queue issue publish RQ-0655 --label bug --assignee @me\n\
  # Target another repo\n\
  ralph queue issue publish RQ-0655 --repo owner/repo")]
pub struct QueueIssuePublishArgs {
    /// Task ID to publish.
    pub task_id: String,

    /// Dry run: print rendered title/body and the action that would be executed.
    #[arg(long)]
    pub dry_run: bool,

    /// Labels to apply (repeatable).
    #[arg(long)]
    pub label: Vec<String>,

    /// Assignees to apply (repeatable). Supports @me for self-assignment.
    #[arg(long)]
    pub assignee: Vec<String>,

    /// Target repository (OWNER/REPO format). Optional; uses current repo by default.
    #[arg(long)]
    pub repo: Option<String>,
}

/// Arguments for `ralph queue issue publish-many`.
#[derive(Args, Clone)]
#[command(after_long_help = "Examples:\n\
  # Safe preview from todo backlog\n\
  ralph queue issue publish-many --status todo --tag bug --dry-run\n\
  # Publish selected slice with regex and labels\n\
  ralph queue issue publish-many --status todo --tag bug --id-pattern '^RQ-08' --label triage\n\
  # Execute publish with confirmation override\n\
  ralph queue issue publish-many --status todo --execute --force")]
pub struct QueueIssuePublishManyArgs {
    /// Filter by status (repeatable). Defaults to all non-draft statuses.
    #[arg(long)]
    pub status: Vec<StatusArg>,

    /// Filter by tag (repeatable).
    #[arg(long)]
    pub tag: Vec<String>,

    /// Filter by task ID regular expression.
    #[arg(long)]
    pub id_pattern: Option<String>,

    /// Preview mode (default). No writes to queue or GitHub.
    #[arg(long)]
    pub dry_run: bool,

    /// Execute publishes and persist GitHub metadata.
    #[arg(long)]
    pub execute: bool,

    /// Labels to apply for each issue in the bulk run.
    #[arg(long)]
    pub label: Vec<String>,

    /// Assignees to apply for each issue in the bulk run.
    #[arg(long)]
    pub assignee: Vec<String>,

    /// Target repository (OWNER/REPO format). Optional; uses current repo by default.
    #[arg(long)]
    pub repo: Option<String>,
}

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueIssueArgs) -> Result<()> {
    match args.command {
        QueueIssueCommand::Publish(args) => handle_publish(resolved, force, args),
        QueueIssueCommand::PublishMany(args) => handle_publish_many(resolved, force, args),
    }
}

pub(crate) fn handle_publish(
    resolved: &Resolved,
    force: bool,
    args: QueueIssuePublishArgs,
) -> Result<()> {
    let task_id = args.task_id.trim();
    if task_id.is_empty() {
        bail!("Task ID must be non-empty");
    }

    if args.dry_run {
        let (mut queue_file, _done_file) = load_and_validate_queues_read_only(resolved, false)?;
        let result = publish_task(
            resolved,
            &mut queue_file,
            task_id,
            PublishMode::DryRun,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        )?;

        return print_single_publish_result(
            &queue_file,
            task_id,
            result,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        );
    }

    check_gh_available()?;
    let _lock =
        crate::queue::acquire_queue_lock(&resolved.repo_root, "queue issue publish", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(resolved, &format!("queue issue publish {}", task_id))?;

    let (mut queue_file, _done_file) = crate::queue::load_and_validate_queues(resolved, false)?;
    let result = publish_task(
        resolved,
        &mut queue_file,
        task_id,
        PublishMode::Execute,
        &args.label,
        &args.assignee,
        args.repo.as_deref(),
    )?;

    match &result {
        PublishItemResult::Created | PublishItemResult::Updated => {
            crate::queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
            crate::queue::save_queue(&resolved.queue_path, &queue_file)?;
        }
        PublishItemResult::SkippedUnchanged => {}
        PublishItemResult::Failed(err) => return Err(anyhow!("{err}")),
    }

    match result {
        PublishItemResult::Created | PublishItemResult::Updated => {
            let task = find_task(&queue_file, task_id)?;
            let url = fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_URL_KEY)
                .unwrap_or_else(|| "unknown".to_string());
            if matches!(result, PublishItemResult::Created) {
                println!("Created GitHub issue: {url}");
            } else {
                println!("Updated GitHub issue: {url}");
            }
        }
        PublishItemResult::SkippedUnchanged => {
            println!("No changes for '{task_id}'; issue payload already synced.");
        }
        PublishItemResult::Failed(err) => return Err(anyhow!("{err}")),
    }

    Ok(())
}

pub(crate) fn handle_publish_many(
    resolved: &Resolved,
    force: bool,
    args: QueueIssuePublishManyArgs,
) -> Result<()> {
    let mode = resolve_publish_mode(args.dry_run, args.execute)?;
    let filters = parse_publish_many_filters(&args)?;
    let (queue_for_plan, _done_file) = load_and_validate_queues_read_only(resolved, false)?;
    let selected_task_ids = select_publishable_task_ids(&queue_for_plan, &filters)?;

    if selected_task_ids.is_empty() {
        println!("No matching tasks found for publish-many filters.");
        return Ok(());
    }

    let mut plan_queue = queue_for_plan;
    let mut plan_summary = PublishManySummary {
        selected: selected_task_ids.len(),
        ..PublishManySummary::default()
    };
    let mut planned = Vec::with_capacity(selected_task_ids.len());

    for task_id in &selected_task_ids {
        let result = publish_task(
            resolved,
            &mut plan_queue,
            task_id,
            PublishMode::DryRun,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        )?;

        accumulate_publish_result(&mut plan_summary, &result);
        planned.push((task_id.clone(), result));
    }

    print_publish_many_plan(&selected_task_ids, &planned);
    print_publish_many_summary(&plan_summary, true);

    if matches!(mode, PublishMode::DryRun) {
        return Ok(());
    }

    if !force {
        if !is_terminal_context() {
            bail!(
                "Refusing to execute bulk publish in non-interactive context without --force. Use --dry-run first."
            );
        }
        if !confirm_execution(&plan_summary)? {
            bail!("Bulk publish cancelled by user");
        }
    }

    check_gh_available()?;
    let _lock =
        crate::queue::acquire_queue_lock(&resolved.repo_root, "queue issue publish-many", force)?;

    // Create undo snapshot BEFORE any mutations
    crate::undo::create_undo_snapshot(resolved, "queue issue publish-many")?;

    let (mut queue_file, _done_file) = crate::queue::load_and_validate_queues(resolved, false)?;
    let mut final_summary = PublishManySummary {
        selected: selected_task_ids.len(),
        ..PublishManySummary::default()
    };
    let mut failures = Vec::new();

    for task_id in &selected_task_ids {
        let result = publish_task(
            resolved,
            &mut queue_file,
            task_id,
            PublishMode::Execute,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        )
        .unwrap_or_else(PublishItemResult::Failed);

        if let PublishItemResult::Failed(err) = &result {
            failures.push((task_id.clone(), err.to_string()));
        }

        print_publish_many_task_result(task_id, &result);
        accumulate_publish_result(&mut final_summary, &result);
    }

    if final_summary.has_mutations() {
        crate::queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
        crate::queue::save_queue(&resolved.queue_path, &queue_file)?;
    }

    print_publish_many_summary(&final_summary, false);
    if !failures.is_empty() {
        println!();
        println!("Failures:");
        for (task_id, reason) in failures {
            println!("  {task_id}: {reason}");
        }
        bail!(
            "publish-many completed with {} failed task(s).",
            final_summary.failed
        );
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum PublishMode {
    DryRun,
    Execute,
}

#[derive(Debug)]
enum PublishItemResult {
    Created,
    Updated,
    SkippedUnchanged,
    Failed(anyhow::Error),
}

#[derive(Debug, Default)]
struct PublishManySummary {
    selected: usize,
    created: usize,
    updated: usize,
    skipped: usize,
    failed: usize,
}

impl PublishManySummary {
    fn has_mutations(&self) -> bool {
        self.created > 0 || self.updated > 0
    }
}

struct PublishManyFilters {
    statuses: Vec<TaskStatus>,
    tags: Vec<String>,
    id_pattern: Option<Regex>,
}

fn resolve_publish_mode(dry_run: bool, execute: bool) -> Result<PublishMode> {
    if dry_run && execute {
        bail!("Cannot combine --dry-run and --execute");
    }
    if execute {
        Ok(PublishMode::Execute)
    } else {
        Ok(PublishMode::DryRun)
    }
}

fn parse_publish_many_filters(args: &QueueIssuePublishManyArgs) -> Result<PublishManyFilters> {
    let statuses = if args.status.is_empty() {
        DEFAULT_PUBLISH_STATUSES.to_vec()
    } else {
        args.status.iter().map(|status| (*status).into()).collect()
    };

    let tags = args
        .tag
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();

    let id_pattern = match args.id_pattern.as_deref() {
        Some(pattern) if !pattern.trim().is_empty() => {
            Some(Regex::new(pattern).with_context(|| {
                format!("Invalid --id-pattern '{pattern}'. Use valid regular-expression syntax.")
            })?)
        }
        Some(pattern) if pattern.trim().is_empty() => {
            bail!("--id-pattern cannot be empty when provided");
        }
        Some(_) => unreachable!(),
        None => None,
    };

    Ok(PublishManyFilters {
        statuses,
        tags,
        id_pattern,
    })
}

fn select_publishable_task_ids(
    queue_file: &QueueFile,
    filters: &PublishManyFilters,
) -> Result<Vec<String>> {
    let status_filter: HashSet<TaskStatus> = filters.statuses.iter().copied().collect();
    let statuses = status_filter.into_iter().collect::<Vec<_>>();

    let tasks = crate::queue::filter_tasks(queue_file, &statuses, &filters.tags, &[], None);
    let mut selected = Vec::new();

    for task in tasks {
        if let Some(pattern) = &filters.id_pattern
            && !pattern.is_match(task.id.trim())
        {
            continue;
        }
        selected.push(task.id.trim().to_string());
    }

    Ok(selected)
}

fn publish_task(
    resolved: &Resolved,
    queue: &mut QueueFile,
    task_id: &str,
    mode: PublishMode,
    labels: &[String],
    assignees: &[String],
    repo: Option<&str>,
) -> Result<PublishItemResult> {
    let normalized_labels = normalize_issue_metadata_list(labels);
    let normalized_assignees = normalize_issue_metadata_list(assignees);

    let task = find_task_mut(queue, task_id)?;
    let title = format!("{}: {}", task.id.trim(), task.title);
    let body = super::export::render_task_as_github_issue_body(task);
    let sync_hash = compute_issue_sync_hash(
        &title,
        &body,
        &normalized_labels,
        &normalized_assignees,
        repo,
    )?;

    let existing_url = fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_URL_KEY);

    if let Some(url) = existing_url {
        let existing_sync_hash =
            fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_SYNC_HASH_KEY);
        if existing_sync_hash.as_deref() == Some(sync_hash.as_str()) {
            return Ok(PublishItemResult::SkippedUnchanged);
        }

        if matches!(mode, PublishMode::DryRun) {
            return Ok(PublishItemResult::Updated);
        }

        let tmp = crate::fsutil::create_ralph_temp_file("issue")
            .context("create temp file for issue body")?;
        std::fs::write(tmp.path(), body).context("write issue body to temp file")?;
        edit_issue(
            &resolved.repo_root,
            repo,
            &url,
            &title,
            tmp.path(),
            &normalized_labels,
            &normalized_assignees,
        )
        .with_context(|| format!("Failed to update GitHub issue at {url}"))?;

        if fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_NUMBER_KEY).is_none()
            && let Some(number) = parse_issue_number(&url)
        {
            task.custom_fields
                .insert(GITHUB_ISSUE_NUMBER_KEY.to_string(), number.to_string());
        }

        task.custom_fields
            .insert(GITHUB_ISSUE_SYNC_HASH_KEY.to_string(), sync_hash);
        task.updated_at = Some(crate::timeutil::now_utc_rfc3339_or_fallback());
        Ok(PublishItemResult::Updated)
    } else {
        if matches!(mode, PublishMode::DryRun) {
            return Ok(PublishItemResult::Created);
        }

        let tmp = crate::fsutil::create_ralph_temp_file("issue")
            .context("create temp file for issue body")?;
        std::fs::write(tmp.path(), body).context("write issue body to temp file")?;
        let issue = create_issue(
            &resolved.repo_root,
            repo,
            &title,
            tmp.path(),
            &normalized_labels,
            &normalized_assignees,
        )?;

        task.custom_fields
            .insert(GITHUB_ISSUE_URL_KEY.to_string(), issue.url.clone());
        if let Some(number) = issue.number {
            task.custom_fields
                .insert(GITHUB_ISSUE_NUMBER_KEY.to_string(), number.to_string());
        }
        task.custom_fields
            .insert(GITHUB_ISSUE_SYNC_HASH_KEY.to_string(), sync_hash);
        task.updated_at = Some(crate::timeutil::now_utc_rfc3339_or_fallback());

        Ok(PublishItemResult::Created)
    }
}

fn print_single_publish_result(
    queue: &QueueFile,
    task_id: &str,
    result: PublishItemResult,
    labels: &[String],
    assignees: &[String],
    repo: Option<&str>,
) -> Result<()> {
    let task = find_task(queue, task_id)?;
    let title = format!("{}: {}", task.id, task.title);
    let body = super::export::render_task_as_github_issue_body(task);

    println!("=== DRY RUN ===");
    println!("Target task: {task_id}");
    println!("Title: {title}");
    println!();
    println!("Body:");
    println!("{body}");

    match result {
        PublishItemResult::Created => println!("Would create new GitHub issue."),
        PublishItemResult::Updated => {
            let existing_url = fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_URL_KEY)
                .unwrap_or_else(|| "unknown".to_string());
            println!("Would update existing issue: {existing_url}");
        }
        PublishItemResult::SkippedUnchanged => {
            println!("Would skip task; issue payload is already synced.");
        }
        PublishItemResult::Failed(err) => return Err(err),
    }

    if let Some(repo) = repo {
        println!("Target repo: {repo}");
    }
    if !labels.is_empty() {
        println!("Labels: {}", labels.join(", "));
    }
    if !assignees.is_empty() {
        println!("Assignees: {}", assignees.join(", "));
    }

    Ok(())
}

fn print_publish_many_plan(task_ids: &[String], results: &[(String, PublishItemResult)]) {
    println!("publish-many plan for {} task(s):", task_ids.len());
    for (task_id, result) in results {
        let label = match result {
            PublishItemResult::Created => "CREATE",
            PublishItemResult::Updated => "UPDATE",
            PublishItemResult::SkippedUnchanged => "SKIP",
            PublishItemResult::Failed(_) => "ERROR",
        };

        if let PublishItemResult::Failed(err) = result {
            println!("  [{label}] {task_id}: {err}");
        } else {
            println!("  [{label}] {task_id}");
        }
    }
}

fn print_publish_many_task_result(task_id: &str, result: &PublishItemResult) {
    let label = match result {
        PublishItemResult::Created => "CREATE",
        PublishItemResult::Updated => "UPDATE",
        PublishItemResult::SkippedUnchanged => "SKIP",
        PublishItemResult::Failed(_) => "ERROR",
    };

    if let PublishItemResult::Failed(err) = result {
        println!("  [{label}] {task_id}: {err}");
    } else {
        println!("  [{label}] {task_id}");
    }
}

fn print_publish_many_summary(summary: &PublishManySummary, dry_run: bool) {
    let mode = if dry_run { "dry-run" } else { "execution" };
    println!(
        "publish-many {mode} summary: selected={} created={} updated={} skipped={} failed={}",
        summary.selected, summary.created, summary.updated, summary.skipped, summary.failed,
    );
}

fn accumulate_publish_result(summary: &mut PublishManySummary, result: &PublishItemResult) {
    match result {
        PublishItemResult::Created => summary.created += 1,
        PublishItemResult::Updated => summary.updated += 1,
        PublishItemResult::SkippedUnchanged => summary.skipped += 1,
        PublishItemResult::Failed(_) => summary.failed += 1,
    }
}

fn confirm_execution(summary: &PublishManySummary) -> Result<bool> {
    println!("About to execute {} task(s):", summary.selected);
    println!("  create: {}", summary.created);
    println!("  update: {}", summary.updated);
    println!("  skip: {}", summary.skipped);
    print!("Proceed with publish-many execution? [y/N]: ");
    io::stdout().flush().context("flush confirmation prompt")?;

    let mut response = String::new();
    io::stdin()
        .lock()
        .read_line(&mut response)
        .context("read confirmation input")?;
    Ok(matches!(
        response.trim().to_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn is_terminal_context() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn fetch_custom_field(
    custom_fields: &std::collections::HashMap<String, String>,
    key: &str,
) -> Option<String> {
    custom_fields
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn find_task<'a>(queue: &'a QueueFile, task_id: &str) -> Result<&'a Task> {
    let task_id = task_id.trim();
    queue
        .tasks
        .iter()
        .find(|task| task.id.trim() == task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue(task_id)
            )
        })
}

fn find_task_mut<'a>(queue: &'a mut QueueFile, task_id: &str) -> Result<&'a mut Task> {
    let task_id = task_id.trim();
    queue
        .tasks
        .iter_mut()
        .find(|task| task.id.trim() == task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue(task_id)
            )
        })
}
