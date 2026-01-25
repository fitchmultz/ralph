//! `ralph task ...` command group: Clap types and handler.

use anyhow::{bail, Result};
use clap::{Args, Subcommand, ValueEnum};

use crate::contracts::TaskStatus;
use crate::queue::TaskEditKey;
use crate::{agent, completions, config, fsutil, queue, runner, task_cmd, timeutil};

pub fn handle_task(args: TaskArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        Some(TaskCommand::Ready(args)) => {
            let _queue_lock =
                crate::queue::acquire_queue_lock(&resolved.repo_root, "task ready", force)?;
            let mut queue_file = crate::queue::load_queue(&resolved.queue_path)?;
            let now = crate::timeutil::now_utc_rfc3339()?;
            crate::queue::promote_draft_to_todo(
                &mut queue_file,
                &args.task_id,
                &now,
                args.note.as_deref(),
            )?;
            crate::queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!("Task {} marked ready (draft -> todo).", args.task_id);
            Ok(())
        }

        Some(TaskCommand::Status(args)) => {
            let status: TaskStatus = args.status.into();

            match status {
                TaskStatus::Done => complete_task_or_signal(
                    &resolved,
                    &args.task_id,
                    TaskStatus::Done,
                    &[],
                    force,
                    "task done",
                ),
                TaskStatus::Rejected => complete_task_or_signal(
                    &resolved,
                    &args.task_id,
                    TaskStatus::Rejected,
                    &[],
                    force,
                    "task reject",
                ),
                TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task status", force)?;
                    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
                    let now = timeutil::now_utc_rfc3339()?;
                    queue::set_status(
                        &mut queue_file,
                        &args.task_id,
                        status,
                        &now,
                        args.note.as_deref(),
                    )?;
                    queue::save_queue(&resolved.queue_path, &queue_file)?;
                    log::info!("Updated task {} to status {}.", args.task_id, status);
                    Ok(())
                }
            }
        }

        Some(TaskCommand::Done(args)) => complete_task_or_signal(
            &resolved,
            &args.task_id,
            TaskStatus::Done,
            &args.note,
            force,
            "task done",
        ),

        Some(TaskCommand::Reject(args)) => complete_task_or_signal(
            &resolved,
            &args.task_id,
            TaskStatus::Rejected,
            &args.note,
            force,
            "task reject",
        ),

        Some(TaskCommand::Field(args)) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task field", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_field(&mut queue_file, &args.task_id, &args.key, &args.value, &now)?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!("Set field '{}' on task {}.", args.key, args.task_id);
            Ok(())
        }

        Some(TaskCommand::Edit(args)) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task edit", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let done_file = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done_file)
            };
            let now = timeutil::now_utc_rfc3339()?;
            queue::apply_task_edit(
                &mut queue_file,
                done_ref,
                &args.task_id,
                args.field.into(),
                &args.value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
            )?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!(
                "Updated task {} field {}.",
                args.task_id,
                args.field.as_str()
            );
            Ok(())
        }

        Some(TaskCommand::Update(args)) => {
            let _queue_lock =
                crate::queue::acquire_queue_lock(&resolved.repo_root, "task update", force)?;

            let queue_file = crate::queue::load_queue(&resolved.queue_path)?;
            if !queue_file
                .tasks
                .iter()
                .any(|t| t.id.trim() == args.task_id.trim())
            {
                bail!("Task not found: {}", args.task_id);
            }

            let valid_fields = ["scope", "evidence", "plan", "notes", "tags", "depends_on"];
            let fields_to_update = if args.fields.trim().is_empty() || args.fields.trim() == "all" {
                "scope,evidence,plan,notes,tags,depends_on".to_string()
            } else {
                for field in args.fields.split(',') {
                    if !valid_fields.contains(&field.trim()) {
                        bail!(
                            "Invalid field '{}'. Valid fields: {}",
                            field,
                            valid_fields.join(", ")
                        );
                    }
                }
                args.fields.clone()
            };

            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                rp_on: args.rp_on,
                rp_off: args.rp_off,
            })?;
            let settings = runner::resolve_agent_settings(
                overrides.runner,
                overrides.model,
                overrides.reasoning_effort,
                None,
                &resolved.config.agent,
            )?;

            task_cmd::update_task(
                &resolved,
                task_cmd::TaskUpdateOptions {
                    task_id: args.task_id.clone(),
                    fields: fields_to_update,
                    runner: settings.runner,
                    model: settings.model,
                    reasoning_effort: settings.reasoning_effort,
                    force,
                    repoprompt_required: agent::resolve_rp_required(
                        args.rp_on,
                        args.rp_off,
                        &resolved,
                    ),
                },
            )
        }

        Some(TaskCommand::Build(args)) => {
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                rp_on: args.rp_on,
                rp_off: args.rp_off,
            })?;
            let settings = runner::resolve_agent_settings(
                overrides.runner,
                overrides.model,
                overrides.reasoning_effort,
                None,
                &resolved.config.agent,
            )?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner: settings.runner,
                    model: settings.model,
                    reasoning_effort: settings.reasoning_effort,
                    force,
                    repoprompt_required: agent::resolve_rp_required(
                        args.rp_on,
                        args.rp_off,
                        &resolved,
                    ),
                },
            )
        }

        None => {
            let args = args.build;
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                rp_on: args.rp_on,
                rp_off: args.rp_off,
            })?;
            let settings = runner::resolve_agent_settings(
                overrides.runner,
                overrides.model,
                overrides.reasoning_effort,
                None,
                &resolved.config.agent,
            )?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner: settings.runner,
                    model: settings.model,
                    reasoning_effort: settings.reasoning_effort,
                    force,
                    repoprompt_required: agent::resolve_rp_required(
                        args.rp_on,
                        args.rp_off,
                        &resolved,
                    ),
                },
            )
        }
    }
}

fn complete_task_or_signal(
    resolved: &config::Resolved,
    task_id: &str,
    status: TaskStatus,
    notes: &[String],
    force: bool,
    lock_label: &str,
) -> Result<()> {
    let lock_dir = fsutil::queue_lock_dir(&resolved.repo_root);
    if fsutil::is_supervising_process(&lock_dir)? {
        let signal = completions::CompletionSignal {
            task_id: task_id.to_string(),
            status,
            notes: notes.to_vec(),
        };
        let path = completions::write_completion_signal(&resolved.repo_root, &signal)?;
        log::info!(
            "Running under supervision - wrote completion signal at {}",
            path.display()
        );
        return Ok(());
    }

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, lock_label, force)?;
    let now = timeutil::now_utc_rfc3339()?;
    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        notes,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    log::info!(
        "Task {} completed (status: {}) and moved to done archive.",
        task_id,
        status
    );
    Ok(())
}

#[derive(Args)]
#[command(
    about = "Create and build tasks from freeform requests",
    subcommand_required = false,
    after_long_help = "Examples:\n ralph task \"Add tests for the new queue logic\"\n ralph task --runner opencode --model gpt-5.2 \"Fix CLI help strings\"\n ralph task ready RQ-0005\n ralph task status doing --note \"Starting work\" RQ-0001\n ralph task update RQ-0001\n ralph task update --fields scope,evidence RQ-0001\n ralph task edit title \"Refine queue edit\" RQ-0001\n ralph task field severity high RQ-0003\n ralph task done --note \"Finished work\" RQ-0001\n ralph task reject --note \"No longer needed\" RQ-0002\n ralph task build \"(explicit build subcommand still works)\""
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
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nExamples:\n ralph task \"Add integration tests for run one\"\n ralph task --tags cli,rust \"Refactor queue parsing\"\n ralph task --scope crates/ralph \"Fix TUI rendering bug\"\n ralph task --runner opencode --model gpt-5.2 \"Add docs for OpenCode setup\"\n ralph task --runner gemini --model gemini-3-flash-preview \"Draft risk checklist\"\n ralph task --runner codex --model gpt-5.2-codex --effort high \"Fix queue validation\"\n ralph task --rp-on \"Audit error handling\"\n ralph task --rp-off \"Quick typo fix\"\n echo \"Triage flaky CI\" | ralph task --runner codex --model gpt-5.2-codex --effort medium\n\nExplicit subcommand:\n ralph task build \"Add integration tests for run one\""
    )]
    Build(TaskBuildArgs),

    /// Promote a draft task to todo.
    #[command(
        after_long_help = "Examples:\n ralph task ready RQ-0005\n ralph task ready --note \"Ready for implementation\" RQ-0005"
    )]
    Ready(TaskReadyArgs),

    /// Update a task's status (draft, todo, doing, done, rejected).
    ///
    /// Note: terminal statuses (done, rejected) complete and archive the task.
    #[command(
        after_long_help = "Examples:\n ralph task status doing RQ-0001\n ralph task status doing --note \"Starting work\" RQ-0001\n ralph task status todo --note \"Back to backlog\" RQ-0001\n ralph task status done RQ-0001\n ralph task status rejected --note \"Invalid request\" RQ-0002"
    )]
    Status(TaskStatusArgs),

    /// Complete a task as done and move it to the done archive.
    #[command(
        after_long_help = "Examples:\n ralph task done RQ-0001\n ralph task done --note \"Finished work\" --note \"make ci green\" RQ-0001"
    )]
    Done(TaskDoneArgs),

    /// Complete a task as rejected and move it to the done archive.
    #[command(
        alias = "rejected",
        after_long_help = "Examples:\n ralph task reject RQ-0002\n ralph task reject --note \"No longer needed\" RQ-0002"
    )]
    Reject(TaskRejectArgs),

    /// Set a custom field on a task.
    #[command(
        after_long_help = "Examples:\n ralph task field severity high RQ-0001\n ralph task field complexity \"O(n log n)\" RQ-0002"
    )]
    Field(TaskFieldArgs),

    /// Edit any task field (default or custom).
    #[command(
        after_long_help = "Examples:\n ralph task edit title \"Clarify CLI edit\" RQ-0001\n ralph task edit status doing RQ-0001\n ralph task edit priority high RQ-0001\n ralph task edit tags \"cli, rust\" RQ-0001\n ralph task edit custom_fields \"severity=high, owner=ralph\" RQ-0001\n ralph task edit request \"\" RQ-0001\n ralph task edit completed_at \"2026-01-20T12:00:00Z\" RQ-0001"
    )]
    Edit(TaskEditArgs),

    /// Update existing task fields based on current repository state.
    #[command(
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nField selection:\n - By default, all updatable fields are refreshed: scope, evidence, plan, notes, tags, depends_on.\n - Use --fields to specify which fields to update.\n\nExamples:\n ralph task update RQ-0001\n ralph task update --fields scope,evidence,plan RQ-0001\n ralph task update --runner opencode --model gpt-5.2 RQ-0001\n ralph task update --fields tags RQ-0042"
    )]
    Update(TaskUpdateArgs),
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
    #[arg(long)]
    pub effort: Option<String>,

    /// Force RepoPrompt required (must use context_builder).
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,
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

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
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

    /// Task ID to update.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
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

    /// Task ID to update.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value (empty string clears optional fields).
    pub value: String,

    /// Task ID to update.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
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
    #[arg(long)]
    pub effort: Option<String>,

    /// Force RepoPrompt required (must use context_builder).
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,

    /// Task ID to update.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
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
    CustomFields,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
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
            TaskEditFieldArg::CustomFields => "custom_fields",
            TaskEditFieldArg::CreatedAt => "created_at",
            TaskEditFieldArg::UpdatedAt => "updated_at",
            TaskEditFieldArg::CompletedAt => "completed_at",
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
            TaskEditFieldArg::CustomFields => TaskEditKey::CustomFields,
            TaskEditFieldArg::CreatedAt => TaskEditKey::CreatedAt,
            TaskEditFieldArg::UpdatedAt => TaskEditKey::UpdatedAt,
            TaskEditFieldArg::CompletedAt => TaskEditKey::CompletedAt,
        }
    }
}
