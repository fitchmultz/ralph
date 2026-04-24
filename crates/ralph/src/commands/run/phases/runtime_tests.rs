//! Run phase orchestration tests grouped by behavior.
//!
//! Purpose:
//! - Run phase orchestration tests grouped by behavior.
//!
//! Responsibilities:
//! - Provide shared fixtures for phase orchestration scenario tests.
//! - Keep phase-specific scenarios in focused submodules.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::super::phase2::cache_phase2_final_response;
use super::super::phase3::ensure_phase3_completion;
use super::super::shared::run_ci_gate_with_continue;
use super::super::{
    PhaseInvocation, PostRunMode, execute_phase1_planning, execute_phase2_implementation,
    execute_phase3_review, generate_phase_session_id, phase_session_id_for_runner,
};
use crate::commands::run::supervision::ContinueSession;
use crate::constants::defaults::PHASE2_FINAL_RESPONSE_FALLBACK;
use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
use crate::contracts::{
    ClaudePermissionMode, Config, GitRevertMode, Model, QueueConfig, QueueFile, ReasoningEffort,
    Runner, Task, TaskPriority, TaskStatus,
};
use crate::queue;
use crate::testsupport::runner::create_fake_runner;
use crate::testsupport::{INTERRUPT_TEST_MUTEX, reset_ctrlc_interrupt_flag};
use crate::{git, promptflow, runner, runutil};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

fn git_status_ok(dir: &Path, args: &[&str], description: &str) -> Result<()> {
    let _path_guard = crate::testsupport::path::path_lock()
        .lock()
        .expect("path lock");
    let status = Command::new("git").current_dir(dir).args(args).status()?;
    anyhow::ensure!(status.success(), "{description}");
    Ok(())
}

fn git_init(dir: &Path) -> Result<()> {
    git_status_ok(dir, &["init", "--quiet"], "git init failed")?;

    let gitignore_path = dir.join(".gitignore");
    std::fs::write(&gitignore_path, ".ralph/lock\n.ralph/cache/\nbin/\n")?;
    git_status_ok(dir, &["add", ".gitignore"], "git add .gitignore failed")?;
    git_status_ok(
        dir,
        &["commit", "--quiet", "-m", "add gitignore"],
        "git commit .gitignore failed",
    )?;

    Ok(())
}

#[test]
fn cache_phase2_final_response_writes_detected_message() -> Result<()> {
    let temp = TempDir::new()?;
    let stdout = concat!(
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Draft"}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Final answer"}}"#,
        "\n"
    );
    cache_phase2_final_response(temp.path(), "RQ-0001", stdout)?;
    let cached = promptflow::read_phase2_final_response_cache(temp.path(), "RQ-0001")?;
    assert_eq!(cached, "Final answer");
    Ok(())
}

#[test]
fn cache_phase2_final_response_writes_fallback_when_missing() -> Result<()> {
    let temp = TempDir::new()?;
    let stdout = r#"{"type":"tool_use","tool_name":"read"}"#;
    cache_phase2_final_response(temp.path(), "RQ-0001", stdout)?;
    let cached = promptflow::read_phase2_final_response_cache(temp.path(), "RQ-0001")?;
    assert_eq!(cached, PHASE2_FINAL_RESPONSE_FALLBACK);
    Ok(())
}

#[test]
fn generate_phase_session_id_uses_task_phase_and_timestamp_format() {
    let task_id = "RQ-0001";
    let session_id = generate_phase_session_id(task_id, 2);
    let prefix = format!("{task_id}-p2-");
    assert!(
        session_id.starts_with(&prefix),
        "expected prefix {prefix}, got {session_id}"
    );
    assert!(!session_id.starts_with("ralph-"));
    let suffix = session_id.strip_prefix(&prefix).expect("session id prefix");
    assert!(
        !suffix.is_empty(),
        "expected timestamp suffix, got empty string"
    );
    assert!(
        suffix.chars().all(|c| c.is_ascii_digit()),
        "timestamp suffix should be digits, got {suffix}"
    );
}

#[test]
fn phase_session_id_for_runner_only_returns_for_kimi() {
    let task_id = "RQ-0009";
    let kimi_id = phase_session_id_for_runner(Runner::Kimi, task_id, 2);
    assert!(
        kimi_id.is_some(),
        "expected kimi session id to be generated"
    );
    let opencode_id = phase_session_id_for_runner(Runner::Opencode, task_id, 2);
    assert!(
        opencode_id.is_none(),
        "expected opencode session id to be extracted from runner output"
    );
}

fn resolved_for_repo(repo_root: PathBuf, opencode_bin: &Path) -> crate::config::Resolved {
    let mut cfg = Config::default();
    cfg.agent.runner = Some(Runner::Opencode);
    cfg.agent.model = Some(Model::Custom("zai-coding-plan/glm-4.7".to_string()));
    cfg.agent.reasoning_effort = Some(ReasoningEffort::Medium);
    cfg.agent.phases = Some(2);
    cfg.agent.claude_permission_mode = Some(ClaudePermissionMode::BypassPermissions);
    cfg.agent.git_revert_mode = Some(GitRevertMode::Ask);
    cfg.agent.git_publish_mode = Some(crate::contracts::GitPublishMode::CommitAndPush);
    cfg.agent.repoprompt_plan_required = Some(false);
    cfg.agent.repoprompt_tool_injection = Some(false);
    cfg.agent.opencode_bin = Some(opencode_bin.display().to_string());
    cfg.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(false),
        argv: None,
    });
    cfg.queue = QueueConfig {
        file: Some(PathBuf::from(".ralph/queue.jsonc")),
        done_file: Some(PathBuf::from(".ralph/done.jsonc")),
        id_prefix: Some("RQ".to_string()),
        id_width: Some(4),
        size_warning_threshold_kb: Some(500),
        task_count_warning_threshold: Some(500),
        max_dependency_depth: Some(10),
        auto_archive_terminal_after_days: None,
        aging_thresholds: None,
    };

    crate::config::Resolved {
        config: cfg,
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.jsonc"),
        done_path: repo_root.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
    }
}

fn resolved_for_completion(repo_root: PathBuf) -> crate::config::Resolved {
    crate::config::Resolved {
        config: Config::default(),
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.jsonc"),
        done_path: repo_root.join(".ralph/done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
    }
}

fn write_queue_and_done(repo_root: &Path, status: TaskStatus) -> Result<()> {
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
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
        completed_at: Some("2026-01-18T00:00:00Z".to_string()),
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    queue::save_queue(
        &repo_root.join(".ralph/queue.jsonc"),
        &QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    queue::save_queue(
        &repo_root.join(".ralph/done.jsonc"),
        &QueueFile {
            version: 1,
            tasks: vec![task],
        },
    )?;
    git_status_ok(
        repo_root,
        &["add", "-f", ".ralph/queue.jsonc", ".ralph/done.jsonc"],
        "git add queue bookkeeping failed",
    )?;
    Ok(())
}

fn trust_repo(repo_root: &Path) -> Result<()> {
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    std::fs::write(
        repo_root.join(".ralph/trust.jsonc"),
        "{\n  \"allow_project_commands\": true\n}\n",
    )?;
    Ok(())
}

#[path = "runtime_tests/ci_gate.rs"]
mod ci_gate;
#[path = "runtime_tests/phase1_followup.rs"]
mod phase1_followup;
#[path = "runtime_tests/phase1_guardrails.rs"]
mod phase1_guardrails;
#[path = "runtime_tests/phase1_ralph_paths.rs"]
mod phase1_ralph_paths;
#[path = "runtime_tests/phase3.rs"]
mod phase3;
