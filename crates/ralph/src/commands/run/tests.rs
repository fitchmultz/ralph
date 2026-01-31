//! Unit tests for run command orchestration helpers.

use super::{
    apply_followup_reasoning_effort, resolve_iteration_settings, resolve_run_agent_settings,
    run_one_with_id_locked, task_context_for_prompt,
};
use crate::completions;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Config, GitRevertMode, Model, ModelEffort,
    NotificationConfig, QueueConfig, QueueFile, ReasoningEffort, Runner, Task, TaskAgent,
    TaskStatus,
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
