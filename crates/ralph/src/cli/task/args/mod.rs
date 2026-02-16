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

use clap::{Args, Subcommand};

// Submodules
mod batch;
mod build;
mod edit;
mod lifecycle;
mod relations;
mod template;
mod types;

// Re-exports for backward compatibility
pub use batch::{
    BatchEditArgs, BatchFieldArgs, BatchOperation, BatchSelectArgs, BatchStatusArgs, TaskBatchArgs,
};
pub use build::{TaskBuildArgs, TaskBuildRefactorArgs};
pub use edit::{TaskEditArgs, TaskFieldArgs, TaskUpdateArgs};
pub use lifecycle::{
    TaskDoneArgs, TaskReadyArgs, TaskRejectArgs, TaskScheduleArgs, TaskShowArgs, TaskStartArgs,
    TaskStatusArgs,
};
pub use relations::{
    TaskBlocksArgs, TaskChildrenArgs, TaskCloneArgs, TaskMarkDuplicateArgs, TaskParentArgs,
    TaskRelateArgs, TaskRelationFormat, TaskSplitArgs,
};
pub use template::{
    TaskFromArgs, TaskFromCommand, TaskFromTemplateArgs, TaskTemplateArgs, TaskTemplateBuildArgs,
    TaskTemplateCommand, TaskTemplateShowArgs,
};
pub use types::{BatchMode, TaskEditFieldArg, TaskStatusArg};

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

    /// Build a task from a template with variable substitution.
    ///
    /// This is a convenience command that combines template selection,
    /// variable substitution, and task creation in one step.
    #[command(
        name = "from",
        next_help_heading = "Batch and templates",
        after_long_help = "Examples:\n  ralph task from template bug --title \"Fix login timeout\"\n  ralph task from template feature --title \"Add dark mode\" --set target=src/ui/theme.rs\n  ralph task from template add-tests --title \"Test auth\" --set target=src/auth/mod.rs\n\nSee 'ralph task template list' for available templates."
    )]
    From(TaskFromArgs),
}
