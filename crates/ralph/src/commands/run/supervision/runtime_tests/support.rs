//! Shared builders for supervision runtime tests.
//!
//! Responsibilities:
//! - Build queue fixtures and resolved config state for supervision tests.
//! - Provide canonical `ContinueSession` setup helpers for runner-specific regressions.
//! - Centralize serialized environment mutation used by resume tests.
//!
//! Not handled here:
//! - Scenario assertions or behavior-specific orchestration.
//! - Integration-test subprocess helpers outside supervision unit coverage.
//!
//! Invariants/assumptions:
//! - Helper configs disable unrelated features unless a scenario opts in.
//! - Queue fixtures always target `RQ-0001`.

use crate::commands::run::supervision::ContinueSession;
use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
use crate::contracts::{
    AgentConfig, Config, GitRevertMode, NotificationConfig, QueueConfig, QueueFile, Runner,
    RunnerRetryConfig, Task, TaskPriority, TaskStatus,
};
use crate::queue;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(super) static PI_ENV_MUTEX: Mutex<()> = Mutex::new(());

pub(super) fn make_task(id: &str, title: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: title.to_string(),
        description: None,
        priority: TaskPriority::Medium,
        tags: vec!["tests".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: matches!(status, TaskStatus::Done | TaskStatus::Rejected)
            .then(|| "2026-01-18T00:05:00Z".to_string()),
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

pub(super) fn write_queue(repo_root: &Path, status: TaskStatus) -> anyhow::Result<()> {
    write_queue_tasks(repo_root, vec![make_task("RQ-0001", "Test task", status)])
}

pub(super) fn write_queue_tasks(repo_root: &Path, tasks: Vec<Task>) -> anyhow::Result<()> {
    queue::save_queue(
        &repo_root.join(".ralph/queue.jsonc"),
        &QueueFile { version: 1, tasks },
    )?;
    Ok(())
}

pub(super) fn write_done_tasks(repo_root: &Path, tasks: Vec<Task>) -> anyhow::Result<()> {
    queue::save_queue(
        &repo_root.join(".ralph/done.jsonc"),
        &QueueFile { version: 1, tasks },
    )?;
    Ok(())
}

pub(super) fn resolved_for_repo(repo_root: &Path) -> crate::config::Resolved {
    let cfg = Config {
        agent: AgentConfig {
            runner: Some(Runner::Codex),
            model: Some(crate::contracts::Model::Gpt53Codex),
            reasoning_effort: Some(crate::contracts::ReasoningEffort::Medium),
            iterations: Some(1),
            followup_reasoning_effort: None,
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            cursor_bin: Some("agent".to_string()),
            kimi_bin: Some("kimi".to_string()),
            pi_bin: Some("pi".to_string()),
            claude_permission_mode: Some(crate::contracts::ClaudePermissionMode::BypassPermissions),
            runner_cli: None,
            phase_overrides: None,
            instruction_files: None,
            repoprompt_plan_required: Some(false),
            repoprompt_tool_injection: Some(false),
            ci_gate: Some(crate::contracts::CiGateConfig {
                enabled: Some(false),
                argv: None,
            }),
            git_revert_mode: Some(GitRevertMode::Disabled),
            git_publish_mode: Some(crate::contracts::GitPublishMode::CommitAndPush),
            phases: Some(2),
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
            file: Some(PathBuf::from(".ralph/queue.jsonc")),
            done_file: Some(PathBuf::from(".ralph/done.jsonc")),
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

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.to_path_buf(),
        queue_path: repo_root.join(".ralph/queue.jsonc"),
        done_path: repo_root.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
    }
}

pub(super) fn continue_session_with(
    runner: Runner,
    session_id: Option<&str>,
    phase_type: crate::commands::run::PhaseType,
) -> ContinueSession {
    ContinueSession {
        runner,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type,
        session_id: session_id.map(str::to_string),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    }
}

pub(super) fn continue_review_session(session_id: &str) -> ContinueSession {
    let mut session = continue_session_with(
        Runner::Opencode,
        Some(session_id),
        crate::commands::run::PhaseType::Review,
    );
    session.ci_failure_retry_count = CI_GATE_AUTO_RETRY_LIMIT;
    session
}
