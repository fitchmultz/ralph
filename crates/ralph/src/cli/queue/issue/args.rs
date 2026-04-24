//! Clap argument types for `ralph queue issue`.
//!
//! Purpose:
//! - Clap argument types for `ralph queue issue`.
//!
//! Responsibilities:
//! - Define the single-task and bulk GitHub issue publish CLI surface.
//! - Keep help text/examples colocated with the issue subcommands.
//! - Expose stable argument types for queue CLI parsing and tests.
//!
//! Not handled here:
//! - Queue loading or mutation.
//! - GitHub API/CLI execution.
//! - Publish workflow validation beyond clap parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Argument defaults preserve existing CLI behavior.
//! - Examples stay aligned with the implemented publish flows.
//! - Parsing-only tests may construct these types directly.

use clap::{Args, Subcommand};

/// Top-level arguments for `ralph queue issue`.
#[derive(Args)]
pub struct QueueIssueArgs {
    #[command(subcommand)]
    pub command: QueueIssueCommand,
}

/// Issue publishing subcommands.
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
    pub status: Vec<super::super::shared::StatusArg>,

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
