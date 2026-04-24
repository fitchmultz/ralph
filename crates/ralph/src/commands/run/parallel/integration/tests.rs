//! Tests for parallel integration helpers.
//!
//! Purpose:
//! - Tests for parallel integration helpers.
//!
//! Responsibilities:
//! - Cover integration config, prompt rendering, compliance helpers, and marker persistence.
//! - Keep validation-focused tests separate from production orchestration code.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::bookkeeping::{finalize_bookkeeping_and_push, rebuild_bookkeeping_from_target};
use super::*;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::runutil::FixedBackoffSchedule;
use crate::testsupport::git as git_test;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;

fn make_task(id: &str, status: TaskStatus) -> Task {
    let completed_at = matches!(status, TaskStatus::Done | TaskStatus::Rejected)
        .then(|| "2026-01-01T00:00:00Z".to_string());
    Task {
        id: id.to_string(),
        title: format!("Task {}", id),
        description: None,
        status,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    }
}

fn task_ids(queue_file: &QueueFile) -> Vec<String> {
    queue_file
        .tasks
        .iter()
        .map(|task| task.id.trim().to_string())
        .collect()
}

#[test]
fn integration_config_default_backoff() {
    let config = IntegrationConfig {
        max_attempts: 5,
        backoff_schedule: FixedBackoffSchedule::from_millis(&[500, 2000, 5000, 10000]),
        target_branch: "main".into(),
        ci_enabled: true,
        ci_label: "make ci".into(),
    };

    assert_eq!(config.backoff_for_attempt(0), Duration::from_millis(500));
    assert_eq!(config.backoff_for_attempt(1), Duration::from_millis(2000));
    assert_eq!(config.backoff_for_attempt(2), Duration::from_millis(5000));
    assert_eq!(config.backoff_for_attempt(3), Duration::from_millis(10000));
    assert_eq!(config.backoff_for_attempt(4), Duration::from_millis(10000));
    assert_eq!(config.backoff_for_attempt(10), Duration::from_millis(10000));
}

#[test]
fn remediation_handoff_builder() {
    let handoff = RemediationHandoff::new("RQ-0001", "Test Task", "main", 2, 5)
        .with_conflicts(vec!["src/lib.rs".into(), "src/main.rs".into()])
        .with_git_status("UU src/lib.rs\nUU src/main.rs".into())
        .with_phase_summary("Implemented feature X".into())
        .with_task_intent("Complete feature X implementation".into());

    assert_eq!(handoff.task_id, "RQ-0001");
    assert_eq!(handoff.task_title, "Test Task");
    assert_eq!(handoff.target_branch, "main");
    assert_eq!(handoff.attempt, 2);
    assert_eq!(handoff.max_attempts, 5);
    assert_eq!(handoff.conflict_files.len(), 2);
    assert_eq!(handoff.phase_summary, "Implemented feature X");
    assert!(handoff.ci_context.is_none());
}

#[test]
fn remediation_handoff_with_ci() {
    let handoff = RemediationHandoff::new("RQ-0001", "Test", "main", 1, 5).with_ci_context(
        "make ci".into(),
        "test failed".into(),
        1,
    );

    assert!(handoff.ci_context.is_some());
    let ci = handoff.ci_context.unwrap();
    assert_eq!(ci.command, "make ci");
    assert_eq!(ci.last_output, "test failed");
    assert_eq!(ci.exit_code, 1);
}

#[test]
fn integration_prompt_contains_mandatory_contract() {
    let queue_path = crate::testsupport::path::portable_abs_path("queue.json");
    let done_path = crate::testsupport::path::portable_abs_path("done.json");
    let prompt = build_agent_integration_prompt(
        "RQ-0001",
        "Implement feature",
        "main",
        &queue_path,
        &done_path,
        1,
        5,
        "phase summary",
        " M src/lib.rs",
        true,
        "make ci",
        Some("previous failure"),
    );

    assert!(prompt.contains("MUST execute integration git operations"));
    assert!(prompt.contains("Completion Contract (Mandatory)"));
    assert!(prompt.contains("Do not push"));
    assert!(prompt.contains("previous failure"));
}

#[test]
fn integration_prompt_uses_explicit_target_branch_for_integration() {
    let queue_path = crate::testsupport::path::portable_abs_path("queue.json");
    let done_path = crate::testsupport::path::portable_abs_path("done.json");
    let prompt = build_agent_integration_prompt(
        "RQ-0001",
        "Implement feature",
        "release/2026",
        &queue_path,
        &done_path,
        1,
        5,
        "phase summary",
        " M src/lib.rs",
        true,
        "make ci",
        None,
    );

    assert!(prompt.contains("git fetch origin release/2026"));
    assert!(prompt.contains("git rebase origin/release/2026"));
    assert!(prompt.contains("Ralph will reconcile queue/done bookkeeping"));
}

#[test]
fn integration_prompt_sanitizes_nul_bytes() {
    let queue_path = crate::testsupport::path::portable_abs_path("queue.json");
    let done_path = crate::testsupport::path::portable_abs_path("done.json");
    let prompt = build_agent_integration_prompt(
        "RQ-0001",
        "NUL test",
        "main",
        &queue_path,
        &done_path,
        1,
        5,
        "phase\0summary",
        "status\0snapshot",
        true,
        "make ci",
        Some("previous\0failure"),
    );

    assert!(!prompt.contains('\0'));
    assert!(prompt.contains("phase summary"));
    assert!(prompt.contains("status snapshot"));
    assert!(prompt.contains("previous failure"));
}

#[test]
fn compliance_result_all_passed() {
    let passed = ComplianceResult {
        has_unresolved_conflicts: false,
        queue_done_valid: true,
        task_archived: true,
        ci_passed: true,
        conflict_files: vec![],
        validation_error: None,
    };
    assert!(passed.all_passed());

    let failed = ComplianceResult {
        has_unresolved_conflicts: false,
        queue_done_valid: true,
        task_archived: false,
        ci_passed: true,
        conflict_files: vec![],
        validation_error: None,
    };
    assert!(!failed.all_passed());
}

#[test]
fn integration_config_uses_explicit_target_branch() -> anyhow::Result<()> {
    let dir = tempfile::TempDir::new()?;
    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: dir.path().to_path_buf(),
        queue_path: dir.path().join(".ralph/queue.json"),
        done_path: dir.path().join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let cfg = IntegrationConfig::from_resolved(&resolved, "release/2026");
    assert_eq!(cfg.target_branch, "release/2026");
    Ok(())
}

#[test]
fn task_archived_validation_uses_resolved_paths_not_workspace_local_files() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let coordinator = dir.path().join("coordinator");
    let worker_workspace = dir.path().join("worker-ws");
    std::fs::create_dir_all(&coordinator)?;
    std::fs::create_dir_all(worker_workspace.join(".ralph"))?;

    let coordinator_queue = coordinator.join("queue.json");
    let coordinator_done = coordinator.join("done.json");
    let workspace_queue = worker_workspace.join(".ralph/queue.json");
    let workspace_done = worker_workspace.join(".ralph/done.json");

    let mut coordinator_queue_file = QueueFile::default();
    coordinator_queue_file
        .tasks
        .push(make_task("RQ-0001", TaskStatus::Todo));
    crate::queue::save_queue(&coordinator_queue, &coordinator_queue_file)?;
    crate::queue::save_queue(&coordinator_done, &QueueFile::default())?;

    crate::queue::save_queue(&workspace_queue, &QueueFile::default())?;
    let mut workspace_done_file = QueueFile::default();
    workspace_done_file
        .tasks
        .push(make_task("RQ-0001", TaskStatus::Done));
    crate::queue::save_queue(&workspace_done, &workspace_done_file)?;

    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: worker_workspace,
        queue_path: coordinator_queue.clone(),
        done_path: coordinator_done,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let err = validate_task_archived(&resolved, "RQ-0001")
        .expect_err("validation should use resolved queue path");
    let msg = err.to_string();
    assert!(
        msg.contains(coordinator_queue.to_string_lossy().as_ref()),
        "error should reference resolved queue path, got: {msg}"
    );
    Ok(())
}

#[test]
fn queue_done_semantics_validation_uses_resolved_paths() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let coordinator = dir.path().join("coordinator");
    let worker_workspace = dir.path().join("worker-ws");
    std::fs::create_dir_all(&coordinator)?;
    std::fs::create_dir_all(worker_workspace.join(".ralph"))?;

    let coordinator_queue = coordinator.join("queue.json");
    let coordinator_done = coordinator.join("done.json");
    let workspace_queue = worker_workspace.join(".ralph/queue.json");
    let workspace_done = worker_workspace.join(".ralph/done.json");

    let mut invalid_queue = QueueFile::default();
    invalid_queue
        .tasks
        .push(make_task("BAD-ID", TaskStatus::Todo));
    crate::queue::save_queue(&coordinator_queue, &invalid_queue)?;
    crate::queue::save_queue(&coordinator_done, &QueueFile::default())?;

    let mut valid_queue = QueueFile::default();
    valid_queue
        .tasks
        .push(make_task("RQ-0001", TaskStatus::Todo));
    crate::queue::save_queue(&workspace_queue, &valid_queue)?;
    crate::queue::save_queue(&workspace_done, &QueueFile::default())?;

    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: worker_workspace.clone(),
        queue_path: coordinator_queue,
        done_path: coordinator_done,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    validate_queue_done_semantics(&worker_workspace, &resolved)
        .expect_err("validation should fail from resolved queue path");
    Ok(())
}

#[test]
fn blocked_marker_roundtrip() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    super::persistence::write_blocked_push_marker(temp.path(), "RQ-0001", "blocked reason", 5, 5)?;
    let marker = read_blocked_push_marker(temp.path())?.expect("marker should exist");
    assert_eq!(marker.task_id, "RQ-0001");
    assert_eq!(marker.reason, "blocked reason");
    assert_eq!(marker.attempt, 5);
    assert_eq!(marker.max_attempts, 5);

    super::persistence::clear_blocked_push_marker(temp.path());
    assert!(read_blocked_push_marker(temp.path())?.is_none());
    Ok(())
}

#[test]
fn machine_bookkeeping_rebuilds_from_latest_target_before_push() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let remote = temp.path().join("remote.git");
    std::fs::create_dir_all(&remote)?;
    git_test::init_bare_repo(&remote)?;

    let seed = temp.path().join("seed");
    std::fs::create_dir_all(&seed)?;
    git_test::init_repo(&seed)?;
    git_test::add_remote(&seed, "origin", &remote)?;

    let mut target_queue = QueueFile::default();
    target_queue
        .tasks
        .push(make_task("RQ-0001", TaskStatus::Todo));
    target_queue
        .tasks
        .push(make_task("RQ-0003", TaskStatus::Todo));
    let mut target_done = QueueFile::default();
    target_done
        .tasks
        .push(make_task("RQ-0002", TaskStatus::Done));
    crate::queue::save_queue(&seed.join(".ralph/queue.jsonc"), &target_queue)?;
    crate::queue::save_queue(&seed.join(".ralph/done.jsonc"), &target_done)?;
    std::fs::write(seed.join("README.md"), "base\n")?;
    git_test::commit_all(&seed, "seed queue")?;
    let branch = git_test::git_output(&seed, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::push_branch(&seed, &branch)?;
    git_test::git_run(
        &remote,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    )?;

    let worker = temp.path().join("worker");
    git_test::clone_repo(&remote, &worker)?;
    git_test::configure_user(&worker)?;

    let mut stale_queue = QueueFile::default();
    stale_queue
        .tasks
        .push(make_task("RQ-0001", TaskStatus::Todo));
    stale_queue
        .tasks
        .push(make_task("RQ-0002", TaskStatus::Todo));
    stale_queue
        .tasks
        .push(make_task("RQ-0003", TaskStatus::Todo));
    crate::queue::save_queue(&worker.join(".ralph/queue.jsonc"), &stale_queue)?;
    crate::queue::save_queue(&worker.join(".ralph/done.jsonc"), &QueueFile::default())?;
    std::fs::write(worker.join("work.txt"), "worker implementation\n")?;
    git_test::commit_all(&worker, "worker stale bookkeeping snapshot")?;

    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: worker.clone(),
        queue_path: worker.join(".ralph/queue.jsonc"),
        done_path: worker.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };
    let config = IntegrationConfig {
        max_attempts: 3,
        backoff_schedule: FixedBackoffSchedule::from_millis(&[1]),
        target_branch: branch.clone(),
        ci_enabled: false,
        ci_label: "disabled".to_string(),
    };

    let result = finalize_bookkeeping_and_push(&resolved, "RQ-0001", &config)?;
    assert!(
        result.pushed,
        "machine integration should push successfully"
    );

    git_test::git_run(&worker, &["fetch", "origin", &branch])?;
    let remote_subject = git_test::git_output(
        &worker,
        &["log", "-1", "--pretty=%s", &format!("origin/{branch}")],
    )?;
    let remote_queue_json = git_test::git_output(
        &worker,
        &["show", &format!("origin/{branch}:.ralph/queue.jsonc")],
    )?;
    let remote_done_json = git_test::git_output(
        &worker,
        &["show", &format!("origin/{branch}:.ralph/done.jsonc")],
    )?;
    let remote_queue: QueueFile = serde_json::from_str(&remote_queue_json)?;
    let remote_done: QueueFile = serde_json::from_str(&remote_done_json)?;

    assert_eq!(remote_subject, "ralph: archive RQ-0001 queue bookkeeping");
    assert_eq!(task_ids(&remote_queue), vec!["RQ-0003"]);
    assert_eq!(task_ids(&remote_done), vec!["RQ-0002", "RQ-0001"]);
    Ok(())
}

#[test]
fn machine_bookkeeping_applies_parallel_followup_proposal_before_push() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let remote = temp.path().join("remote.git");
    std::fs::create_dir_all(&remote)?;
    git_test::init_bare_repo(&remote)?;

    let seed = temp.path().join("seed");
    std::fs::create_dir_all(&seed)?;
    git_test::init_repo(&seed)?;
    git_test::add_remote(&seed, "origin", &remote)?;

    let mut source = make_task("RQ-0001", TaskStatus::Todo);
    source.request = Some("Audit docs and create follow-up work".to_string());
    let target_queue = QueueFile {
        version: 1,
        tasks: vec![source],
    };
    crate::queue::save_queue(&seed.join(".ralph/queue.jsonc"), &target_queue)?;
    crate::queue::save_queue(&seed.join(".ralph/done.jsonc"), &QueueFile::default())?;
    std::fs::write(seed.join("README.md"), "base\n")?;
    git_test::commit_all(&seed, "seed queue")?;
    let branch = git_test::git_output(&seed, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::push_branch(&seed, &branch)?;
    git_test::git_run(
        &remote,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    )?;

    let worker = temp.path().join("worker");
    git_test::clone_repo(&remote, &worker)?;
    git_test::configure_user(&worker)?;
    std::fs::write(worker.join("work.txt"), "worker implementation\n")?;
    git_test::commit_all(&worker, "worker implementation")?;
    write_parallel_followups(&worker, "RQ-0001", valid_parallel_followups())?;

    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: worker.clone(),
        queue_path: worker.join(".ralph/queue.jsonc"),
        done_path: worker.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };
    let config = IntegrationConfig {
        max_attempts: 3,
        backoff_schedule: FixedBackoffSchedule::from_millis(&[1]),
        target_branch: branch.clone(),
        ci_enabled: false,
        ci_label: "disabled".to_string(),
    };

    let result = finalize_bookkeeping_and_push(&resolved, "RQ-0001", &config)?;
    assert!(result.pushed);

    git_test::git_run(&worker, &["fetch", "origin", &branch])?;
    let remote_queue_json = git_test::git_output(
        &worker,
        &["show", &format!("origin/{branch}:.ralph/queue.jsonc")],
    )?;
    let remote_done_json = git_test::git_output(
        &worker,
        &["show", &format!("origin/{branch}:.ralph/done.jsonc")],
    )?;
    let remote_queue: QueueFile = serde_json::from_str(&remote_queue_json)?;
    let remote_done: QueueFile = serde_json::from_str(&remote_done_json)?;

    assert_eq!(task_ids(&remote_queue), vec!["RQ-0002", "RQ-0003"]);
    assert_eq!(task_ids(&remote_done), vec!["RQ-0001"]);
    assert_eq!(
        remote_queue.tasks[0].request.as_deref(),
        Some("Audit docs and create follow-up work")
    );
    assert_eq!(remote_queue.tasks[0].relates_to, vec!["RQ-0001"]);
    assert_eq!(remote_queue.tasks[1].depends_on, vec!["RQ-0002"]);
    assert!(!crate::queue::default_followups_path(&worker, "RQ-0001").exists());
    Ok(())
}

#[test]
fn machine_bookkeeping_keeps_followup_proposal_until_push_succeeds() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let remote = temp.path().join("remote.git");
    std::fs::create_dir_all(&remote)?;
    git_test::init_bare_repo(&remote)?;

    let seed = temp.path().join("seed");
    std::fs::create_dir_all(&seed)?;
    git_test::init_repo(&seed)?;
    git_test::add_remote(&seed, "origin", &remote)?;

    let mut source = make_task("RQ-0001", TaskStatus::Todo);
    source.request = Some("Audit docs and create follow-up work".to_string());
    crate::queue::save_queue(
        &seed.join(".ralph/queue.jsonc"),
        &QueueFile {
            version: 1,
            tasks: vec![source],
        },
    )?;
    crate::queue::save_queue(&seed.join(".ralph/done.jsonc"), &QueueFile::default())?;
    std::fs::write(seed.join("README.md"), "base\n")?;
    git_test::commit_all(&seed, "seed queue")?;
    let branch = git_test::git_output(&seed, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::push_branch(&seed, &branch)?;
    git_test::git_run(
        &remote,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    )?;

    let worker = temp.path().join("worker");
    git_test::clone_repo(&remote, &worker)?;
    git_test::configure_user(&worker)?;
    let proposal_path = write_parallel_followups(&worker, "RQ-0001", valid_parallel_followups())?;

    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: worker.clone(),
        queue_path: worker.join(".ralph/queue.jsonc"),
        done_path: worker.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let report = rebuild_bookkeeping_from_target(&resolved, "RQ-0001", &branch)?;

    assert!(report.is_some());
    assert!(proposal_path.exists());
    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(task_ids(&queue), vec!["RQ-0002", "RQ-0003"]);
    Ok(())
}

#[test]
fn machine_bookkeeping_blocks_invalid_parallel_followup_proposal() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let remote = temp.path().join("remote.git");
    std::fs::create_dir_all(&remote)?;
    git_test::init_bare_repo(&remote)?;

    let seed = temp.path().join("seed");
    std::fs::create_dir_all(&seed)?;
    git_test::init_repo(&seed)?;
    git_test::add_remote(&seed, "origin", &remote)?;

    let target_queue = QueueFile {
        version: 1,
        tasks: vec![make_task("RQ-0001", TaskStatus::Todo)],
    };
    crate::queue::save_queue(&seed.join(".ralph/queue.jsonc"), &target_queue)?;
    crate::queue::save_queue(&seed.join(".ralph/done.jsonc"), &QueueFile::default())?;
    std::fs::write(seed.join("README.md"), "base\n")?;
    git_test::commit_all(&seed, "seed queue")?;
    let branch = git_test::git_output(&seed, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::push_branch(&seed, &branch)?;
    git_test::git_run(
        &remote,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    )?;

    let worker = temp.path().join("worker");
    git_test::clone_repo(&remote, &worker)?;
    git_test::configure_user(&worker)?;
    std::fs::write(worker.join("work.txt"), "worker implementation\n")?;
    git_test::commit_all(&worker, "worker implementation")?;

    let mut proposal = valid_parallel_followups();
    proposal["tasks"][0]["depends_on_keys"] = serde_json::json!(["missing-key"]);
    let proposal_path = write_parallel_followups(&worker, "RQ-0001", proposal)?;

    let resolved = crate::config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: worker.clone(),
        queue_path: worker.join(".ralph/queue.jsonc"),
        done_path: worker.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };
    let config = IntegrationConfig {
        max_attempts: 3,
        backoff_schedule: FixedBackoffSchedule::from_millis(&[1]),
        target_branch: branch.clone(),
        ci_enabled: false,
        ci_label: "disabled".to_string(),
    };

    let result = finalize_bookkeeping_and_push(&resolved, "RQ-0001", &config)?;
    assert!(!result.pushed);
    assert!(
        result
            .push_error
            .as_deref()
            .is_some_and(|error| error.contains("apply parallel worker follow-up proposal"))
    );
    assert!(proposal_path.exists());

    git_test::git_run(&worker, &["fetch", "origin", &branch])?;
    let remote_queue_json = git_test::git_output(
        &worker,
        &["show", &format!("origin/{branch}:.ralph/queue.jsonc")],
    )?;
    let remote_queue: QueueFile = serde_json::from_str(&remote_queue_json)?;
    assert_eq!(task_ids(&remote_queue), vec!["RQ-0001"]);
    Ok(())
}

fn valid_parallel_followups() -> serde_json::Value {
    serde_json::json!({
        "version": 1,
        "source_task_id": "RQ-0001",
        "tasks": [
            {
                "key": "docs-hierarchy",
                "title": "Rework docs hierarchy",
                "description": "Split oversized docs into a navigable hierarchy.",
                "priority": "high",
                "tags": ["docs"],
                "scope": ["docs/"],
                "evidence": ["Audit found oversized docs."],
                "plan": ["Design hierarchy.", "Move content."],
                "depends_on_keys": [],
                "independence_rationale": "Separate remediation discovered by the active task."
            },
            {
                "key": "cli-flags",
                "title": "Document CLI flags",
                "description": "Add deeper CLI flag coverage.",
                "priority": "medium",
                "tags": ["docs", "cli"],
                "scope": ["docs/cli.md"],
                "evidence": ["Audit found shallow CLI coverage."],
                "plan": ["Inventory flags.", "Expand docs."],
                "depends_on_keys": ["docs-hierarchy"],
                "independence_rationale": "Depends on the docs hierarchy but is separate work."
            }
        ]
    })
}

fn write_parallel_followups(
    repo_root: &std::path::Path,
    task_id: &str,
    proposal: serde_json::Value,
) -> anyhow::Result<std::path::PathBuf> {
    let path = crate::queue::default_followups_path(repo_root, task_id);
    crate::fsutil::write_atomic(&path, serde_json::to_string_pretty(&proposal)?.as_bytes())?;
    Ok(path)
}
