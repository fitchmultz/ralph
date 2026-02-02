//! Task-building and task-updating command helpers (request parsing, runner invocation, and queue updates).
//!
//! Responsibilities:
//! - Shared types and configuration for task operations (build, update, refactor).
//! - Parse task request inputs from CLI args or stdin.
//! - Runner settings resolution for task operations.
//! - JSON field comparison for task updates.
//!
//! Not handled here:
//! - Actual task building logic (see build.rs).
//! - Task update logic (see update.rs).
//! - Refactor task generation and LOC scanning (see refactor.rs).
//! - CLI argument definitions or command routing.
//! - Runner process implementation details or output parsing.
//! - Queue schema definitions or config persistence.
//!
//! Invariants/assumptions:
//! - Queue/done files are the source of truth for task ordering and status.
//! - Runner execution requires stream-json output for parsing.
//! - Permission/approval defaults come from config unless overridden at CLI.

use crate::contracts::{
    ClaudePermissionMode, Model, ReasoningEffort, Runner, RunnerCliOptionsPatch,
};
use crate::{config, runner};
use anyhow::{Context, Result, bail};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

mod build;
mod refactor;
mod update;

/// Batching mode for grouping related files in build-refactor.
#[derive(Clone, Copy, Debug)]
pub enum BatchMode {
    /// Group files in same directory with similar names (e.g., test files with source).
    Auto,
    /// Create individual task per file.
    Never,
    /// Group all files in same module/directory.
    Aggressive,
}

impl From<crate::cli::task::BatchMode> for BatchMode {
    fn from(mode: crate::cli::task::BatchMode) -> Self {
        match mode {
            crate::cli::task::BatchMode::Auto => BatchMode::Auto,
            crate::cli::task::BatchMode::Never => BatchMode::Never,
            crate::cli::task::BatchMode::Aggressive => BatchMode::Aggressive,
        }
    }
}

/// Options for the build-refactor command.
pub struct TaskBuildRefactorOptions {
    pub threshold: usize,
    pub path: Option<PathBuf>,
    pub dry_run: bool,
    pub batch: BatchMode,
    pub extra_tags: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
}

// TaskBuildOptions controls runner-driven task creation via .ralph/prompts/task_builder.md.
pub struct TaskBuildOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    /// Optional template name to use as a base for task fields
    pub template_hint: Option<String>,
    /// Optional target path for template variable substitution
    pub template_target: Option<String>,
    /// Fail on unknown template variables (default: false, warns only)
    pub strict_templates: bool,
}

// TaskUpdateSettings controls runner-driven task updates via .ralph/prompts/task_updater.md.
pub struct TaskUpdateSettings {
    pub fields: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskRunnerSettings {
    pub(crate) runner: Runner,
    pub(crate) model: Model,
    pub(crate) reasoning_effort: Option<ReasoningEffort>,
    pub(crate) runner_cli: runner::ResolvedRunnerCliOptions,
    pub(crate) permission_mode: Option<ClaudePermissionMode>,
}

pub(crate) fn resolve_task_runner_settings(
    resolved: &config::Resolved,
    runner_override: Option<Runner>,
    model_override: Option<Model>,
    reasoning_effort_override: Option<ReasoningEffort>,
    runner_cli_overrides: &RunnerCliOptionsPatch,
) -> Result<TaskRunnerSettings> {
    let settings = runner::resolve_agent_settings(
        runner_override,
        model_override,
        reasoning_effort_override,
        runner_cli_overrides,
        None,
        &resolved.config.agent,
    )?;

    Ok(TaskRunnerSettings {
        runner: settings.runner,
        model: settings.model,
        reasoning_effort: settings.reasoning_effort,
        runner_cli: settings.runner_cli,
        permission_mode: resolved.config.agent.claude_permission_mode,
    })
}

pub(crate) fn resolve_task_build_settings(
    resolved: &config::Resolved,
    opts: &TaskBuildOptions,
) -> Result<TaskRunnerSettings> {
    resolve_task_runner_settings(
        resolved,
        opts.runner_override,
        opts.model_override.clone(),
        opts.reasoning_effort_override,
        &opts.runner_cli_overrides,
    )
}

pub(crate) fn resolve_task_update_settings(
    resolved: &config::Resolved,
    settings: &TaskUpdateSettings,
) -> Result<TaskRunnerSettings> {
    resolve_task_runner_settings(
        resolved,
        settings.runner_override,
        settings.model_override.clone(),
        settings.reasoning_effort_override,
        &settings.runner_cli_overrides,
    )
}

pub fn read_request_from_args_or_reader(
    args: &[String],
    stdin_is_terminal: bool,
    mut reader: impl Read,
) -> Result<String> {
    if !args.is_empty() {
        let joined = args.join(" ");
        let trimmed = joined.trim();
        if trimmed.is_empty() {
            bail!(
                "Missing request: task requires a request description. Pass arguments or pipe input to the command."
            );
        }
        return Ok(trimmed.to_string());
    }

    if stdin_is_terminal {
        bail!(
            "Missing request: task requires a request description. Pass arguments or pipe input to the command."
        );
    }

    let mut buf = String::new();
    reader.read_to_string(&mut buf).context("read stdin")?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        bail!(
            "Missing request: task requires a request description (pass arguments or pipe input to the command)."
        );
    }
    Ok(trimmed.to_string())
}

// read_request_from_args_or_stdin joins any positional args, otherwise reads stdin.
pub fn read_request_from_args_or_stdin(args: &[String]) -> Result<String> {
    let stdin = std::io::stdin();
    let stdin_is_terminal = stdin.is_terminal();
    let handle = stdin.lock();
    read_request_from_args_or_reader(args, stdin_is_terminal, handle)
}

pub fn compare_task_fields(before: &str, after: &str) -> Result<Vec<String>> {
    let before_value: serde_json::Value = serde_json::from_str(before)?;
    let after_value: serde_json::Value = serde_json::from_str(after)?;

    if let (Some(before_obj), Some(after_obj)) = (before_value.as_object(), after_value.as_object())
    {
        let mut changed = Vec::new();
        for (key, after_val) in after_obj {
            if let Some(before_val) = before_obj.get(key) {
                if before_val != after_val {
                    changed.push(key.clone());
                }
            } else {
                changed.push(key.clone());
            }
        }
        Ok(changed)
    } else {
        Ok(vec!["task".to_string()])
    }
}

// Re-export public functions from submodules
pub use build::{build_task, build_task_without_lock};
pub use refactor::build_refactor_tasks;
pub use update::{update_all_tasks, update_task, update_task_without_lock};

#[cfg(test)]
mod tests {
    use super::{
        TaskBuildOptions, TaskUpdateSettings, read_request_from_args_or_reader,
        resolve_task_build_settings, resolve_task_update_settings,
    };
    use crate::config;
    use crate::contracts::{
        ClaudePermissionMode, Config, RunnerApprovalMode, RunnerCliConfigRoot,
        RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
        RunnerVerbosity, UnsupportedOptionPolicy,
    };
    use std::collections::BTreeMap;
    use std::io::Cursor;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn resolved_with_config(config: Config) -> (config::Resolved, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let repo_root = dir.path().to_path_buf();
        let queue_rel = config
            .queue
            .file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/queue.json"));
        let done_rel = config
            .queue
            .done_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/done.json"));
        let id_prefix = config
            .queue
            .id_prefix
            .clone()
            .unwrap_or_else(|| "RQ".to_string());
        let id_width = config.queue.id_width.unwrap_or(4) as usize;

        (
            config::Resolved {
                config,
                repo_root: repo_root.clone(),
                queue_path: repo_root.join(queue_rel),
                done_path: repo_root.join(done_rel),
                id_prefix,
                id_width,
                global_config_path: None,
                project_config_path: Some(repo_root.join(".ralph/config.json")),
            },
            dir,
        )
    }

    fn build_opts() -> TaskBuildOptions {
        TaskBuildOptions {
            request: "request".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
            strict_templates: false,
        }
    }

    fn update_settings() -> TaskUpdateSettings {
        TaskUpdateSettings {
            fields: "scope".to_string(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            dry_run: false,
        }
    }

    #[test]
    fn read_request_from_args_or_reader_rejects_empty_args_on_terminal() {
        let args: Vec<String> = vec![];
        let reader = Cursor::new("");
        let err = read_request_from_args_or_reader(&args, true, reader).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Missing request"));
        assert!(message.contains("Pass arguments"));
    }

    #[test]
    fn read_request_from_args_or_reader_reads_piped_input() {
        let args: Vec<String> = vec![];
        let reader = Cursor::new("  hello world  ");
        let value = read_request_from_args_or_reader(&args, false, reader).unwrap();
        assert_eq!(value, "hello world");
    }

    #[test]
    fn read_request_from_args_or_reader_rejects_empty_piped_input() {
        let args: Vec<String> = vec![];
        let reader = Cursor::new("   ");
        let err = read_request_from_args_or_reader(&args, false, reader).unwrap_err();
        assert!(err.to_string().contains("Missing request"));
    }

    #[test]
    fn task_build_respects_config_permission_mode_when_approval_default() {
        let mut config = Config::default();
        config.agent.claude_permission_mode = Some(ClaudePermissionMode::AcceptEdits);
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                verbosity: Some(RunnerVerbosity::Normal),
                approval_mode: Some(RunnerApprovalMode::Default),
                sandbox: Some(RunnerSandboxMode::Default),
                plan_mode: Some(RunnerPlanMode::Default),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Warn),
            },
            runners: BTreeMap::new(),
        });

        let (resolved, _dir) = resolved_with_config(config);
        let settings = resolve_task_build_settings(&resolved, &build_opts()).expect("settings");
        let effective = settings
            .runner_cli
            .effective_claude_permission_mode(settings.permission_mode);
        assert_eq!(effective, Some(ClaudePermissionMode::AcceptEdits));
    }

    #[test]
    fn task_update_cli_override_yolo_bypasses_permission_mode() {
        let mut config = Config::default();
        config.agent.claude_permission_mode = Some(ClaudePermissionMode::AcceptEdits);
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                verbosity: Some(RunnerVerbosity::Normal),
                approval_mode: Some(RunnerApprovalMode::Default),
                sandbox: Some(RunnerSandboxMode::Default),
                plan_mode: Some(RunnerPlanMode::Default),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Warn),
            },
            runners: BTreeMap::new(),
        });

        let mut settings = update_settings();
        settings.runner_cli_overrides = RunnerCliOptionsPatch {
            approval_mode: Some(RunnerApprovalMode::Yolo),
            ..RunnerCliOptionsPatch::default()
        };

        let (resolved, _dir) = resolved_with_config(config);
        let runner_settings = resolve_task_update_settings(&resolved, &settings).expect("settings");
        let effective = runner_settings
            .runner_cli
            .effective_claude_permission_mode(runner_settings.permission_mode);
        assert_eq!(effective, Some(ClaudePermissionMode::BypassPermissions));
    }

    #[test]
    fn task_build_fails_fast_when_safe_approval_requires_prompt() {
        let mut config = Config::default();
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                approval_mode: Some(RunnerApprovalMode::Safe),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
                ..RunnerCliOptionsPatch::default()
            },
            runners: BTreeMap::new(),
        });

        let (resolved, _dir) = resolved_with_config(config);
        let err = resolve_task_build_settings(&resolved, &build_opts()).expect_err("error");
        assert!(err.to_string().contains("approval_mode=safe"));
    }

    #[test]
    fn task_update_fails_fast_when_safe_approval_requires_prompt() {
        let mut config = Config::default();
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                approval_mode: Some(RunnerApprovalMode::Safe),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
                ..RunnerCliOptionsPatch::default()
            },
            runners: BTreeMap::new(),
        });

        let (resolved, _dir) = resolved_with_config(config);
        let err = resolve_task_update_settings(&resolved, &update_settings()).expect_err("error");
        assert!(err.to_string().contains("approval_mode=safe"));
    }
}
