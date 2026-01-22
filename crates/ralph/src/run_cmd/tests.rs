//! Unit tests for run command orchestration helpers.

use super::{resolve_run_agent_settings, run_one_with_id_locked, task_context_for_prompt};
use crate::completions;
use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Config, GitRevertMode, Model, QueueConfig, QueueFile,
    ReasoningEffort, Runner, Task, TaskAgent, TaskStatus,
};
use crate::queue;
use std::path::PathBuf;
use tempfile::TempDir;

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
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            phases: Some(2),
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            require_repoprompt: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
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
            codex_bin: Some("codex".to_string()),
            opencode_bin: Some("opencode".to_string()),
            gemini_bin: Some("gemini".to_string()),
            claude_bin: Some("claude".to_string()),
            phases: Some(3),
            claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            require_repoprompt: None,
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_revert_mode: Some(GitRevertMode::Ask),
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
        reasoning_effort: Some(ReasoningEffort::High),
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
        reasoning_effort: Some(ReasoningEffort::Low),
    });

    let overrides = super::AgentOverrides {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        reasoning_effort: Some(ReasoningEffort::High),
        phases: None,
        repoprompt_required: None,
        git_revert_mode: None,
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
        repoprompt_required: None,
        git_revert_mode: None,
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
        repoprompt_required: None,
        git_revert_mode: None,
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
fn resolve_run_agent_settings_effort_is_ignored_for_opencode() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Opencode),
        Some(Model::Gpt52),
        Some(ReasoningEffort::Low),
    );

    let task = base_task();
    let overrides = super::AgentOverrides {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        reasoning_effort: Some(ReasoningEffort::High),
        phases: None,
        repoprompt_required: None,
        git_revert_mode: None,
        include_draft: None,
    };

    let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Gpt52);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
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
