//! Unit tests for run command orchestration helpers.
//!
//! This module has been split into submodules organized by functional area:
//! - `infrastructure`: Shared test infrastructure and helpers (in this file)
//! - `queue_lock`: Queue lock handling tests
//! - `agent_settings`: Agent settings resolution tests
//! - `auto_resume`: Auto-resume session tests
//! - `notifications`: Notification configuration tests
//! - `stop_signal`: Stop signal tests
//! - `phase_settings_matrix`: Per-phase settings resolution matrix tests
//! - `phase_settings_wiring`: Per-phase settings wiring tests
//! - `dirty_repo`: Dirty repository error detection tests

// Shared test infrastructure
use crate::config;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Config, GitRevertMode, Model, ModelEffort,
    NotificationConfig, PhaseOverrides, QueueConfig, ReasoningEffort, Runner, RunnerRetryConfig,
    Task, TaskAgent, TaskStatus,
};
use log::{LevelFilter, Log, Metadata, Record};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

// Re-export commonly used types for convenience

/// Test logger for capturing log output during tests.
pub struct TestLogger;

static LOGGER: TestLogger = TestLogger;
static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoggerState {
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

/// Initialize the test logger and return the logger state and logs mutex.
pub fn init_logger() -> (LoggerState, &'static Mutex<Vec<String>>) {
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

/// Take all accumulated logs and return them along with the logger state.
pub fn take_logs() -> (LoggerState, Vec<String>) {
    let (state, logs) = init_logger();
    let mut guard = logs.lock().expect("log mutex");
    let drained = guard.drain(..).collect::<Vec<_>>();
    (state, drained)
}

/// Create a Resolved config with agent defaults for testing.
pub fn resolved_with_agent_defaults(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
) -> config::Resolved {
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
            runner_retry: RunnerRetryConfig::default(),
            session_timeout_hours: None,
            scan_prompt_version: None,
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
            auto_archive_terminal_after_days: None,
            aging_thresholds: None,
        },
        ..Config::default()
    };

    config::Resolved {
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

/// Create a basic task with default values for testing.
pub fn base_task() -> Task {
    Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Todo,
        title: "Test task".to_string(),
        description: None,
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
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

/// Create a Resolved config with a specific repo root for testing.
pub fn resolved_with_repo_root(repo_root: PathBuf) -> config::Resolved {
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
            runner_retry: RunnerRetryConfig::default(),
            session_timeout_hours: None,
            scan_prompt_version: None,
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
            auto_archive_terminal_after_days: None,
            aging_thresholds: None,
        },
        ..Config::default()
    };

    config::Resolved {
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

/// Create a task with a specific status for testing.
pub fn task_with_status(status: TaskStatus) -> Task {
    Task {
        id: "RQ-0001".to_string(),
        status,
        title: "Test task".to_string(),
        description: None,
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
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

/// Create a task with a specific ID and status for testing.
pub fn task_with_id_and_status(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        description: None,
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
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

/// Find a PID that is definitely not running on the system.
/// Used for testing stale lock detection.
pub fn find_definitely_dead_pid() -> u32 {
    // `pid_is_running` is best-effort; pick a PID that we can confirm is not running.
    // Prefer very large values to avoid colliding with real processes.
    for pid in [0xFFFFFFFE, 999_999, 500_000, 250_000, 100_000] {
        if crate::lock::pid_is_running(pid) == Some(false) {
            return pid;
        }
    }
    panic!("Could not find a definitely-dead PID on this system");
}

/// Helper to create a Resolved config with specific notification settings.
pub fn resolved_with_notification_config(
    notify_on_complete: Option<bool>,
    notify_on_fail: Option<bool>,
    notify_on_loop_complete: Option<bool>,
) -> config::Resolved {
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
            runner_retry: RunnerRetryConfig::default(),
            session_timeout_hours: None,
            scan_prompt_version: None,
        },
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
            auto_archive_terminal_after_days: None,
            aging_thresholds: None,
        },
        ..Config::default()
    };

    config::Resolved {
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

/// Helper to create AgentOverrides with specific notification overrides.
pub fn overrides_with_notifications(
    notify_on_complete: Option<bool>,
    notify_on_fail: Option<bool>,
) -> crate::agent::AgentOverrides {
    crate::agent::AgentOverrides {
        profile: None,
        runner: None,
        model: None,
        reasoning_effort: None,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
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

/// Helper to create a minimal AgentConfig for testing phase settings.
pub fn test_config_agent(
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
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: None,
        scan_prompt_version: None,
    }
}

/// Helper to create a minimal TaskAgent for testing phase settings.
pub fn test_task_agent(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: ModelEffort,
) -> TaskAgent {
    TaskAgent {
        runner,
        model,
        model_effort: effort,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    }
}

/// Helper to create AgentOverrides with phase-specific settings.
pub fn test_overrides_with_phases(
    runner: Option<Runner>,
    model: Option<Model>,
    effort: Option<ReasoningEffort>,
    phase_overrides: Option<PhaseOverrides>,
) -> crate::agent::AgentOverrides {
    crate::agent::AgentOverrides {
        profile: None,
        runner,
        model,
        reasoning_effort: effort,
        runner_cli: crate::contracts::RunnerCliOptionsPatch::default(),
        phases: None,
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

// Test submodules
mod agent_settings;
mod auto_resume;
mod dirty_repo;
mod notifications;
mod phase_settings_matrix;
mod phase_settings_wiring;
mod queue_lock;
mod stop_signal;
