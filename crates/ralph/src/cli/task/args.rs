//! CLI argument definitions for `ralph task ...` commands.
//!
//! Responsibilities:
//! - Define all `#[derive(Args)]` structs for task subcommands.
//! - Define all `#[derive(Subcommand)]` enums for command routing.
//! - Define all `#[derive(ValueEnum)]` enums for typed arguments.
//! - Provide conversions from CLI types to internal types.
//!
//! Not handled here:
//! - Command execution logic (see individual handler modules).
//! - Business logic or queue operations.
//!
//! Invariants/assumptions:
//! - All argument types must be `Clone` where needed for clap flattening.
//! - Defaults should match the behavior described in help text.

use clap::{Args, Subcommand, ValueEnum};

use crate::agent;
use crate::cli::queue::QueueShowFormat;
use crate::contracts::{TaskPriority, TaskStatus};
use crate::queue::TaskEditKey;

/// Batching mode for grouping related files in build-refactor.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
#[clap(rename_all = "snake_case")]
pub enum BatchMode {
    /// Group files in same directory with similar names (e.g., test files with source).
    #[default]
    Auto,
    /// Create individual task per file.
    Never,
    /// Group all files in same module/directory.
    Aggressive,
}

/// Task priority argument for CLI.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum TaskPriorityArg {
    Critical,
    High,
    Medium,
    Low,
}

impl From<TaskPriorityArg> for TaskPriority {
    fn from(value: TaskPriorityArg) -> Self {
        match value {
            TaskPriorityArg::Critical => TaskPriority::Critical,
            TaskPriorityArg::High => TaskPriority::High,
            TaskPriorityArg::Medium => TaskPriority::Medium,
            TaskPriorityArg::Low => TaskPriority::Low,
        }
    }
}

/// Shared task selection + filters for batch operations.
#[derive(Args, Clone, Debug, Default)]
pub struct BatchSelectArgs {
    /// Task IDs to target (conflicts with --tag-filter).
    #[arg(value_name = "TASK_ID...", conflicts_with = "tag_filter")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag (case-insensitive, repeatable; OR logic).
    #[arg(long, value_name = "TAG", conflicts_with = "task_ids")]
    pub tag_filter: Vec<String>,

    /// Filter selected tasks by status (repeatable; OR logic).
    #[arg(long, value_enum, value_name = "STATUS")]
    pub status_filter: Vec<TaskStatusArg>,

    /// Filter selected tasks by priority (repeatable; OR logic).
    #[arg(long, value_enum, value_name = "PRIORITY")]
    pub priority_filter: Vec<TaskPriorityArg>,

    /// Filter selected tasks by scope substring (repeatable; OR logic; case-insensitive).
    #[arg(long, value_name = "PATTERN")]
    pub scope_filter: Vec<String>,

    /// Filter selected tasks whose updated_at is older than this cutoff.
    /// Supported forms: "7d", "1w", "2026-01-01", RFC3339
    #[arg(long, value_name = "WHEN")]
    pub older_than: Option<String>,
}

/// Batch operation type.
#[derive(Subcommand)]
pub enum BatchOperation {
    /// Update status for multiple tasks.
    Status(BatchStatusArgs),
    /// Set a custom field on multiple tasks.
    Field(BatchFieldArgs),
    /// Edit any field on multiple tasks.
    Edit(BatchEditArgs),

    /// Delete multiple tasks from the active queue.
    Delete(BatchDeleteArgs),

    /// Archive terminal tasks (Done/Rejected) from active queue into done archive.
    Archive(BatchArchiveArgs),

    /// Clone multiple tasks.
    Clone(BatchCloneArgs),

    /// Split multiple tasks into child tasks.
    Split(BatchSplitArgs),

    /// Append plan items to multiple tasks.
    #[command(name = "plan-append")]
    PlanAppend(BatchPlanAppendArgs),

    /// Prepend plan items to multiple tasks.
    #[command(name = "plan-prepend")]
    PlanPrepend(BatchPlanPrependArgs),
}

/// Arguments for batch status operation.
#[derive(Args)]
pub struct BatchStatusArgs {
    /// New status.
    #[arg(value_enum)]
    pub status: TaskStatusArg,

    /// Optional note to append to all affected tasks.
    #[arg(long)]
    pub note: Option<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch field operation.
#[derive(Args)]
pub struct BatchFieldArgs {
    /// Custom field key.
    pub key: String,

    /// Custom field value.
    pub value: String,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch edit operation.
#[derive(Args)]
pub struct BatchEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value.
    pub value: String,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch delete operation.
#[derive(Args)]
pub struct BatchDeleteArgs {
    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch archive operation.
#[derive(Args)]
pub struct BatchArchiveArgs {
    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch clone operation.
#[derive(Args)]
pub struct BatchCloneArgs {
    /// Status for the cloned tasks (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to the cloned task titles.
    #[arg(long)]
    pub title_prefix: Option<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch split operation.
#[derive(Args)]
pub struct BatchSplitArgs {
    /// Number of child tasks to create per source task (default: 2, minimum: 2).
    #[arg(short = 'n', long, default_value = "2")]
    pub number: usize,

    /// Status for child tasks (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to child task titles.
    #[arg(long)]
    pub title_prefix: Option<String>,

    /// Distribute plan items across child tasks.
    #[arg(long)]
    pub distribute_plan: bool,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch plan-append operation.
#[derive(Args)]
pub struct BatchPlanAppendArgs {
    /// Plan items to append (repeatable).
    #[arg(long = "plan-item", value_name = "ITEM", required = true)]
    pub plan_items: Vec<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for batch plan-prepend operation.
#[derive(Args)]
pub struct BatchPlanPrependArgs {
    /// Plan items to prepend (repeatable).
    #[arg(long = "plan-item", value_name = "ITEM", required = true)]
    pub plan_items: Vec<String>,

    #[command(flatten)]
    pub select: BatchSelectArgs,
}

/// Arguments for the batch command.
#[derive(Args)]
pub struct TaskBatchArgs {
    /// Batch operation type.
    #[command(subcommand)]
    pub operation: BatchOperation,

    /// Preview changes without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Continue on individual task failures (default: atomic/all-or-nothing).
    #[arg(long)]
    pub continue_on_error: bool,
}

#[derive(Args)]
#[command(
    about = "Create and build tasks from freeform requests",
    subcommand_required = false,
    after_long_help = "Common journeys:\n - Create a task:\n   ralph task \"Refactor queue parsing\"\n   ralph task build-refactor\n - Start work on a task:\n   ralph task ready RQ-0001\n   ralph task start RQ-0001\n - Complete a task:\n   ralph task status done RQ-0001\n   ralph task done --note \"Build checks pass\" RQ-0001\n - Split a task:\n   ralph task split RQ-0001\n   ralph task split --number 3 RQ-0001\n\nCommand intent sections:\nCreate and build: task, build, refactor, build-refactor\nLifecycle: show, ready, status, done, reject, start, schedule\nEdit: field, edit, update\nRelationships: clone, split, relate, blocks, mark-duplicate, children, parent\nBatch and templates: batch, template"
)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub command: Option<TaskCommand>,

    #[command(flatten)]
    pub build: TaskBuildArgs,
}

#[derive(Args)]
pub struct TaskBuildArgs {
    /// Freeform request text; if omitted, reads from stdin.
    #[arg(value_name = "REQUEST")]
    pub request: Vec<String>,

    /// Optional hint tags (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub tags: String,

    /// Optional hint scope (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub scope: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Template to use for pre-filling task fields (bug, feature, refactor, test, docs,
    /// add-tests, refactor-performance, fix-error-handling, add-docs, security-audit).
    #[arg(short = 't', long, value_name = "TEMPLATE")]
    pub template: Option<String>,

    /// Target file/path for template variable substitution ({{target}}, {{module}}, {{file}}).
    /// Used with --template to auto-fill template variables.
    #[arg(long, value_name = "PATH")]
    pub target: Option<String>,

    /// Fail on unknown template variables (default: warn only).
    /// When enabled, template loading fails if the template contains unknown {{variables}}.
    /// When disabled (default), unknown variables are left as-is with a warning.
    #[arg(long)]
    pub strict_templates: bool,
}

#[derive(Args)]
#[command(after_long_help = "Examples:
 ralph task build refactor
 ralph task build refactor --threshold 700
 ralph task build refactor --path crates/ralph/src/cli
 ralph task build refactor --dry-run --threshold 500
 ralph task build refactor --batch never
 ralph task build refactor --tags urgent,technical-debt")]
pub struct TaskBuildRefactorArgs {
    /// LOC threshold for flagging files as "large" (default: 1000).
    /// Files exceeding ~1000 LOC are presumed mis-scoped per AGENTS.md.
    #[arg(long, default_value = "1000")]
    pub threshold: usize,

    /// Directory to scan for Rust files (default: current directory / repo root).
    #[arg(long)]
    pub path: Option<std::path::PathBuf>,

    /// Preview tasks without inserting into queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Batching behavior for related files.
    /// - auto: Group files in same directory with similar names (default).
    /// - never: Create individual task per file.
    /// - aggressive: Group all files in same module.
    #[arg(long, value_enum, default_value = "auto")]
    pub batch: BatchMode,

    /// Additional tags to add to generated tasks (comma-separated).
    #[arg(long)]
    pub tags: Option<String>,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Fail on unknown template variables (default: warn only).
    /// When enabled, template loading fails if the template contains unknown {{variables}}.
    /// When disabled (default), unknown variables are left as-is with a warning.
    #[arg(long)]
    pub strict_templates: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n ralph task show RQ-0001\n ralph task show RQ-0001 --format compact"
)]
pub struct TaskShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Json)]
    pub format: QueueShowFormat,
}

#[derive(Args)]
pub struct TaskReadyArgs {
    /// Optional note to append when marking ready.
    #[arg(long)]
    pub note: Option<String>,

    /// Draft task ID to promote.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskStatusArg {
    /// Task is a draft and not ready to run.
    Draft,
    /// Task is waiting to be started.
    Todo,
    /// Task is currently being worked on.
    Doing,
    /// Task is complete (terminal, archived).
    Done,
    /// Task was rejected (terminal, archived).
    Rejected,
}

impl From<TaskStatusArg> for TaskStatus {
    fn from(value: TaskStatusArg) -> Self {
        match value {
            TaskStatusArg::Draft => TaskStatus::Draft,
            TaskStatusArg::Todo => TaskStatus::Todo,
            TaskStatusArg::Doing => TaskStatus::Doing,
            TaskStatusArg::Done => TaskStatus::Done,
            TaskStatusArg::Rejected => TaskStatus::Rejected,
        }
    }
}

#[derive(Args)]
pub struct TaskStatusArgs {
    /// Optional note to append.
    #[arg(long)]
    pub note: Option<String>,

    /// New status.
    #[arg(value_enum)]
    pub status: TaskStatusArg,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

#[derive(Args)]
pub struct TaskDoneArgs {
    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,

    /// Task ID to complete.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
#[command(
    about = "Mark a task as started (sets started_at and moves to doing)",
    after_long_help = "Examples:\n ralph task start RQ-0001\n ralph task start --reset RQ-0001"
)]
pub struct TaskStartArgs {
    /// Task ID to start.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Reset started_at even if already set.
    #[arg(long)]
    pub reset: bool,
}

#[derive(Args)]
pub struct TaskRejectArgs {
    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,

    /// Task ID to reject.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskFieldArgs {
    /// Custom field key (must not contain whitespace).
    pub key: String,

    /// Custom field value.
    pub value: String,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

#[derive(Args)]
pub struct TaskCloneArgs {
    /// Source task ID to clone.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Status for the cloned task (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to the cloned task title.
    #[arg(long)]
    pub title_prefix: Option<String>,

    /// Preview the clone without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task split RQ-0001\n  ralph task split --number 3 RQ-0001\n  ralph task split --status todo --number 2 RQ-0001\n  ralph task split --distribute-plan RQ-0001"
)]
pub struct TaskSplitArgs {
    /// Task ID to split.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Number of child tasks to create (default: 2, minimum: 2).
    #[arg(short = 'n', long, default_value = "2")]
    pub number: usize,

    /// Status for child tasks (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to child task titles.
    #[arg(long)]
    pub title_prefix: Option<String>,

    /// Distribute plan items across child tasks.
    #[arg(long)]
    pub distribute_plan: bool,

    /// Preview the split without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

/// Output format for task hierarchy commands (children, parent).
#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskRelationFormat {
    Compact,
    Long,
    Json,
}

#[derive(Args)]
#[command(
    about = "List child tasks (parent_id == TASK_ID)",
    after_long_help = "Examples:\n ralph task children RQ-0001\n ralph task children RQ-0001 --recursive\n ralph task children RQ-0001 --include-done\n ralph task children RQ-0001 --format json"
)]
pub struct TaskChildrenArgs {
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    #[arg(long)]
    pub include_done: bool,

    #[arg(long)]
    pub recursive: bool,

    #[arg(long, value_enum, default_value_t = TaskRelationFormat::Compact)]
    pub format: TaskRelationFormat,
}

#[derive(Args)]
#[command(
    about = "Show a task's parent (parent_id)",
    after_long_help = "Examples:\n ralph task parent RQ-0002\n ralph task parent RQ-0002 --include-done\n ralph task parent RQ-0002 --format json"
)]
pub struct TaskParentArgs {
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    #[arg(long)]
    pub include_done: bool,

    #[arg(long, value_enum, default_value_t = TaskRelationFormat::Compact)]
    pub format: TaskRelationFormat,
}

#[derive(Args)]
pub struct TaskEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value (empty string clears optional fields).
    pub value: String,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,

    /// Preview changes without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct TaskUpdateArgs {
    /// Fields to update (comma-separated, default: all).
    ///
    /// Valid fields: scope, evidence, plan, notes, tags, depends_on
    #[arg(long, default_value = "")]
    pub fields: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Task ID to update (omit to update all tasks).
    #[arg(value_name = "TASK_ID")]
    pub task_id: Option<String>,

    /// Preview changes without modifying the queue.
    ///
    /// For task update, this shows the prompt that would be sent to the runner.
    /// Actual changes depend on runner analysis of repository state.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskEditFieldArg {
    Title,
    Status,
    Priority,
    Tags,
    Scope,
    Evidence,
    Plan,
    Notes,
    Request,
    DependsOn,
    Blocks,
    RelatesTo,
    Duplicates,
    CustomFields,
    Agent,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
    StartedAt,
    ScheduledStart,
}

impl TaskEditFieldArg {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskEditFieldArg::Title => "title",
            TaskEditFieldArg::Status => "status",
            TaskEditFieldArg::Priority => "priority",
            TaskEditFieldArg::Tags => "tags",
            TaskEditFieldArg::Scope => "scope",
            TaskEditFieldArg::Evidence => "evidence",
            TaskEditFieldArg::Plan => "plan",
            TaskEditFieldArg::Notes => "notes",
            TaskEditFieldArg::Request => "request",
            TaskEditFieldArg::DependsOn => "depends_on",
            TaskEditFieldArg::Blocks => "blocks",
            TaskEditFieldArg::RelatesTo => "relates_to",
            TaskEditFieldArg::Duplicates => "duplicates",
            TaskEditFieldArg::CustomFields => "custom_fields",
            TaskEditFieldArg::Agent => "agent",
            TaskEditFieldArg::CreatedAt => "created_at",
            TaskEditFieldArg::UpdatedAt => "updated_at",
            TaskEditFieldArg::CompletedAt => "completed_at",
            TaskEditFieldArg::StartedAt => "started_at",
            TaskEditFieldArg::ScheduledStart => "scheduled_start",
        }
    }
}

impl From<TaskEditFieldArg> for TaskEditKey {
    fn from(value: TaskEditFieldArg) -> Self {
        match value {
            TaskEditFieldArg::Title => TaskEditKey::Title,
            TaskEditFieldArg::Status => TaskEditKey::Status,
            TaskEditFieldArg::Priority => TaskEditKey::Priority,
            TaskEditFieldArg::Tags => TaskEditKey::Tags,
            TaskEditFieldArg::Scope => TaskEditKey::Scope,
            TaskEditFieldArg::Evidence => TaskEditKey::Evidence,
            TaskEditFieldArg::Plan => TaskEditKey::Plan,
            TaskEditFieldArg::Notes => TaskEditKey::Notes,
            TaskEditFieldArg::Request => TaskEditKey::Request,
            TaskEditFieldArg::DependsOn => TaskEditKey::DependsOn,
            TaskEditFieldArg::Blocks => TaskEditKey::Blocks,
            TaskEditFieldArg::RelatesTo => TaskEditKey::RelatesTo,
            TaskEditFieldArg::Duplicates => TaskEditKey::Duplicates,
            TaskEditFieldArg::CustomFields => TaskEditKey::CustomFields,
            TaskEditFieldArg::Agent => TaskEditKey::Agent,
            TaskEditFieldArg::CreatedAt => TaskEditKey::CreatedAt,
            TaskEditFieldArg::UpdatedAt => TaskEditKey::UpdatedAt,
            TaskEditFieldArg::CompletedAt => TaskEditKey::CompletedAt,
            TaskEditFieldArg::StartedAt => TaskEditKey::StartedAt,
            TaskEditFieldArg::ScheduledStart => TaskEditKey::ScheduledStart,
        }
    }
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task schedule RQ-0001 '2026-02-01T09:00:00Z'\n  ralph task schedule RQ-0001 'tomorrow 9am'\n  ralph task schedule RQ-0001 'in 2 hours'\n  ralph task schedule RQ-0001 'next monday'\n  ralph task schedule RQ-0001 --clear"
)]
pub struct TaskScheduleArgs {
    /// Task ID to schedule.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Timestamp or relative time expression (e.g., 'tomorrow 9am', 'in 2 hours').
    #[arg(value_name = "WHEN")]
    pub when: Option<String>,

    /// Clear the scheduled start time.
    #[arg(long, conflicts_with = "when")]
    pub clear: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task relate RQ-0001 blocks RQ-0002\n  ralph task relate RQ-0001 relates_to RQ-0003\n  ralph task relate RQ-0001 duplicates RQ-0004"
)]
pub struct TaskRelateArgs {
    /// Source task ID.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Relationship type (blocks, relates_to, duplicates).
    #[arg(value_name = "RELATION")]
    pub relation: String,

    /// Target task ID.
    #[arg(value_name = "OTHER_TASK_ID")]
    pub other_task_id: String,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task blocks RQ-0001 RQ-0002\n  ralph task blocks RQ-0001 RQ-0002 RQ-0003"
)]
pub struct TaskBlocksArgs {
    /// Task that does the blocking.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Task(s) being blocked.
    #[arg(value_name = "BLOCKED_TASK_ID...")]
    pub blocked_task_ids: Vec<String>,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph task mark-duplicate RQ-0001 RQ-0002")]
pub struct TaskMarkDuplicateArgs {
    /// Task to mark as duplicate.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Original task this duplicates.
    #[arg(value_name = "ORIGINAL_TASK_ID")]
    pub original_task_id: String,
}

#[derive(Args)]
pub struct TaskTemplateArgs {
    #[command(subcommand)]
    pub command: TaskTemplateCommand,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum TaskTemplateCommand {
    /// List available task templates
    List,
    /// Show template details
    Show(TaskTemplateShowArgs),
    /// Build a task from a template
    Build(TaskTemplateBuildArgs),
}

#[derive(Args)]
pub struct TaskTemplateShowArgs {
    /// Template name (e.g., "bug", "feature")
    pub name: String,
}

#[derive(Args)]
pub struct TaskTemplateBuildArgs {
    /// Template name
    pub template: String,

    /// Target file/path for template variable substitution ({{target}}, {{module}}, {{file}}).
    /// Used to auto-fill template variables with context from the specified path.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Task title/request
    pub request: Vec<String>,

    /// Additional tags to merge
    #[arg(short, long)]
    pub tags: Option<String>,

    /// Additional scope to merge
    #[arg(short, long)]
    pub scope: Option<String>,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Fail on unknown template variables (default: warn only).
    /// When enabled, template loading fails if the template contains unknown {{variables}}.
    /// When disabled (default), unknown variables are left as-is with a warning.
    #[arg(long)]
    pub strict_templates: bool,
}

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Build a new task from a natural language request.
    #[command(
        next_help_heading = "Create and build",
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n - Override approval/sandbox/verbosity/plan-mode via flags.\n - Unsupported options follow --unsupported-option-policy.\n\nExamples:\n ralph task \"Add integration tests for run one\"\n ralph task --tags cli,rust \"Refactor queue parsing\"\n ralph task --scope crates/ralph \"Fix queue graph output\"\n ralph task --runner opencode --model gpt-5.2 \"Add docs for OpenCode setup\"\n ralph task --runner gemini --model gemini-3-flash-preview \"Draft risk checklist\"\n ralph task --runner pi --model gpt-5.2 \"Draft risk checklist\"\n ralph task --runner codex --model gpt-5.3-codex --effort high \"Fix queue validation\"\n ralph task --approval-mode auto-edits --runner claude \"Audit approvals\"\n ralph task --sandbox disabled --runner codex \"Audit sandbox\"\n ralph task --repo-prompt plan \"Audit error handling\"\n ralph task --repo-prompt off \"Quick typo fix\"\n echo \"Triage flaky CI\" | ralph task --runner codex --model gpt-5.3-codex --effort medium\n\nExplicit subcommand:\n ralph task build \"Add integration tests for run one\""
    )]
    Build(TaskBuildArgs),

    /// Automatically create refactoring tasks for large files.
    #[command(
        next_help_heading = "Create and build",
        alias = "ref",
        after_long_help = "Examples:\n ralph task refactor\n ralph task refactor --threshold 700\n ralph task refactor --path crates/ralph/src/cli\n ralph task refactor --dry-run --threshold 500\n ralph task refactor --batch never\n ralph task refactor --tags urgent,technical-debt\n ralph task ref --threshold 800"
    )]
    Refactor(TaskBuildRefactorArgs),

    /// Automatically create refactoring tasks for large files (alternative to 'refactor').
    #[command(
        next_help_heading = "Create and build",
        name = "build-refactor",
        after_long_help = "Alternative command name. Prefer 'ralph task refactor'.\n\nExamples:\n ralph task build-refactor\n ralph task build-refactor --threshold 700"
    )]
    BuildRefactor(TaskBuildRefactorArgs),

    /// Show a task by ID (queue + done).
    #[command(
        next_help_heading = "Lifecycle",
        alias = "details",
        after_long_help = "Examples:\n ralph task show RQ-0001\n ralph task details RQ-0001 --format compact"
    )]
    Show(TaskShowArgs),

    /// Promote a draft task to todo.
    #[command(
        next_help_heading = "Lifecycle",
        after_long_help = "Examples:\n ralph task ready RQ-0005\n ralph task ready --note \"Ready for implementation\" RQ-0005"
    )]
    Ready(TaskReadyArgs),

    /// Update a task's status (draft, todo, doing, done, rejected).
    ///
    /// Note: terminal statuses (done, rejected) complete and archive the task.
    #[command(
        next_help_heading = "Lifecycle",
        after_long_help = "Examples:\n ralph task status doing RQ-0001\n ralph task status doing --note \"Starting work\" RQ-0001\n ralph task status todo --note \"Back to backlog\" RQ-0001\n ralph task status done RQ-0001\n ralph task status rejected --note \"Invalid request\" RQ-0002"
    )]
    Status(TaskStatusArgs),

    /// Complete a task as done and move it to the done archive.
    #[command(
        next_help_heading = "Lifecycle",
        after_long_help = "Examples:\n ralph task done RQ-0001\n ralph task done --note \"Finished work\" --note \"make ci green\" RQ-0001"
    )]
    Done(TaskDoneArgs),

    /// Complete a task as rejected and move it to the done archive.
    #[command(
        next_help_heading = "Lifecycle",
        alias = "rejected",
        after_long_help = "Examples:\n ralph task reject RQ-0002\n ralph task reject --note \"No longer needed\" RQ-0002"
    )]
    Reject(TaskRejectArgs),

    /// Set a custom field on a task.
    #[command(
        next_help_heading = "Edit",
        after_long_help = "Examples:\n ralph task field severity high RQ-0001\n ralph task field complexity \"O(n log n)\" RQ-0002"
    )]
    Field(TaskFieldArgs),

    /// Edit any task field (default or custom).
    #[command(
        next_help_heading = "Edit",
        after_long_help = "Examples:\n ralph task edit title \"Clarify CLI edit\" RQ-0001\n ralph task edit status doing RQ-0001\n ralph task edit priority high RQ-0001\n ralph task edit tags \"cli, rust\" RQ-0001\n ralph task edit custom_fields \"severity=high, owner=ralph\" RQ-0001\n ralph task edit agent '{\"runner\":\"codex\",\"model\":\"gpt-5.3-codex\",\"phases\":2}' RQ-0001\n ralph task edit request \"\" RQ-0001\n ralph task edit completed_at \"2026-01-20T12:00:00Z\" RQ-0001\n ralph task edit --dry-run title \"Preview change\" RQ-0001"
    )]
    Edit(TaskEditArgs),

    /// Update existing task fields based on current repository state.
    #[command(
        next_help_heading = "Edit",
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n - Override approval/sandbox/verbosity/plan-mode via flags.\n - Unsupported options follow --unsupported-option-policy.\n\nField selection:\n - By default, all updatable fields are refreshed: scope, evidence, plan, notes, tags, depends_on.\n - Use --fields to specify which fields to update.\n\nTask selection:\n - Omit TASK_ID to update every task in the active queue.\n\nExamples:\n ralph task update\n ralph task update RQ-0001\n ralph task update --fields scope,evidence,plan RQ-0001\n ralph task update --runner opencode --model gpt-5.2 RQ-0001\n ralph task update --approval-mode auto-edits --runner claude RQ-0001\n ralph task update --repo-prompt plan RQ-0001\n ralph task update --repo-prompt off --fields scope,evidence RQ-0001\n ralph task update --fields tags RQ-0042\n ralph task update --dry-run RQ-0001"
    )]
    Update(TaskUpdateArgs),

    /// Manage task templates for common task types.
    #[command(
        next_help_heading = "Batch and templates",
        after_long_help = "Examples:\n ralph task template list\n ralph task template show bug\n ralph task template show add-tests\n ralph task template build bug \"Fix login timeout\"\n ralph task template build add-tests src/module.rs \"Add tests for module\"\n ralph task template build refactor-performance src/bottleneck.rs \"Optimize performance\"\n\nAvailable templates:\n - bug: Bug fix with reproduction steps and regression tests\n - feature: New feature with design, implementation, and documentation\n - refactor: Code refactoring with behavior preservation\n - test: Test addition or improvement\n - docs: Documentation update or creation\n - add-tests: Add tests for existing code with coverage verification\n - refactor-performance: Optimize performance with profiling and benchmarking\n - fix-error-handling: Fix error handling with proper types and context\n - add-docs: Add documentation for a specific file or module\n - security-audit: Security audit with vulnerability checks"
    )]
    Template(TaskTemplateArgs),

    /// Clone an existing task to create a new task from it.
    #[command(
        next_help_heading = "Relationships",
        alias = "duplicate",
        after_long_help = "Examples:\n ralph task clone RQ-0001\n ralph task clone RQ-0001 --status todo\n ralph task clone RQ-0001 --title-prefix \"[Follow-up] \"\n ralph task clone RQ-0001 --dry-run\n ralph task duplicate RQ-0001"
    )]
    Clone(TaskCloneArgs),

    /// Perform batch operations on multiple tasks efficiently.
    #[command(
        next_help_heading = "Batch and templates",
        after_long_help = "Examples:\n ralph task batch status doing RQ-0001 RQ-0002 RQ-0003\n ralph task batch status done --tag-filter ready\n ralph task batch field priority high --tag-filter urgent\n ralph task batch edit tags \"reviewed\" --tag-filter rust\n ralph task batch --dry-run status doing --tag-filter cli\n ralph task batch --continue-on-error status doing RQ-0001 RQ-0002 RQ-9999\n ralph task batch delete RQ-0001 RQ-0002\n ralph task batch delete --tag-filter stale --older-than 30d\n ralph task batch archive --tag-filter done --status-filter done\n ralph task batch clone --tag-filter template --status todo --title-prefix \"[Sprint] \"\n ralph task batch split --tag-filter epic --number 3 --distribute-plan\n ralph task batch plan-append --tag-filter rust --plan-item \"Run make ci\"\n ralph task batch plan-prepend RQ-0001 --plan-item \"Confirm repro\""
    )]
    Batch(TaskBatchArgs),

    /// Schedule a task to start after a specific time.
    #[command(
        next_help_heading = "Lifecycle",
        after_long_help = "Examples:\n ralph task schedule RQ-0001 '2026-02-01T09:00:00Z'\n ralph task schedule RQ-0001 'tomorrow 9am'\n ralph task schedule RQ-0001 'in 2 hours'\n ralph task schedule RQ-0001 'next monday'\n ralph task schedule RQ-0001 --clear"
    )]
    Schedule(TaskScheduleArgs),

    /// Add a relationship between tasks.
    #[command(
        next_help_heading = "Relationships",
        after_long_help = "Examples:\n ralph task relate RQ-0001 blocks RQ-0002\n ralph task relate RQ-0001 relates_to RQ-0003\n ralph task relate RQ-0001 duplicates RQ-0004"
    )]
    Relate(TaskRelateArgs),

    /// Mark a task as blocking another task (shorthand for 'relate <task> blocks <blocked>').
    #[command(
        next_help_heading = "Relationships",
        after_long_help = "Examples:\n ralph task blocks RQ-0001 RQ-0002\n ralph task blocks RQ-0001 RQ-0002 RQ-0003"
    )]
    Blocks(TaskBlocksArgs),

    /// Mark a task as a duplicate of another task (shorthand for 'relate <task> duplicates <original>').
    #[command(
        next_help_heading = "Relationships",
        name = "mark-duplicate",
        after_long_help = "Examples:\n ralph task mark-duplicate RQ-0001 RQ-0002"
    )]
    MarkDuplicate(TaskMarkDuplicateArgs),

    /// Split a task into multiple child tasks for better granularity.
    #[command(
        next_help_heading = "Relationships",
        after_long_help = "Examples:\n ralph task split RQ-0001\n ralph task split --number 3 RQ-0001\n ralph task split --status todo --number 2 RQ-0001\n ralph task split --distribute-plan RQ-0001"
    )]
    Split(TaskSplitArgs),

    /// Start work on a task (sets started_at and moves it to doing).
    #[command(
        next_help_heading = "Lifecycle",
        after_long_help = "Examples:\n ralph task start RQ-0001\n ralph task start --reset RQ-0001"
    )]
    Start(TaskStartArgs),

    /// List child tasks for a given task (based on parent_id).
    #[command(
        next_help_heading = "Relationships",
        after_long_help = "Examples:\n ralph task children RQ-0001\n ralph task children RQ-0001 --recursive\n ralph task children RQ-0001 --include-done\n ralph task children RQ-0001 --format json"
    )]
    Children(TaskChildrenArgs),

    /// Show the parent task for a given task (based on parent_id).
    #[command(
        next_help_heading = "Relationships",
        after_long_help = "Examples:\n ralph task parent RQ-0002\n ralph task parent RQ-0002 --include-done\n ralph task parent RQ-0002 --format json"
    )]
    Parent(TaskParentArgs),
}
