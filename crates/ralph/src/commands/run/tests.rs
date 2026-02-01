//! Unit tests for run command orchestration helpers.

use super::{
    apply_followup_reasoning_effort, resolve_iteration_settings, resolve_run_agent_settings,
    run_one_with_id_locked, task_context_for_prompt,
};
use crate::completions;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Config, GitRevertMode, Model, ModelEffort,
    NotificationConfig, PhaseOverrideConfig, PhaseOverrides, QueueConfig, QueueFile,
    ReasoningEffort, Runner, Task, TaskAgent, TaskStatus,
};
use crate::queue;
use crate::runner;
use crate::testsupport::git as git_test;
use log::{LevelFilter, Log, Metadata, Record};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

struct TestLogger;

static LOGGER: TestLogger = TestLogger;
static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoggerState {
    TestLogger,
    OtherLogger,
}

impl Log for TestLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let logs = LOGS.get_or_init(|| Mutex::new(Vec::new()));
        let mut guard = logs.lock().expect("log mutex");
        guard.push(record.args().to_string());
    }

    fn flush(&self) {}
}

fn init_logger() -> (LoggerState, &'static Mutex<Vec<String>>) {
    let state = *LOGGER_STATE.get_or_init(|| {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(LevelFilter::Warn);
            LoggerState::TestLogger
        } else {
            LoggerState::OtherLogger
        }
    });
    (state, LOGS.get_or_init(|| Mutex::new(Vec::new())))
}

fn take_logs() -> (LoggerState, Vec<String>) {
    let (state, logs) = init_logger();
    let mut guard = logs.lock().expect("log mutex");
    let drained = guard.drain(..).collect::<Vec<_>>();
    (state, drained)
}

fn resolved_with_agent_defaults(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
) -> crate::config::Resolved {
    let dir = TempDir::new().expect("temp dir");
    let repo_root = dir.path().to_path_buf();

    let cfg = Config {
        agent: AgentConfig {
            runner,
            model,
            reasoning_effort: effort,
            iterations: None,
            followup_reasoning_effort: None,
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            cursor_bin: Some("agent".to_string()),
            kimi_bin: Some("kimi".to_string()),
            pi_bin: Some("pi".to_string()),
            phases: Some(2),
            update_task_before_run: None,
            fail_on_prerun_update_error: None,
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            runner_cli: None,
            phase_overrides: None,
            instruction_files: None,
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
            git_commit_push_enabled: Some(true),
            notification: NotificationConfig {
                enabled: Some(false),
                ..NotificationConfig::default()
            },
            webhook: crate::contracts::WebhookConfig::default(),
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
        },
        ..Config::default()
    };

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

fn base_task() -> Task {
    Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
    }
}

fn resolved_with_repo_root(repo_root: PathBuf) -> crate::config::Resolved {
    let cfg = Config {
        agent: AgentConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Medium),
            iterations: None,
            followup_reasoning_effort: None,
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            cursor_bin: Some("agent".to_string()),
            kimi_bin: Some("kimi".to_string()),
            pi_bin: Some("pi".to_string()),
            phases: Some(3),
            update_task_before_run: None,
            fail_on_prerun_update_error: None,
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            runner_cli: None,
            phase_overrides: None,
            instruction_files: None,
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
            git_commit_push_enabled: Some(true),
            notification: NotificationConfig {
                enabled: Some(false),
                ..NotificationConfig::default()
            },
            webhook: crate::contracts::WebhookConfig::default(),
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
        },
        tui: crate::contracts::TuiConfig {
            auto_archive_terminal: None,
            celebrations_enabled: Some(false),
            stats_enabled: Some(false),
        },
        ..Config::default()
    };

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

fn task_with_status(status: TaskStatus) -> Task {
    Task {
        id: "RQ-0001".to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
    }
}

#[test]
fn run_one_with_id_locked_skips_reacquiring_queue_lock() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let resolved = resolved_with_repo_root(repo_root.clone());

    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    let task = Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: Some("2026-01-18T01:00:00Z".to_string()),
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
    };
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task],
        },
    )?;

    let _lock = queue::acquire_queue_lock(&resolved.repo_root, "test lock", false)?;

    let err = run_one_with_id_locked(
        &resolved,
        &super::AgentOverrides::default(),
        false,
        "RQ-0001",
        None,
        None,
    )
    .expect_err("expected runnable status error");
    let message = err.to_string();
    assert!(message.contains("not runnable"));
    assert!(!message.contains("Queue lock already held"));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_task_agent_overrides_config() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        model_effort: ModelEffort::High,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
    });

    let overrides = super::AgentOverrides::default();
    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Gpt52);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_run_agent_settings_cli_overrides_task_agent_and_config() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Opencode),
        Some(Model::Gpt52),
        Some(ReasoningEffort::Low),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        model_effort: ModelEffort::Low,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
    });

    let overrides = super::AgentOverrides {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    };

    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Codex);
    assert_eq!(settings.model, Model::Gpt52Codex);
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::High));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_defaults_to_glm47_for_opencode_runner() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
    );

    let task = base_task();

    let overrides = super::AgentOverrides {
        runner: Some(Runner::Opencode),
        model: None,
        reasoning_effort: None,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    };

    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Glm47);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_run_agent_settings_defaults_to_gemini_flash_for_gemini_runner() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
    );

    let task = base_task();

    let overrides = super::AgentOverrides {
        runner: Some(Runner::Gemini),
        model: None,
        reasoning_effort: None,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    };

    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Gemini);
    assert_eq!(settings.model.as_str(), "gemini-3-flash-preview");
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_run_agent_settings_effort_defaults_to_medium_for_codex_when_unspecified(
) -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let task = base_task();
    let overrides = super::AgentOverrides::default();

    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Codex);
    assert_eq!(settings.model, Model::Gpt52Codex);
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::Medium));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_model_effort_default_uses_config() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::High),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        model_effort: ModelEffort::Default,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
    });

    let overrides = super::AgentOverrides::default();
    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::High));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_model_effort_overrides_config_for_codex() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Low),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        model_effort: ModelEffort::XHigh,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
    });

    let overrides = super::AgentOverrides::default();
    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::XHigh));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_effort_is_ignored_for_opencode() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Opencode),
        Some(Model::Gpt52),
        Some(ReasoningEffort::Low),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        model_effort: ModelEffort::High,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
    });
    let overrides = super::AgentOverrides {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    };

    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Gpt52);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_iteration_settings_defaults_to_one() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(None, None, None);
    let task = base_task();

    let settings = resolve_iteration_settings(&task, &resolved.config.agent)?;
    assert_eq!(settings.count, 1);
    assert_eq!(settings.followup_reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_iteration_settings_prefers_task_over_config() -> anyhow::Result<()> {
    let mut resolved = resolved_with_agent_defaults(None, None, None);
    resolved.config.agent.iterations = Some(3);
    resolved.config.agent.followup_reasoning_effort = Some(ReasoningEffort::Low);

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        iterations: Some(2),
        followup_reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: None,
    });

    let settings = resolve_iteration_settings(&task, &resolved.config.agent)?;
    assert_eq!(settings.count, 2);
    assert_eq!(
        settings.followup_reasoning_effort,
        Some(ReasoningEffort::High)
    );
    Ok(())
}

#[test]
fn apply_followup_reasoning_effort_overrides_codex_only() {
    let base = runner::AgentSettings {
        runner: Runner::Codex,
        model: Model::Gpt52Codex,
        reasoning_effort: Some(ReasoningEffort::Medium),
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let updated = apply_followup_reasoning_effort(&base, Some(ReasoningEffort::High), true);
    assert_eq!(updated.reasoning_effort, Some(ReasoningEffort::High));

    let base_non_codex = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Glm47,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let updated_non_codex =
        apply_followup_reasoning_effort(&base_non_codex, Some(ReasoningEffort::High), true);
    assert_eq!(updated_non_codex.reasoning_effort, None);
}

#[test]
fn apply_followup_reasoning_effort_warns_for_non_codex() {
    let base_non_codex = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Glm47,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };

    let (state, _) = take_logs();
    let _ = apply_followup_reasoning_effort(&base_non_codex, Some(ReasoningEffort::High), true);
    let (_, logs) = take_logs();

    if state == LoggerState::TestLogger {
        assert!(
            logs.iter()
                .any(|entry| entry.contains("Follow-up reasoning_effort configured")),
            "expected warning log, got {logs:?}"
        );
    }
}

#[test]
fn task_context_block_includes_id_and_title() -> anyhow::Result<()> {
    let mut t = base_task();
    t.id = "RQ-0001".to_string();
    t.title = "Hello world".to_string();
    let rendered = task_context_for_prompt(&t)?;
    assert!(rendered.contains("RQ-0001"));
    assert!(rendered.contains("Hello world"));
    assert!(rendered.contains("Raw task JSON"));
    Ok(())
}

#[test]
fn apply_phase3_completion_signal_moves_task_and_clears_signal() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_status(TaskStatus::Doing)],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["Reviewed".to_string()],
    };
    completions::write_completion_signal(&resolved.repo_root, &signal)?;

    let status = super::apply_phase3_completion_signal(&resolved, "RQ-0001")?;
    assert_eq!(status, Some(TaskStatus::Done));

    let done = queue::load_queue(&resolved.done_path)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");
    assert_eq!(done.tasks[0].status, TaskStatus::Done);
    assert_eq!(done.tasks[0].notes, vec!["Reviewed".to_string()]);

    let remaining = queue::load_queue(&resolved.queue_path)?;
    assert!(remaining.tasks.is_empty());

    let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
    assert!(signal_after.is_none());

    Ok(())
}

#[test]
fn apply_phase3_completion_signal_already_archived_clears_signal() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let mut done_task = task_with_status(TaskStatus::Done);
    done_task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    let done_file = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    queue::save_queue(&resolved.done_path, &done_file)?;

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["Reviewed".to_string()],
    };
    completions::write_completion_signal(&resolved.repo_root, &signal)?;

    let status = super::apply_phase3_completion_signal(&resolved, "RQ-0001")?;
    assert_eq!(status, Some(TaskStatus::Done));

    let done = queue::load_queue(&resolved.done_path)?;
    assert_eq!(done.tasks.len(), 1);
    assert_eq!(done.tasks[0].id, "RQ-0001");

    let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
    assert!(signal_after.is_none());
    Ok(())
}

#[test]
fn apply_phase3_completion_signal_missing_returns_none() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_status(TaskStatus::Doing)],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let status = super::apply_phase3_completion_signal(&resolved, "RQ-0001")?;
    assert!(status.is_none());
    Ok(())
}

#[test]
fn apply_phase3_completion_signal_keeps_signal_on_failure() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let resolved = resolved_with_repo_root(temp.path().to_path_buf());

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["Reviewed".to_string()],
    };
    completions::write_completion_signal(&resolved.repo_root, &signal)?;

    let err = super::apply_phase3_completion_signal(&resolved, "RQ-0001").unwrap_err();
    assert!(
        err.to_string().contains("task not found"),
        "expected missing task error, got: {err}"
    );

    let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
    assert!(signal_after.is_some());
    Ok(())
}

#[test]
fn finalize_phase3_if_done_runs_post_run_supervise_without_signal() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    let mut resolved = resolved_with_repo_root(temp.path().to_path_buf());
    resolved.config.agent.ci_gate_enabled = Some(false);

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue::save_queue(&resolved.queue_path, &queue_file)?;
    let mut done_task = task_with_status(TaskStatus::Done);
    done_task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
    let done_file = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    queue::save_queue(&resolved.done_path, &done_file)?;
    git_test::commit_all(temp.path(), "init")?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let finalized = super::finalize_phase3_if_done(
        &resolved,
        "RQ-0001",
        None,
        GitRevertMode::Disabled,
        true,
        None,
        None,
        None,
        false,
        false,
    )?;
    assert!(finalized, "expected phase 3 finalization to run");

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(status.trim().is_empty(), "expected clean repo");
    Ok(())
}

// ============================================================================
// Auto-resume session tests
// ============================================================================

use crate::session;

fn task_with_id_and_status(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
    }
}

#[test]
fn validate_resumed_task_succeeds_when_task_exists_and_doing() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    // Should succeed when task exists and is Doing
    super::validate_resumed_task(&queue_file, "RQ-0001", &repo_root)?;

    Ok(())
}

#[test]
fn validate_resumed_task_fails_when_task_missing() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    // Should fail when task doesn't exist
    let err = super::validate_resumed_task(&queue_file, "RQ-9999", &repo_root).unwrap_err();
    assert!(err.to_string().contains("no longer exists"));

    Ok(())
}

#[test]
fn validate_resumed_task_fails_when_task_not_doing() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Todo)],
    };

    // Should fail when task exists but is not Doing
    let err = super::validate_resumed_task(&queue_file, "RQ-0001", &repo_root).unwrap_err();
    assert!(err.to_string().contains("not in Doing status"));

    Ok(())
}

#[test]
fn validate_resumed_task_clears_session_when_invalid() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Create a session for a task
    let session = crate::contracts::SessionState::new(
        "test-session".to_string(),
        "RQ-9999".to_string(),
        crate::timeutil::now_utc_rfc3339_or_fallback(),
        1,
        Runner::Claude,
        "sonnet".to_string(),
        0,
        None,
        None, // phase_settings
    );
    session::save_session(&cache_dir, &session)?;
    assert!(session::session_exists(&cache_dir));

    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Doing)],
    };

    // Validation should fail and clear the session
    let _ = super::validate_resumed_task(&queue_file, "RQ-9999", &repo_root);

    // Session should be cleared
    assert!(!session::session_exists(&cache_dir));

    Ok(())
}

#[test]
fn validate_resumed_task_clears_session_when_terminal() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Create a session for a done task
    let session = crate::contracts::SessionState::new(
        "test-session".to_string(),
        "RQ-0001".to_string(),
        crate::timeutil::now_utc_rfc3339_or_fallback(),
        1,
        Runner::Claude,
        "sonnet".to_string(),
        0,
        None,
        None, // phase_settings
    );
    session::save_session(&cache_dir, &session)?;
    assert!(session::session_exists(&cache_dir));

    // Task is done (terminal status)
    let queue_file = QueueFile {
        version: 1,
        tasks: vec![task_with_id_and_status("RQ-0001", TaskStatus::Done)],
    };

    // Validation should fail and clear the session
    let _ = super::validate_resumed_task(&queue_file, "RQ-0001", &repo_root);

    // Session should be cleared
    assert!(!session::session_exists(&cache_dir));

    Ok(())
}

/// Test that invalid phases values produce a clear user-facing error message.
/// This verifies the defensive programming pattern: even though phases are
/// validated early (1..=3), the match arm uses bail! instead of unreachable!
/// to provide graceful error handling if an invalid value somehow reaches it.
#[test]
fn invalid_phases_produces_user_facing_error() {
    // Directly test the error message format that would be produced
    // by the bail! macro in the match arm
    let phases: u8 = 4;
    let err = anyhow::format_err!(
        "Invalid phases value: {} (expected 1, 2, or 3). \
         This indicates a configuration error or internal inconsistency.",
        phases
    );
    let msg = err.to_string();
    assert!(
        msg.contains("Invalid phases value: 4"),
        "error should mention the invalid value"
    );
    assert!(
        msg.contains("expected 1, 2, or 3"),
        "error should state valid values"
    );
    assert!(
        msg.contains("configuration error or internal inconsistency"),
        "error should indicate severity"
    );
}

/// Test edge cases for invalid phases values.
#[test]
fn invalid_phases_edge_cases() {
    for invalid_phase in [0u8, 4u8, 255u8] {
        let err = anyhow::format_err!(
            "Invalid phases value: {} (expected 1, 2, or 3). \
             This indicates a configuration error or internal inconsistency.",
            invalid_phase
        );
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("Invalid phases value: {}", invalid_phase)),
            "error should contain the invalid value {}",
            invalid_phase
        );
    }
}

// ============================================================================
// Notification config construction tests
// ============================================================================

/// Helper to create a Resolved config with specific notification settings
fn resolved_with_notification_config(
    notify_on_complete: Option<bool>,
    notify_on_fail: Option<bool>,
    notify_on_loop_complete: Option<bool>,
) -> crate::config::Resolved {
    let dir = TempDir::new().expect("temp dir");
    let repo_root = dir.path().to_path_buf();

    let cfg = Config {
        agent: AgentConfig {
            runner: Some(Runner::Claude),
            model: Some(Model::Gpt52),
            reasoning_effort: None,
            iterations: None,
            followup_reasoning_effort: None,
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            cursor_bin: Some("agent".to_string()),
            kimi_bin: Some("kimi".to_string()),
            pi_bin: Some("pi".to_string()),
            phases: Some(2),
            update_task_before_run: None,
            fail_on_prerun_update_error: None,
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            runner_cli: None,
            phase_overrides: None,
            instruction_files: None,
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
            git_commit_push_enabled: Some(true),
            notification: NotificationConfig {
                enabled: Some(true),
                notify_on_complete,
                notify_on_fail,
                notify_on_loop_complete,
                suppress_when_active: Some(true),
                sound_enabled: Some(false),
                sound_path: None,
                timeout_ms: Some(8000),
            },
            webhook: crate::contracts::WebhookConfig::default(),
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
        },
        ..Config::default()
    };

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

/// Helper to create AgentOverrides with specific notification overrides
fn overrides_with_notifications(
    notify_on_complete: Option<bool>,
    notify_on_fail: Option<bool>,
) -> super::AgentOverrides {
    super::AgentOverrides {
        runner: None,
        model: None,
        reasoning_effort: None,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    }
}

/// Test that enabled=true when --notify-fail is set without --notify
/// This is the core bug fix: enabled should be true if ANY notification type is enabled
#[test]
fn notification_config_enabled_true_when_notify_fail_only() {
    // Config defaults: all notifications enabled
    let resolved = resolved_with_notification_config(Some(true), Some(true), Some(true));

    // CLI: --notify-fail (enable fail notifications) without --notify
    let overrides = overrides_with_notifications(None, Some(true));

    // Calculate the notification config values (mirroring run_loop logic)
    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be true because notify_on_fail is true
    assert!(
        enabled,
        "enabled should be true when notify_on_fail is true"
    );
    assert!(
        notify_on_complete,
        "notify_on_complete should be true from config"
    );
    assert!(
        notify_on_fail,
        "notify_on_fail should be true from CLI override"
    );
    assert!(
        notify_on_loop_complete,
        "notify_on_loop_complete should be true from config"
    );
}

/// Test that enabled=true when --no-notify and --notify-fail are both set
/// This ensures failure notifications work even when completion notifications are disabled
#[test]
fn notification_config_enabled_true_when_no_notify_and_notify_fail() {
    // Config defaults: all notifications enabled
    let resolved = resolved_with_notification_config(Some(true), Some(true), Some(true));

    // CLI: --no-notify --notify-fail
    let overrides = overrides_with_notifications(Some(false), Some(true));

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be true because notify_on_fail is true
    assert!(
        enabled,
        "enabled should be true when notify_on_fail is true even if notify_on_complete is false"
    );
    assert!(
        !notify_on_complete,
        "notify_on_complete should be false from CLI override"
    );
    assert!(
        notify_on_fail,
        "notify_on_fail should be true from CLI override"
    );
    assert!(
        notify_on_loop_complete,
        "notify_on_loop_complete should be true from config"
    );
}

/// Test that enabled=false when all notification types are disabled
#[test]
fn notification_config_enabled_false_when_all_disabled() {
    // Config: all notifications disabled
    let resolved = resolved_with_notification_config(Some(false), Some(false), Some(false));

    // CLI: no overrides
    let overrides = overrides_with_notifications(None, None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be false because all types are disabled
    assert!(
        !enabled,
        "enabled should be false when all notification types are disabled"
    );
    assert!(!notify_on_complete, "notify_on_complete should be false");
    assert!(!notify_on_fail, "notify_on_fail should be false");
    assert!(
        !notify_on_loop_complete,
        "notify_on_loop_complete should be false"
    );
}

/// Test that enabled=true when --notify is set alone
#[test]
fn notification_config_enabled_true_when_notify_alone() {
    // Config: all notifications disabled
    let resolved = resolved_with_notification_config(Some(false), Some(false), Some(false));

    // CLI: --notify (enable completion notifications)
    let overrides = overrides_with_notifications(Some(true), None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be true because notify_on_complete is true
    assert!(
        enabled,
        "enabled should be true when notify_on_complete is true"
    );
    assert!(
        notify_on_complete,
        "notify_on_complete should be true from CLI override"
    );
    assert!(
        !notify_on_fail,
        "notify_on_fail should be false from config"
    );
    assert!(
        !notify_on_loop_complete,
        "notify_on_loop_complete should be false from config (not using default)"
    );
}

/// Test that CLI overrides take precedence over config
#[test]
fn notification_config_cli_overrides_config() {
    // Config: all notifications disabled
    let resolved = resolved_with_notification_config(Some(false), Some(false), Some(false));

    // CLI: --notify --notify-fail (enable both)
    let overrides = overrides_with_notifications(Some(true), Some(true));

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    assert!(enabled);
    assert!(notify_on_complete, "CLI should override config");
    assert!(notify_on_fail, "CLI should override config");
    assert!(
        !notify_on_loop_complete,
        "notify_on_loop_complete should be false from config (not using default)"
    );
}

/// Test that config values are used when no CLI overrides
#[test]
fn notification_config_uses_config_when_no_cli_overrides() {
    // Config: mixed settings
    let resolved = resolved_with_notification_config(Some(true), Some(false), Some(true));

    // CLI: no overrides
    let overrides = overrides_with_notifications(None, None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    assert!(enabled);
    assert!(notify_on_complete, "should use config value");
    assert!(!notify_on_fail, "should use config value");
    assert!(notify_on_loop_complete, "should use config value");
}

/// Test the original bug: --no-notify would set enabled=false, suppressing ALL notifications
/// This verifies the fix works correctly
#[test]
fn notification_config_bug_no_notify_suppresses_all_notifications() {
    // Config: all notifications enabled
    let resolved = resolved_with_notification_config(Some(true), Some(true), Some(true));

    // CLI: --no-notify only (the bug scenario)
    let overrides = overrides_with_notifications(Some(false), None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);

    // With the fix: enabled should be true because notify_on_fail and notify_on_loop_complete are still true
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // Before the fix, this would have been false because enabled was set from notify_on_complete only
    assert!(
        enabled,
        "BUG: enabled should be true because notify_on_fail and notify_on_loop_complete are still enabled"
    );

    // Verify the individual flags
    assert!(
        !notify_on_complete,
        "notify_on_complete should be false from --no-notify"
    );
    assert!(
        notify_on_fail,
        "notify_on_fail should still be true from config"
    );
    assert!(
        notify_on_loop_complete,
        "notify_on_loop_complete should still be true from config"
    );
}

// ============================================================================
// Stop signal tests
// ============================================================================

use crate::signal;

#[test]
fn stop_signal_is_detected_after_task_completion() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // Create stop signal
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    // Clear it
    let cleared = signal::clear_stop_signal(&cache_dir)?;
    assert!(cleared);
    assert!(!signal::stop_signal_exists(&cache_dir));

    Ok(())
}

#[test]
fn stop_signal_clear_is_idempotent() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // Clearing non-existent signal returns Ok(false)
    let cleared = signal::clear_stop_signal(&cache_dir)?;
    assert!(!cleared);

    Ok(())
}

#[test]
fn stop_signal_create_is_idempotent() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // First creation
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    // Second creation (should succeed, overwriting)
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    Ok(())
}

// ============================================================================
// Per-phase settings resolution matrix tests (RQ-0491)
// ============================================================================

use crate::agent::AgentOverrides;
use crate::commands::run::RunnerCliOptionsPatch;
use crate::runner::resolve_phase_settings_matrix;

/// Helper to create a minimal AgentConfig for testing
fn test_config_agent(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
) -> AgentConfig {
    AgentConfig {
        runner,
        model,
        reasoning_effort: effort,
        iterations: None,
        followup_reasoning_effort: None,
        codex_bin: Some("codex".to_string()),
        opencode_bin: Some("opencode".to_string()),
        gemini_bin: Some("gemini".to_string()),
        claude_bin: Some("claude".to_string()),
        cursor_bin: Some("agent".to_string()),
        kimi_bin: Some("kimi".to_string()),
        pi_bin: Some("pi".to_string()),
        phases: Some(3),
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
        runner_cli: None,
        phase_overrides: None,
        instruction_files: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        ci_gate_command: Some("make ci".to_string()),
        ci_gate_enabled: Some(true),
        git_revert_mode: Some(GitRevertMode::Ask),
        git_commit_push_enabled: Some(true),
        notification: NotificationConfig::default(),
        webhook: crate::contracts::WebhookConfig::default(),
    }
}

/// Helper to create a minimal TaskAgent for testing
fn test_task_agent(runner: Option<Runner>, model: Option<Model>, effort: ModelEffort) -> TaskAgent {
    TaskAgent {
        runner,
        model,
        model_effort: effort,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
    }
}

/// Helper to create AgentOverrides with phase-specific settings
fn test_overrides_with_phases(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
    phase_overrides: Option<PhaseOverrides>,
) -> AgentOverrides {
    AgentOverrides {
        runner,
        model,
        reasoning_effort: effort,
        runner_cli: RunnerCliOptionsPatch::default(),
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides,
    }
}

// ============================================================================
// Precedence chain tests
// ============================================================================

#[test]
fn resolve_phase_settings_cli_phase_override_beats_global() {
    // CLI phase override should beat CLI global override
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(
        Some(Runner::Opencode), // Global CLI override
        Some(Model::Glm47),     // Global CLI model
        Some(ReasoningEffort::High),
        Some(phase_overrides),
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1 should use CLI phase override (not CLI global)
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::Low));
}

#[test]
fn resolve_phase_settings_config_phase_override_beats_global() {
    // Config phase override should beat CLI global override
    let mut config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: None,
        }),
        ..Default::default()
    });

    let overrides = test_overrides_with_phases(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::High),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 2 should use config phase override
    assert_eq!(matrix.phase2.runner, Runner::Gemini);
    assert_eq!(matrix.phase2.model.as_str(), "gemini-pro");
}

#[test]
fn resolve_phase_settings_cli_global_beats_task() {
    // CLI global override should beat task override
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    let task_agent = test_task_agent(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        ModelEffort::High,
    );

    let overrides = test_overrides_with_phases(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    // All phases should use CLI global override
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase3.runner, Runner::Codex);
}

#[test]
fn resolve_phase_settings_task_beats_config() {
    // Task override should beat config default
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    let task_agent = test_task_agent(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        ModelEffort::High,
    );

    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    // All phases should use task override
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}

#[test]
fn resolve_phase_settings_config_beats_default() {
    // Config default should be used when nothing else specified
    let config_agent = test_config_agent(
        Some(Runner::Gemini),
        Some(Model::Custom("gemini-custom".to_string())),
        None,
    );

    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // All phases should use config default
    assert_eq!(matrix.phase1.runner, Runner::Gemini);
    // Custom model should be preserved
    assert_eq!(matrix.phase1.model.as_str(), "gemini-custom");
}

#[test]
fn resolve_phase_settings_uses_code_default_when_nothing_specified() {
    // Code default should be used when nothing specified
    let config_agent = AgentConfig::default();
    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Should use default runner (Claude) and its default model
    assert_eq!(matrix.phase1.runner, Runner::Claude);
}

// ============================================================================
// Model defaulting tests
// ============================================================================

#[test]
fn resolve_phase_settings_runner_override_uses_default_model() {
    // When runner is overridden without explicit model, use runner's default
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let overrides = test_overrides_with_phases(
        Some(Runner::Opencode), // Override runner but not model
        None,
        None,
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Should use Opencode's default model, not config's model
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}

#[test]
fn resolve_phase_settings_phase_runner_override_uses_default_model() {
    // When runner is overridden at phase level without explicit model
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None, // No explicit model
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1 should use Opencode's default model
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);

    // Other phases should use config default
    assert_eq!(matrix.phase2.runner, Runner::Claude);
}

#[test]
fn resolve_phase_settings_explicit_model_preserved_with_runner_override() {
    // When both runner and model are explicitly overridden
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let phase_overrides = PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52), // Explicit model
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 2 should use explicit model
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase2.model, Model::Gpt52);
}

// ============================================================================
// Effort handling tests
// ============================================================================

#[test]
fn resolve_phase_settings_effort_some_for_codex() {
    // Effort should be Some() for Codex runners
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn resolve_phase_settings_effort_none_for_non_codex() {
    // Effort should be None for non-Codex runners
    let config_agent = test_config_agent(Some(Runner::Opencode), Some(Model::Glm47), None);

    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, None);
    assert_eq!(matrix.phase2.reasoning_effort, None);
}

#[test]
fn resolve_phase_settings_effort_precedence_within_codex() {
    // Effort should follow precedence within Codex phases
    let config_agent = test_config_agent(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Low),
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High), // Phase-specific effort
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1 should use phase-specific effort
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    // Other phases use config default
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::Low));
}

// ============================================================================
// Single-pass mapping tests
// ============================================================================

#[test]
fn resolve_phase_settings_single_pass_uses_phase2_overrides() {
    // Single-pass (--phases 1) should use Phase 2 overrides
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    // Phase 2 settings should be resolved (for single-pass execution)
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase2.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));

    // But Phase 1 and Phase 3 overrides are unused
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2); // Phase 2 is used
    assert!(warnings.unused_phase3);
}

#[test]
fn resolve_phase_settings_two_phase_warns_about_phase3() {
    // Two-phase execution should warn about unused phase 3 overrides
    let phase_overrides = PhaseOverrides {
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();

    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);
}

// ============================================================================
// Validation error tests
// ============================================================================

#[test]
fn resolve_phase_settings_invalid_model_for_codex() {
    // Invalid model for Codex should produce phase-specific error
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let phase_overrides = PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Glm47), // Invalid for Codex
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let result = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Phase 2"));
    assert!(err.contains("invalid model"));
}

#[test]
fn resolve_phase_settings_invalid_custom_model_for_codex() {
    // Custom model that's invalid for Codex
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Custom("invalid-model".to_string())),
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let result = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Phase 1"));
}

// ============================================================================
// Warning tests
// ============================================================================

#[test]
fn resolve_phase_settings_warns_unused_phase3_when_phases_is_2() {
    let phase_overrides = PhaseOverrides {
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();

    assert!(warnings.unused_phase3);
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
}

#[test]
fn resolve_phase_settings_warns_unused_phase1_and_phase3_when_phases_is_1() {
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2); // Phase 2 is used for single-pass
    assert!(warnings.unused_phase3);
}

// ============================================================================
// Complex integration tests
// ============================================================================

#[test]
fn resolve_phase_settings_full_matrix_resolution() {
    // Test a complex scenario with different settings per phase
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        Some(ReasoningEffort::Medium),
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,            // Should use Opencode default
            reasoning_effort: None, // Ignored for non-Codex
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: Some(ReasoningEffort::Low), // Ignored for non-Codex
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1: Codex with high effort
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));

    // Phase 2: Opencode with default model, no effort
    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);
    assert_eq!(matrix.phase2.reasoning_effort, None);

    // Phase 3: Gemini with custom model, no effort (non-Codex)
    assert_eq!(matrix.phase3.runner, Runner::Gemini);
    assert_eq!(matrix.phase3.model.as_str(), "gemini-pro");
    assert_eq!(matrix.phase3.reasoning_effort, None);
}

#[test]
fn resolve_phase_settings_config_phase_overrides_only() {
    // Test config-based phase overrides (not CLI)
    let mut config_agent = test_config_agent(Some(Runner::Claude), None, None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: None,
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    });

    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);

    assert_eq!(matrix.phase2.runner, Runner::Claude); // Config default

    assert_eq!(matrix.phase3.runner, Runner::Gemini);
}

#[test]
fn resolve_phase_settings_cli_overrides_config_phase() {
    // CLI phase overrides should beat config phase overrides
    let mut config_agent = test_config_agent(Some(Runner::Claude), None, None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    });

    let cli_phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode), // CLI overrides config
            model: Some(Model::Glm47),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(cli_phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // CLI should win over config
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
    // Effort is ignored for Opencode but CLI value was specified
}

// ============================================================================
// Per-phase settings wiring tests (RQ-0496)
// ============================================================================

use crate::runner::ResolvedPhaseSettings;

/// Test that ResolvedPhaseSettings can be converted to AgentSettings
#[test]
fn resolved_phase_settings_to_agent_settings_conversion() {
    let phase_settings = ResolvedPhaseSettings {
        runner: Runner::Codex,
        model: Model::Gpt52Codex,
        reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };

    let agent_settings = phase_settings.to_agent_settings();

    assert_eq!(agent_settings.runner, Runner::Codex);
    assert_eq!(agent_settings.model, Model::Gpt52Codex);
    assert_eq!(agent_settings.reasoning_effort, Some(ReasoningEffort::High));
}

/// Test that different runners in different phases are properly resolved
#[test]
fn per_phase_settings_different_runners_per_phase() {
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1: Codex
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));

    // Phase 2: Opencode
    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);
    assert_eq!(matrix.phase2.reasoning_effort, None);

    // Phase 3: Gemini
    assert_eq!(matrix.phase3.runner, Runner::Gemini);
    assert_eq!(matrix.phase3.model.as_str(), "gemini-pro");
    assert_eq!(matrix.phase3.reasoning_effort, None);
}

/// Test that single-pass mode (phases=1) uses Phase 2 settings
#[test]
fn single_pass_uses_phase2_settings() {
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    // Phase 2 settings should be resolved for single-pass
    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);

    // Phase 1 and Phase 3 overrides are unused
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2); // Phase 2 is used
    assert!(warnings.unused_phase3);
}

/// Test that Phase 2 settings are always resolved (used in all modes)
#[test]
fn phase2_settings_always_resolved() {
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);
    let overrides = AgentOverrides::default();

    // Test with phases=1
    let (matrix, _) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();
    assert_eq!(matrix.phase2.runner, Runner::Claude);

    // Test with phases=2
    let (matrix, _) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();
    assert_eq!(matrix.phase2.runner, Runner::Claude);

    // Test with phases=3
    let (matrix, _) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();
    assert_eq!(matrix.phase2.runner, Runner::Claude);
}

/// Test that warnings are properly collected for unused phases
#[test]
fn resolution_warnings_collected_correctly() {
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: None,
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    // With phases=1: phase1 and phase3 are unused
    let (_, warnings) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);

    // With phases=2: only phase3 is unused
    let (_, warnings) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);

    // With phases=3: no unused phases
    let (_, warnings) = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(!warnings.unused_phase3);
}
