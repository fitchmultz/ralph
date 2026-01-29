//! `ralph task ...` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap structures for task-related commands.
//! - Route task subcommands to queue operations and task building.
//!
//! Not handled here:
//! - Queue persistence details (see `crate::queue`).
//! - Locking semantics (see `crate::lock`).
//! - Runner execution internals.
//!
//! Invariants/assumptions:
//! - Callers resolve configuration before executing commands.
//! - Queue mutations are protected by locks when required.

use anyhow::{bail, Result};
use clap::{Args, Subcommand, ValueEnum};

use crate::cli::queue::{show_task, QueueShowFormat};
use crate::contracts::TaskStatus;
use crate::queue::TaskEditKey;
use crate::{agent, commands::task as task_cmd, completions, config, lock, queue, timeutil};

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
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
            queue::apply_task_edit(
                &mut queue_file,
                done_ref,
                &args.task_id,
                args.field.into(),
                &args.value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
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
                repo_prompt: args.repo_prompt,
                runner_cli: args.runner_cli.clone(),
            })?;

            let update_settings = task_cmd::TaskUpdateSettings {
                fields: fields_to_update,
                runner_override: overrides.runner,
                model_override: overrides.model,
                reasoning_effort_override: overrides.reasoning_effort,
                runner_cli_overrides: overrides.runner_cli,
                force,
                repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, &resolved),
            };

            match args.task_id.as_deref() {
                Some(task_id) => task_cmd::update_task(&resolved, task_id, &update_settings),
                None => task_cmd::update_all_tasks(&resolved, &update_settings),
            }
        }

        Some(TaskCommand::Build(args)) => {
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                repo_prompt: args.repo_prompt,
                runner_cli: args.runner_cli.clone(),
            })?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.repo_prompt,
                        &resolved,
                    ),
                },
            )
        }

        Some(TaskCommand::Show(args)) => show_task(&resolved, &args.task_id, args.format),

        None => {
            let args = args.build;
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                repo_prompt: args.repo_prompt,
                runner_cli: args.runner_cli.clone(),
            })?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.repo_prompt,
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
    let lock_dir = lock::queue_lock_dir(&resolved.repo_root);
    if lock::is_supervising_process(&lock_dir)? {
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
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        notes,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
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
    after_long_help = "Examples:\n ralph task \"Add tests for the new queue logic\"\n ralph task --runner opencode --model gpt-5.2 \"Fix CLI help strings\"\n ralph task show RQ-0001\n ralph task show RQ-0001 --format compact\n ralph task ready RQ-0005\n ralph task status doing --note \"Starting work\" RQ-0001\n ralph task update\n ralph task update RQ-0001\n ralph task update --fields scope,evidence RQ-0001\n ralph task edit title \"Refine queue edit\" RQ-0001\n ralph task field severity high RQ-0003\n ralph task done --note \"Finished work\" RQ-0001\n ralph task reject --note \"No longer needed\" RQ-0002\n ralph task build \"(explicit build subcommand still works)\""
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
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n - Override approval/sandbox/verbosity/plan-mode via flags.\n - Unsupported options follow --unsupported-option-policy.\n\nExamples:\n ralph task \"Add integration tests for run one\"\n ralph task --tags cli,rust \"Refactor queue parsing\"\n ralph task --scope crates/ralph \"Fix TUI rendering bug\"\n ralph task --runner opencode --model gpt-5.2 \"Add docs for OpenCode setup\"\n ralph task --runner gemini --model gemini-3-flash-preview \"Draft risk checklist\"\n ralph task --runner codex --model gpt-5.2-codex --effort high \"Fix queue validation\"\n ralph task --approval-mode auto-edits --runner claude \"Audit approvals\"\n ralph task --sandbox disabled --runner codex \"Audit sandbox\"\n ralph task --repo-prompt plan \"Audit error handling\"\n ralph task --repo-prompt off \"Quick typo fix\"\n echo \"Triage flaky CI\" | ralph task --runner codex --model gpt-5.2-codex --effort medium\n\nExplicit subcommand:\n ralph task build \"Add integration tests for run one\""
    )]
    Build(TaskBuildArgs),

    /// Show a task by ID (queue + done).
    #[command(
        alias = "details",
        after_long_help = "Examples:\n ralph task show RQ-0001\n ralph task details RQ-0001 --format compact"
    )]
    Show(TaskShowArgs),

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
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n - Override approval/sandbox/verbosity/plan-mode via flags.\n - Unsupported options follow --unsupported-option-policy.\n\nField selection:\n - By default, all updatable fields are refreshed: scope, evidence, plan, notes, tags, depends_on.\n - Use --fields to specify which fields to update.\n\nTask selection:\n - Omit TASK_ID to update every task in the active queue.\n\nExamples:\n ralph task update\n ralph task update RQ-0001\n ralph task update --fields scope,evidence,plan RQ-0001\n ralph task update --runner opencode --model gpt-5.2 RQ-0001\n ralph task update --approval-mode auto-edits --runner claude RQ-0001\n ralph task update --repo-prompt plan RQ-0001\n ralph task update --repo-prompt off --fields scope,evidence RQ-0001\n ralph task update --fields tags RQ-0042"
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
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,
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

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::queue::QueueShowFormat;
    use crate::cli::Cli;

    #[test]
    fn task_update_help_mentions_rp_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let update = task
            .find_subcommand_mut("update")
            .expect("task update subcommand");
        let help = update.render_long_help().to_string();

        assert!(
            help.contains("ralph task update --repo-prompt plan RQ-0001"),
            "missing repo-prompt plan example: {help}"
        );
        assert!(
            help.contains("ralph task update --repo-prompt off --fields scope,evidence RQ-0001"),
            "missing repo-prompt off example: {help}"
        );
        assert!(
            help.contains("ralph task update --approval-mode auto-edits --runner claude RQ-0001"),
            "missing approval-mode example: {help}"
        );
    }

    #[test]
    fn task_show_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let show = task
            .find_subcommand_mut("show")
            .expect("task show subcommand");
        let help = show.render_long_help().to_string();

        assert!(
            help.contains("ralph task show RQ-0001"),
            "missing show example: {help}"
        );
        assert!(
            help.contains("--format compact"),
            "missing format example: {help}"
        );
    }

    #[test]
    fn task_details_alias_parses() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "details", "RQ-0001", "--format", "compact"])
                .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Show(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert_eq!(args.format, QueueShowFormat::Compact);
                }
                _ => panic!("expected task show command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "build",
            "--repo-prompt",
            "plan",
            "-e",
            "high",
            "Add tests",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Build(args)) => {
                    assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Plan));
                    assert_eq!(args.effort.as_deref(), Some("high"));
                }
                _ => panic!("expected task build command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "build",
            "--approval-mode",
            "yolo",
            "--sandbox",
            "disabled",
            "Add tests",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Build(args)) => {
                    assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("yolo"));
                    assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
                }
                _ => panic!("expected task build command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "update",
            "--repo-prompt",
            "off",
            "-e",
            "low",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Off));
                    assert_eq!(args.effort.as_deref(), Some("low"));
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "update",
            "--approval-mode",
            "auto-edits",
            "--sandbox",
            "disabled",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("auto-edits"));
                    assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }
}
