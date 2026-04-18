//! Queue-op unit coverage for post-run supervision.
//!
//! Responsibilities:
//! - Validate queue maintenance helpers, mutation planning, and dirty-repo revert behavior.
//! - Keep `queue_ops.rs` focused on production logic while test fixtures live here.
//!
//! Not handled here:
//! - End-to-end post-run orchestration (covered in `runtime_tests/`).
//! - CI continue-session escalation behavior.

use super::*;
use crate::contracts::{
    AgentConfig, Config, NotificationConfig, QueueConfig, QueueFile, Runner, RunnerRetryConfig,
    Task, TaskPriority, TaskStatus,
};
use crate::queue;
use crate::runutil::{RevertDecision, RevertPromptHandler};
use crate::testsupport::git as git_test;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

fn write_queue(repo_root: &Path, status: TaskStatus) -> Result<()> {
    let task = Task {
        id: "RQ-0001".to_string(),
        status,
        title: "Test task".to_string(),
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
    };

    queue::save_queue(
        &repo_root.join(".ralph/queue.json"),
        &QueueFile {
            version: 1,
            tasks: vec![task],
        },
    )?;
    Ok(())
}

fn resolved_for_repo(repo_root: &Path) -> crate::config::Resolved {
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
            git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
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

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.to_path_buf(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

#[test]
fn maintain_and_validate_queues_backfills_missing_completed_at() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_repo(temp.path());
    let (queue_file, _done_file) = maintain_and_validate_queues(&resolved, None)?;

    let task = queue_file
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("expected task in queue");
    let completed_at = task
        .completed_at
        .as_deref()
        .expect("completed_at should be stamped");

    crate::timeutil::parse_rfc3339(completed_at)?;

    Ok(())
}

#[test]
fn find_task_status_finds_in_queue() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Todo)?;

    let queue_file = queue::load_queue(&temp.path().join(".ralph/queue.json"))?;
    let done_file = QueueFile::default();

    let (status, title, in_done) =
        find_task_status(&queue_file, &done_file, "RQ-0001").expect("should find task");

    assert_eq!(status, TaskStatus::Todo);
    assert_eq!(title, "Test task");
    assert!(!in_done);

    Ok(())
}

#[test]
fn find_task_status_finds_in_done() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_repo(temp.path());
    queue::archive_terminal_tasks(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        10,
    )?;

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;

    let (status, title, in_done) =
        find_task_status(&queue_file, &done_file, "RQ-0001").expect("should find task");

    assert_eq!(status, TaskStatus::Done);
    assert_eq!(title, "Test task");
    assert!(in_done);

    Ok(())
}

#[test]
fn find_task_status_returns_none_for_missing() {
    let queue_file = QueueFile::default();
    let done_file = QueueFile::default();

    let result = find_task_status(&queue_file, &done_file, "RQ-9999");
    assert!(result.is_none());
}

#[test]
fn require_task_status_errors_for_missing() {
    let queue_file = QueueFile::default();
    let done_file = QueueFile::default();

    let err = require_task_status(&queue_file, &done_file, "RQ-9999").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn build_post_run_queue_mutation_plan_marks_pending_completion_as_mutating() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Todo)?;

    let queue_file = queue::load_queue(&temp.path().join(".ralph/queue.json"))?;
    let done_file = QueueFile::default();
    let plan = build_post_run_queue_mutation_plan(&queue_file, &done_file, "RQ-0001")?;

    assert_eq!(plan.task_status, TaskStatus::Todo);
    assert!(plan.mark_task_done);
    assert!(plan.will_mutate_queue_files());
    assert_eq!(plan.archive_candidate_ids, vec!["RQ-0001".to_string()]);

    Ok(())
}

#[test]
fn build_post_run_queue_mutation_plan_detects_archived_done_noop() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_repo(temp.path());
    queue::archive_terminal_tasks(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        10,
    )?;

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let plan = build_post_run_queue_mutation_plan(&queue_file, &done_file, "RQ-0001")?;

    assert!(plan.task_already_archived_done());
    assert!(!plan.mark_task_done);
    assert!(!plan.will_mutate_queue_files());
    assert!(plan.archive_candidate_ids.is_empty());

    Ok(())
}

#[test]
fn ensure_task_done_clean_or_bail_marks_done_when_needed() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Todo)?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let changed = ensure_task_done_clean_or_bail(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Todo,
        false,
    )?;

    assert!(changed);

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let task = queue_file.tasks.iter().find(|t| t.id == "RQ-0001").unwrap();
    assert_eq!(task.status, TaskStatus::Done);

    Ok(())
}

#[test]
fn ensure_task_done_clean_or_bail_no_change_when_already_done() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let changed = ensure_task_done_clean_or_bail(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Done,
        false,
    )?;

    assert!(!changed);

    Ok(())
}

#[test]
fn ensure_task_done_clean_or_bail_errors_on_inconsistency() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Todo)?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let err = ensure_task_done_clean_or_bail(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Todo,
        true,
    )
    .unwrap_err();

    assert!(err.to_string().contains("inconsistency"));

    Ok(())
}

#[test]
fn ensure_task_done_dirty_or_revert_marks_done_when_not_archived() -> Result<()> {
    let temp = TempDir::new()?;
    write_queue(temp.path(), TaskStatus::Todo)?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    ensure_task_done_dirty_or_revert(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Todo,
        false,
        crate::contracts::GitRevertMode::Disabled,
        None,
    )?;

    let persisted = queue::load_queue(&resolved.queue_path)?;
    let task = persisted
        .tasks
        .iter()
        .find(|task| task.id == "RQ-0001")
        .expect("expected task to persist");
    assert_eq!(task.status, TaskStatus::Done);
    assert!(task.completed_at.is_some());

    Ok(())
}

#[test]
fn ensure_task_done_dirty_or_revert_disabled_keeps_dirty_changes_on_inconsistency() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    std::fs::write(temp.path().join("tracked.txt"), "original\n")?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("tracked.txt"), "modified\n")?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let err = ensure_task_done_dirty_or_revert(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Todo,
        true,
        crate::contracts::GitRevertMode::Disabled,
        None,
    )
    .unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("Task inconsistency"));
    assert!(message.contains("Revert skipped (git_revert_mode=disabled)"));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("tracked.txt"))?,
        "modified\n"
    );

    Ok(())
}

#[test]
fn ensure_task_done_dirty_or_revert_enabled_reverts_dirty_changes_on_inconsistency() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    std::fs::write(temp.path().join("tracked.txt"), "original\n")?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("tracked.txt"), "modified\n")?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let err = ensure_task_done_dirty_or_revert(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Todo,
        true,
        crate::contracts::GitRevertMode::Enabled,
        None,
    )
    .unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("Task inconsistency"));
    assert!(message.contains("Uncommitted changes were reverted."));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("tracked.txt"))?,
        "original\n"
    );

    Ok(())
}

#[test]
fn ensure_task_done_dirty_or_revert_ask_uses_prompt_handler() -> Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    std::fs::write(temp.path().join("tracked.txt"), "original\n")?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("tracked.txt"), "modified\n")?;

    let resolved = resolved_for_repo(temp.path());
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let seen_labels = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_labels_for_prompt = Arc::clone(&seen_labels);
    let prompt: RevertPromptHandler = Arc::new(move |context| {
        seen_labels_for_prompt
            .lock()
            .expect("prompt label mutex")
            .push(context.label.clone());
        Ok(RevertDecision::Keep)
    });

    let err = ensure_task_done_dirty_or_revert(
        &resolved,
        &mut queue_file,
        "RQ-0001",
        TaskStatus::Todo,
        true,
        crate::contracts::GitRevertMode::Ask,
        Some(&prompt),
    )
    .unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("Task inconsistency"));
    assert!(message.contains("Revert skipped (user chose to keep changes)"));
    assert_eq!(
        seen_labels.lock().expect("prompt label mutex").as_slice(),
        ["Task inconsistency detected"]
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("tracked.txt"))?,
        "modified\n"
    );

    Ok(())
}
