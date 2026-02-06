//! `ralph queue issue` subcommand for publishing tasks to GitHub Issues.
//!
//! Responsibilities:
//! - Publish (create) or re-publish (update/sync) a single Ralph task as a GitHub Issue.
//! - Persist the created/updated issue metadata back into the task's `custom_fields`.
//!
//! Not handled here:
//! - Bulk publish (single task only).
//! - GitHub Projects automation.
//! - TUI integration (CLI-only).
//!
//! Invariants/assumptions:
//! - `gh` CLI must be installed and authenticated.
//! - The task ID must exist in the queue.
//! - Queue mutations occur within a lock.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;

/// Arguments for `ralph queue issue`.
#[derive(Args)]
pub struct QueueIssueArgs {
    #[command(subcommand)]
    pub command: QueueIssueCommand,
}

#[derive(Subcommand)]
pub enum QueueIssueCommand {
    /// Publish (create or update) a single task as a GitHub issue.
    Publish(QueueIssuePublishArgs),
}

/// Arguments for `ralph queue issue publish`.
#[derive(Args)]
#[command(after_long_help = "Examples:\n\
  # Preview markdown + gh commands\n\
  ralph queue issue publish RQ-0655 --dry-run\n\n\
  # Create or update the issue and persist github_issue_url into custom_fields\n\
  ralph queue issue publish RQ-0655\n\n\
  # Add labels/assignees\n\
  ralph queue issue publish RQ-0655 --label bug --label help-wanted --assignee @me\n\n\
  # Target another repo\n\
  ralph queue issue publish RQ-0655 --repo owner/repo\n")]
pub struct QueueIssuePublishArgs {
    /// Task ID to publish.
    pub task_id: String,

    /// Dry run: print rendered title/body and the gh command that would be executed.
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

/// Handle the queue issue subcommand.
pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueIssueArgs) -> Result<()> {
    match args.command {
        QueueIssueCommand::Publish(publish_args) => handle_publish(resolved, force, publish_args),
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

    // Load queue (without done file for this operation)
    let (queue_file, _done_file) = load_and_validate_queues(resolved, false)?;

    // Find the task
    let task = queue_file
        .tasks
        .iter()
        .find(|t| t.id.trim() == task_id)
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in queue", task_id))?;

    let title = format!("{}: {}", task.id, task.title);
    let body = super::export::render_task_as_github_issue_body(task);

    // Check if we already have a GitHub issue URL
    let existing_url = task
        .custom_fields
        .get("github_issue_url")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if args.dry_run {
        println!("=== DRY RUN ===");
        println!("Title: {}", title);
        println!();
        println!("Body:");
        println!("{}", body);
        println!();
        if let Some(ref url) = existing_url {
            println!("Would update existing issue: {}", url);
        } else {
            println!("Would create new issue");
        }
        if let Some(ref repo) = args.repo {
            println!("Target repo: {}", repo);
        }
        if !args.label.is_empty() {
            println!("Labels: {}", args.label.join(", "));
        }
        if !args.assignee.is_empty() {
            println!("Assignees: {}", args.assignee.join(", "));
        }
        return Ok(());
    }

    // Check gh availability
    crate::git::check_gh_available()?;

    // Acquire lock for queue mutation
    let _lock =
        crate::queue::acquire_queue_lock(&resolved.repo_root, "queue issue publish", force)?;

    // Reload queue after acquiring lock
    let (mut queue_file, _done_file) = load_and_validate_queues(resolved, false)?;

    // Find the task again (mutable this time)
    let task = queue_file
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == task_id)
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in queue", task_id))?;

    // Recompute title and body
    let title = format!("{}: {}", task.id, task.title);
    let body = super::export::render_task_as_github_issue_body(task);

    // Check for existing URL again
    let existing_url = task
        .custom_fields
        .get("github_issue_url")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Create temp file for body
    let tmp = tempfile::NamedTempFile::new().context("create temp file for issue body")?;
    std::fs::write(tmp.path(), body).context("write issue body to temp file")?;

    let repo_sel = args.repo.as_deref();

    if let Some(url) = existing_url {
        // Update existing issue
        crate::git::edit_issue(
            &resolved.repo_root,
            repo_sel,
            &url,
            &title,
            tmp.path(),
            &args.label,
            &args.assignee,
        )
        .with_context(|| format!("Failed to update GitHub issue at {}", url))?;

        // Ensure we have the issue number
        if task
            .custom_fields
            .get("github_issue_number")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .is_none()
            && let Some(num) = crate::git::parse_issue_number(&url)
        {
            task.custom_fields
                .insert("github_issue_number".to_string(), num.to_string());
        }

        println!("Updated GitHub issue: {}", url);
    } else {
        // Create new issue
        let info = crate::git::create_issue(
            &resolved.repo_root,
            repo_sel,
            &title,
            tmp.path(),
            &args.label,
            &args.assignee,
        )
        .context("Failed to create GitHub issue")?;

        // Persist metadata
        task.custom_fields
            .insert("github_issue_url".to_string(), info.url.clone());
        if let Some(num) = info.number {
            task.custom_fields
                .insert("github_issue_number".to_string(), num.to_string());
        }

        println!("Created GitHub issue: {}", info.url);
    }

    // Update timestamp
    task.updated_at = Some(crate::timeutil::now_utc_rfc3339_or_fallback());

    // Validate before write
    crate::queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;

    // Save queue
    crate::queue::save_queue(&resolved.queue_path, &queue_file)?;

    Ok(())
}
