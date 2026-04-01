//! Scan command regression coverage.
//!
//! Responsibilities:
//! - Verify scan runner settings resolve approval behavior correctly.
//! - Cover preflight validation failure and post-run task backfill behavior.
//!
//! Does not handle:
//! - Prompt template contents.
//! - Full CLI argument parsing (covered by integration tests).
//!
//! Assumptions/invariants:
//! - Invalid queues should fail before runner execution.
//! - Newly added scan tasks should receive default request/timestamp backfills.

use super::*;

use crate::contracts::{
    ClaudePermissionMode, Config, GitRevertMode, Model, QueueFile, Runner, RunnerApprovalMode,
    RunnerCliConfigRoot, RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode,
    RunnerSandboxMode, RunnerVerbosity, Task, TaskStatus, UnsupportedOptionPolicy,
};
use crate::queue::{load_queue, save_queue};
use crate::testsupport::git as git_test;
use crate::testsupport::runner::create_fake_runner;
use std::collections::{BTreeMap, HashMap};
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

fn scan_opts() -> ScanOptions {
    ScanOptions {
        focus: "scan".to_string(),
        mode: ScanMode::Maintenance,
        runner_override: None,
        model_override: None,
        reasoning_effort_override: None,
        runner_cli_overrides: RunnerCliOptionsPatch::default(),
        force: false,
        repoprompt_tool_injection: false,
        git_revert_mode: GitRevertMode::Ask,
        lock_mode: ScanLockMode::Held,
        output_handler: None,
        revert_prompt: None,
    }
}

fn scan_task(id: &str, title: &str) -> Task {
    Task {
        id: id.to_string(),
        status: TaskStatus::Todo,
        title: title.to_string(),
        description: None,
        priority: Default::default(),
        tags: Vec::new(),
        scope: Vec::new(),
        evidence: Vec::new(),
        plan: Vec::new(),
        notes: Vec::new(),
        request: Some("seed request".to_string()),
        agent: None,
        created_at: Some("2026-04-01T00:00:00Z".to_string()),
        updated_at: Some("2026-04-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        estimated_minutes: None,
        actual_minutes: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

fn scan_task_missing_request(id: &str, title: &str) -> Task {
    Task {
        request: None,
        ..scan_task(id, title)
    }
}

fn initialize_scan_repo(resolved: &config::Resolved) -> anyhow::Result<()> {
    git_test::init_repo(&resolved.repo_root)?;
    std::fs::create_dir_all(
        resolved
            .queue_path
            .parent()
            .expect("queue parent should exist"),
    )?;
    std::fs::write(resolved.repo_root.join("README.md"), "# scan test\n")?;
    save_queue(&resolved.queue_path, &QueueFile::default())?;
    save_queue(&resolved.done_path, &QueueFile::default())?;
    git_test::commit_all(&resolved.repo_root, "init scan repo")?;
    Ok(())
}

#[test]
fn scan_respects_config_permission_mode_when_approval_default() {
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
    let settings = resolve_scan_runner_settings(&resolved, &scan_opts()).expect("settings");
    let effective = settings
        .runner_cli
        .effective_claude_permission_mode(settings.permission_mode);
    assert_eq!(effective, Some(ClaudePermissionMode::AcceptEdits));
}

#[test]
fn scan_cli_override_yolo_bypasses_permission_mode() {
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

    let mut opts = scan_opts();
    opts.runner_cli_overrides = RunnerCliOptionsPatch {
        approval_mode: Some(RunnerApprovalMode::Yolo),
        ..RunnerCliOptionsPatch::default()
    };

    let (resolved, _dir) = resolved_with_config(config);
    let settings = resolve_scan_runner_settings(&resolved, &opts).expect("settings");
    let effective = settings
        .runner_cli
        .effective_claude_permission_mode(settings.permission_mode);
    assert_eq!(effective, Some(ClaudePermissionMode::BypassPermissions));
}

#[test]
fn scan_fails_fast_when_safe_approval_requires_prompt() {
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
    let err = resolve_scan_runner_settings(&resolved, &scan_opts()).expect_err("error");
    assert!(err.to_string().contains("approval_mode=safe"));
}

#[test]
fn run_scan_backfills_new_tasks_added_by_runner() -> anyhow::Result<()> {
    let (mut resolved, _dir) = resolved_with_config(Config::default());
    initialize_scan_repo(&resolved)?;

    let queue_after = QueueFile {
        version: 1,
        tasks: vec![scan_task_missing_request("RQ-0001", "Follow up on TODOs")],
    };
    let queue_after_path = resolved.repo_root.join(".ralph/cache/queue-after.json");
    std::fs::create_dir_all(
        queue_after_path
            .parent()
            .expect("queue-after parent should exist"),
    )?;
    std::fs::write(
        &queue_after_path,
        serde_json::to_string_pretty(&queue_after)?,
    )?;

    let runner_script = format!(
        r#"#!/bin/sh
set -e
cp "{queue_after}" "{queue_path}"
echo '{{"type":"item.completed","item":{{"type":"agent_message","text":"scan complete"}}}}'
"#,
        queue_after = queue_after_path.display(),
        queue_path = resolved.queue_path.display(),
    );
    let runner_dir = TempDir::new()?;
    let runner_path = create_fake_runner(runner_dir.path(), "codex", &runner_script)?;
    resolved.config.agent.codex_bin = Some(runner_path.to_string_lossy().to_string());

    let mut opts = scan_opts();
    opts.focus = "review TODO coverage".to_string();
    opts.git_revert_mode = GitRevertMode::Disabled;
    opts.runner_override = Some(Runner::Codex);
    opts.model_override = Some(Model::Gpt53Codex);

    run_scan(&resolved, opts)?;

    let queue = load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 1);
    let task = &queue.tasks[0];
    assert_eq!(task.id, "RQ-0001");
    assert_eq!(task.title, "Follow up on TODOs");
    assert_eq!(task.request.as_deref(), Some("scan: review TODO coverage"));
    assert_eq!(task.created_at.as_deref(), Some("2026-04-01T00:00:00Z"));
    assert_eq!(task.updated_at.as_deref(), Some("2026-04-01T00:00:00Z"));
    Ok(())
}

#[test]
fn run_scan_fails_before_runner_when_queue_is_invalid() -> anyhow::Result<()> {
    let (mut resolved, _dir) = resolved_with_config(Config::default());
    initialize_scan_repo(&resolved)?;

    save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![
                scan_task("RQ-0001", "First"),
                scan_task("RQ-0001", "Duplicate"),
            ],
        },
    )?;

    let sentinel = resolved.repo_root.join("runner-was-called");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
touch "{sentinel}"
echo '{{"type":"item.completed","item":{{"type":"agent_message","text":"should not run"}}}}'
"#,
        sentinel = sentinel.display(),
    );
    let runner_dir = TempDir::new()?;
    let runner_path = create_fake_runner(runner_dir.path(), "codex", &runner_script)?;
    resolved.config.agent.codex_bin = Some(runner_path.to_string_lossy().to_string());

    let mut opts = scan_opts();
    opts.git_revert_mode = GitRevertMode::Disabled;
    opts.runner_override = Some(Runner::Codex);
    opts.model_override = Some(Model::Gpt53Codex);

    let err = run_scan(&resolved, opts).expect_err("invalid queue should fail before runner");
    let message = format!("{err:#}");
    assert!(message.contains("Scan validation failed before run."));
    assert!(
        !sentinel.exists(),
        "runner should not execute when queue validation fails"
    );
    Ok(())
}
