//! Task command facade and shared task workflow exports.
//!
//! Purpose:
//! - Expose task build, update, decompose, and refactor command entrypoints through a thin facade.
//!
//! Responsibilities:
//! - Declare task submodules and re-export their canonical public/shared surfaces.
//! - Keep task command module boundaries aligned with the rest of the facade-style command tree.
//!
//! Non-scope:
//! - Task build, update, decomposition, or refactor implementation details.
//! - Request parsing, runner-setting resolution, or diff logic beyond re-exporting shared helpers.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Shared helper exports remain stable for task CLI/machine entrypoints and task submodules.
//! - Implementation logic lives in companion modules rather than this facade file.

mod build;
mod decompose;
mod diff;
mod refactor;
mod request;
mod settings;
mod types;
mod update;

pub use build::{build_task, build_task_created_tasks, build_task_without_lock};
pub use decompose::{
    DecompositionAttachTarget, DecompositionChildPolicy, DecompositionPlan, DecompositionPreview,
    DecompositionSource, PlannedNode, TaskDecomposeOptions, TaskDecomposeWriteResult,
    plan_task_decomposition, write_task_decomposition,
};
pub use diff::compare_task_fields;
pub use refactor::build_refactor_tasks;
pub use request::{read_request_from_args_or_reader, read_request_from_args_or_stdin};
pub(crate) use settings::{
    resolve_task_build_settings, resolve_task_runner_settings, resolve_task_update_settings,
};
pub use types::{
    BatchMode, TaskBuildOptions, TaskBuildOutputTarget, TaskBuildRefactorOptions,
    TaskUpdateSettings,
};
pub use update::{update_all_tasks, update_task, update_task_without_lock};

#[cfg(test)]
mod tests {
    use super::{
        TaskBuildOptions, TaskBuildOutputTarget, TaskUpdateSettings,
        read_request_from_args_or_reader, resolve_task_build_settings,
        resolve_task_update_settings,
    };
    use crate::contracts::{
        ClaudePermissionMode, Config, RunnerApprovalMode, RunnerCliConfigRoot,
        RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
        RunnerVerbosity, UnsupportedOptionPolicy,
    };
    use crate::{config, runner};
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
            .unwrap_or_else(|| PathBuf::from(".ralph/queue.jsonc"));
        let done_rel = config
            .queue
            .done_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/done.jsonc"));
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
                project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
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
            output: TaskBuildOutputTarget::Terminal,
            template_hint: None,
            template_target: None,
            strict_templates: false,
            estimated_minutes: None,
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
    fn task_build_output_target_maps_to_runner_settings() {
        assert_eq!(
            TaskBuildOutputTarget::Terminal.output_stream(),
            runner::OutputStream::Terminal
        );
        assert!(TaskBuildOutputTarget::Terminal.output_handler().is_none());

        assert_eq!(
            TaskBuildOutputTarget::Quiet.output_stream(),
            runner::OutputStream::HandlerOnly
        );
        assert!(TaskBuildOutputTarget::Quiet.output_handler().is_none());
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
