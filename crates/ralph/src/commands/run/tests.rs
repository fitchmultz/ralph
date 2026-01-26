//! Unit tests for run command orchestration helpers.

use super::{
    apply_followup_reasoning_effort, resolve_iteration_settings, resolve_run_agent_settings,
    run_one_with_id_locked, task_context_for_prompt,
};
use crate::completions;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Config, GitRevertMode, Model, ModelEffort, QueueConfig,
    QueueFile, ReasoningEffort, Runner, Task, TaskAgent, TaskStatus,
};
use crate::queue;
use crate::runner;
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
            phases: Some(2),
            update_task_before_run: None,
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
            git_commit_push_enabled: Some(true),
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
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
        depends_on: vec![],
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
            phases: Some(3),
            update_task_before_run: None,
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
            git_commit_push_enabled: Some(true),
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
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
        depends_on: vec![],
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
        depends_on: vec![],
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
    });

    let overrides = super::AgentOverrides {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        reasoning_effort: Some(ReasoningEffort::High),
        phases: None,
        update_task_before_run: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
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
        phases: None,
        update_task_before_run: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
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
        phases: None,
        update_task_before_run: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
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
    });
    let overrides = super::AgentOverrides {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        reasoning_effort: Some(ReasoningEffort::High),
        phases: None,
        update_task_before_run: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
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
    };
    let updated = apply_followup_reasoning_effort(&base, Some(ReasoningEffort::High), true);
    assert_eq!(updated.reasoning_effort, Some(ReasoningEffort::High));

    let base_non_codex = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Glm47,
        reasoning_effort: None,
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
