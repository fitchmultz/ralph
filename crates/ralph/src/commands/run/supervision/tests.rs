//! Tests for supervision module.
//!
//! Responsibilities:
//! - Unit tests for continue_session, parallel_worker, and core orchestration.
//!
//! Not handled here:
//! - Integration tests (see tests/ directory).

use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
use crate::contracts::{
    AgentConfig, Config, NotificationConfig, QueueConfig, QueueFile, Runner, RunnerRetryConfig,
    Task, TaskPriority, TaskStatus,
};
use crate::queue;
use crate::testsupport::git as git_test;
use crate::testsupport::runner::create_fake_runner;
use crate::testsupport::{INTERRUPT_TEST_MUTEX, reset_ctrlc_interrupt_flag};

use super::*;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use tempfile::TempDir;

static PI_ENV_MUTEX: Mutex<()> = Mutex::new(());

fn write_queue(repo_root: &Path, status: TaskStatus) -> anyhow::Result<()> {
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
            model: Some(crate::contracts::Model::Gpt52Codex),
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
                shell: None,
            }),
            git_revert_mode: Some(GitRevertMode::Disabled),
            git_commit_push_enabled: Some(true),
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
fn resume_continue_session_falls_back_to_fresh_invocation_without_session_id() -> anyhow::Result<()>
{
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
echo "$@" > "{args_path}"
echo '{{"type":"text","part":{{"text":"fresh"}}}}'
echo '{{"sessionID":"sess-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "opencode", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.opencode_bin = Some(runner_path.to_string_lossy().to_string());

    let mut session = ContinueSession {
        runner: Runner::Opencode,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: None,
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let (_output, _elapsed) = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "-s"),
        "fresh invocation should not include resume session args, got: {args}"
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_pi_falls_back_to_fresh_when_resume_lookup_fails() -> anyhow::Result<()> {
    let _env_guard = PI_ENV_MUTEX.lock().expect("pi env mutex poisoned");
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("pi-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
echo "$@" > "{args_path}"
echo '{{"type":"result","result":"fresh"}}'
echo '{{"sessionID":"sess-pi-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "pi", &runner_script)?;

    let previous_pi_root = std::env::var_os("PI_CODING_AGENT_DIR");
    let pi_root = temp_dir.path().join("pi-root");
    std::fs::create_dir_all(&pi_root)?;
    // SAFETY: Test serializes PI env mutation with PI_ENV_MUTEX.
    unsafe { std::env::set_var("PI_CODING_AGENT_DIR", &pi_root) };

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.pi_bin = Some(runner_path.to_string_lossy().to_string());

    let mut session = ContinueSession {
        runner: Runner::Pi,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: Some("missing-session-id".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let result = resume_continue_session(&resolved, &mut session, "hello", None);

    match previous_pi_root {
        Some(value) => {
            // SAFETY: Test serializes PI env mutation with PI_ENV_MUTEX.
            unsafe { std::env::set_var("PI_CODING_AGENT_DIR", value) };
        }
        None => {
            // SAFETY: Test serializes PI env mutation with PI_ENV_MUTEX.
            unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };
        }
    }

    let (_output, _elapsed) = result?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "--session"),
        "fresh invocation should not include --session args, got: {args}"
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-pi-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_gemini_falls_back_to_fresh_on_invalid_resume() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("gemini-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
if printf '%s' "$*" | grep -q -- '--resume'; then
  echo 'Error resuming session: Invalid session identifier "does-not-exist".' >&2
  echo '  Use --list-sessions to see available sessions.' >&2
  exit 42
fi
echo "$*" > "{args_path}"
echo '{{"type":"message","role":"assistant","content":"fresh"}}'
echo '{{"session_id":"sess-gemini-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "gemini", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.gemini_bin = Some(runner_path.to_string_lossy().to_string());

    let mut session = ContinueSession {
        runner: Runner::Gemini,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: Some("does-not-exist".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let (_output, _elapsed) = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "--resume"),
        "fresh invocation should not include --resume args, got: {args}"
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-gemini-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_claude_falls_back_to_fresh_on_invalid_uuid() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("claude-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
if printf '%s' "$*" | grep -q -- '--resume'; then
  echo '{{"type":"result","is_error":true,"errors":["--resume requires a valid session ID"]}}'
  exit 1
fi
echo "$*" > "{args_path}"
echo '{{"type":"assistant","session_id":"sess-claude-fresh","message":{{"content":[{{"type":"text","text":"fresh"}}]}}}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "claude", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.claude_bin = Some(runner_path.to_string_lossy().to_string());

    let mut session = ContinueSession {
        runner: Runner::Claude,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: Some("not-a-uuid".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let (_output, _elapsed) = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "--resume"),
        "fresh invocation should not include --resume args, got: {args}"
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-claude-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_opencode_falls_back_when_resume_errors_with_exit_zero()
-> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("opencode-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
for arg in "$@"; do
  if [ "$arg" = "-s" ] || [ "$arg" = "--session" ]; then
    echo 'ZodError: invalid_format sessionID' >&2
    echo 'Invalid string: must start with "ses"' >&2
    exit 0
  fi
done
echo "$*" > "{args_path}"
echo '{{"type":"text","part":{{"text":"fresh"}}}}'
echo '{{"sessionID":"sess-opencode-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "opencode", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.opencode_bin = Some(runner_path.to_string_lossy().to_string());

    let mut session = ContinueSession {
        runner: Runner::Opencode,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: Some("bad-session".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let (_output, _elapsed) = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "-s"),
        "fresh invocation should not include -s args, got: {args}"
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-opencode-fresh"));
    Ok(())
}

#[test]
fn post_run_supervise_commits_and_cleans_when_enabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        true,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(status.trim().is_empty(), "expected clean repo");

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|t| t.id == "RQ-0001"),
        "expected task in done archive"
    );

    Ok(())
}

#[test]
fn post_run_supervise_skips_commit_when_disabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        false,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(!status.trim().is_empty(), "expected dirty repo");
    Ok(())
}

#[test]
fn post_run_supervise_backfills_missing_completed_at() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Done)?;
    git_test::commit_all(temp.path(), "init")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        false,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let task = done_file
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("expected task in done archive");
    let completed_at = task
        .completed_at
        .as_deref()
        .expect("completed_at should be stamped");

    crate::timeutil::parse_rfc3339(completed_at)?;

    Ok(())
}

#[test]
fn post_run_supervise_errors_on_push_failure_when_enabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
    let missing_remote = temp.path().join("missing-remote");
    git_test::git_run(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote.to_str().unwrap(),
        ],
    )?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    let err = post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        true,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )
    .expect_err("expected push failure");
    assert!(format!("{err:#}").contains("Git push failed"));
    Ok(())
}

#[test]
fn post_run_supervise_skips_push_when_disabled() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    let remote = TempDir::new()?;
    git_test::git_run(remote.path(), &["init", "--bare"])?;
    let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    git_test::git_run(
        temp.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    )?;
    git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
    let missing_remote = temp.path().join("missing-remote");
    git_test::git_run(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            missing_remote.to_str().unwrap(),
        ],
    )?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        false,
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;
    Ok(())
}

#[test]
fn post_run_supervise_allows_productivity_json_dirty() -> anyhow::Result<()> {
    // Regression test: ensure productivity.json doesn't block task completion
    // See: supervisor triggering revert prompt due to productivity.json being dirty
    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Done)?;
    git_test::commit_all(temp.path(), "init")?;

    // Create the cache directory and productivity.json file (simulating what
    // trigger_celebration does when recording stats)
    let cache_dir = temp.path().join(".ralph").join("cache");
    std::fs::create_dir_all(&cache_dir)?;
    std::fs::write(
        cache_dir.join("productivity.json"),
        r#"{"version":1,"total_completed":1}"#,
    )?;

    // Also create a real work file that should be committed
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resolved = resolved_for_repo(temp.path());
    // This should succeed even though productivity.json is untracked
    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        true, // git_commit_push_enabled = true
        PushPolicy::RequireUpstream,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
    )?;

    // Verify the task is in done
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    anyhow::ensure!(
        done_file.tasks.iter().any(|t| t.id == "RQ-0001"),
        "expected task in done archive"
    );

    // Verify the repo is clean (productivity.json was committed along with other changes)
    let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
    anyhow::ensure!(
        status.trim().is_empty(),
        "expected clean repo after commit, but found: {}",
        status
    );

    Ok(())
}

#[test]
fn post_run_supervise_ci_gate_continue_resumes_session() -> anyhow::Result<()> {
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;

    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resume_args = temp.path().join("resume-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
echo "$@" > "{resume_args}"
echo '{{"type":"text","part":{{"text":"resume"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
        resume_args = resume_args.display()
    );
    let runner_path = create_fake_runner(temp.path(), "opencode", &runner_script)?;

    let ci_pass = temp.path().join("ci-pass.txt");
    let ci_command = format!("test -f {}", ci_pass.display());

    let mut resolved = resolved_for_repo(temp.path());
    std::fs::write(
        temp.path().join(".ralph/trust.jsonc"),
        r#"{"allow_project_commands": true}"#,
    )?;
    resolved.config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: None,
        shell: Some(crate::contracts::ShellCommandConfig {
            mode: Some(crate::contracts::ShellMode::Posix),
            command: Some(ci_command),
        }),
    });
    resolved.config.agent.opencode_bin = Some(runner_path.to_str().unwrap().to_string());

    let prompt_handler: runutil::RevertPromptHandler = Arc::new(|_context| {
        Ok(runutil::RevertDecision::Continue {
            message: "fix the ci gate".to_string(),
        })
    });

    let mut continue_session = ContinueSession {
        runner: Runner::Opencode,
        model: crate::contracts::Model::Custom("test-model".to_string()),
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: crate::commands::run::PhaseType::Review,
        session_id: Some("sess-123".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: CI_GATE_AUTO_RETRY_LIMIT,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let mut on_resume = |_output: &crate::runner::RunnerOutput,
                         _elapsed: std::time::Duration|
     -> anyhow::Result<()> {
        std::fs::write(&ci_pass, "ok")?;
        Ok(())
    };

    post_run_supervise(
        &resolved,
        "RQ-0001",
        GitRevertMode::Ask,
        false,
        PushPolicy::RequireUpstream,
        Some(prompt_handler),
        Some(CiContinueContext {
            continue_session: &mut continue_session,
            on_resume: &mut on_resume,
        }),
        None,
        None,
        false,
        false,
        None,
    )?;

    let args = std::fs::read_to_string(&resume_args)?;
    anyhow::ensure!(
        args.contains("fix the ci gate"),
        "expected resume args to include continue message, got: {}",
        args
    );

    Ok(())
}

#[test]
fn continue_session_preserves_runner_cli_options() {
    // Verify that ContinueSession correctly stores and preserves runner_cli options.
    // This is a regression test for the bug where runner_cli was re-resolved from
    // config on Continue, losing CLI overrides.
    use crate::contracts::{
        RunnerApprovalMode, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
        UnsupportedOptionPolicy,
    };

    let custom_runner_cli = crate::runner::ResolvedRunnerCliOptions {
        output_format: RunnerOutputFormat::StreamJson,
        verbosity: RunnerVerbosity::Quiet,
        approval_mode: RunnerApprovalMode::Safe,
        sandbox: RunnerSandboxMode::Enabled,
        plan_mode: RunnerPlanMode::Enabled,
        unsupported_option_policy: UnsupportedOptionPolicy::Error,
    };

    let session = ContinueSession {
        runner: Runner::Codex,
        model: crate::contracts::Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: custom_runner_cli,
        phase_type: crate::commands::run::PhaseType::Implementation,
        session_id: Some("test-session".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    // Verify the stored runner_cli matches what was set
    assert_eq!(session.runner_cli.verbosity, RunnerVerbosity::Quiet);
    assert_eq!(session.runner_cli.approval_mode, RunnerApprovalMode::Safe);
    assert_eq!(session.runner_cli.sandbox, RunnerSandboxMode::Enabled);
    assert_eq!(session.runner_cli.plan_mode, RunnerPlanMode::Enabled);
    assert_eq!(
        session.runner_cli.unsupported_option_policy,
        UnsupportedOptionPolicy::Error
    );
}

#[test]
fn continue_session_preserves_phase_type() {
    // Verify that ContinueSession correctly stores and preserves the phase type.
    // This is a regression test for the bug where PhaseType::Implementation was
    // hardcoded for all continues, breaking phase-aware runners.
    use crate::commands::run::PhaseType;

    // Test Planning phase
    let planning_session = ContinueSession {
        runner: Runner::Codex,
        model: crate::contracts::Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: PhaseType::Planning,
        session_id: Some("test-session".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };
    assert_eq!(planning_session.phase_type, PhaseType::Planning);

    // Test Implementation phase
    let impl_session = ContinueSession {
        runner: Runner::Codex,
        model: crate::contracts::Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: PhaseType::Implementation,
        session_id: Some("test-session".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };
    assert_eq!(impl_session.phase_type, PhaseType::Implementation);

    // Test Review phase
    let review_session = ContinueSession {
        runner: Runner::Codex,
        model: crate::contracts::Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: PhaseType::Review,
        session_id: Some("test-session".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };
    assert_eq!(review_session.phase_type, PhaseType::Review);

    // Test SinglePhase
    let single_session = ContinueSession {
        runner: Runner::Codex,
        model: crate::contracts::Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
        phase_type: PhaseType::SinglePhase,
        task_id: "RQ-0001".to_string(),
        session_id: Some("test-session".to_string()),
        output_handler: None,
        output_stream: crate::runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };
    assert_eq!(single_session.phase_type, PhaseType::SinglePhase);
}

#[test]
fn post_run_parallel_worker_restores_bookkeeping_without_signals() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_root = temp_dir.path();
    git_test::init_repo(repo_root)?;

    let cache_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir)?;

    write_queue(repo_root, TaskStatus::Todo)?;
    queue::save_queue(
        &repo_root.join(".ralph/done.json"),
        &QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    let productivity_path = cache_dir.join("productivity.json");
    std::fs::write(&productivity_path, "{\"stats\":[]}")?;
    git_test::commit_all(repo_root, "init queue/done/productivity")?;

    let resolved = resolved_for_repo(repo_root);
    let queue_before = std::fs::read_to_string(&resolved.queue_path)?;
    let done_before = std::fs::read_to_string(&resolved.done_path)?;
    let productivity_before = std::fs::read_to_string(&productivity_path)?;

    // Dirty the bookkeeping files
    std::fs::write(&resolved.queue_path, "{\"version\":1,\"tasks\":[]}")?;
    std::fs::write(&resolved.done_path, "{\"version\":1,\"tasks\":[]}")?;
    std::fs::write(&productivity_path, "{\"stats\":[\"changed\"]}")?;

    post_run_supervise_parallel_worker(
        &resolved,
        "RQ-0001",
        GitRevertMode::Disabled,
        false,
        PushPolicy::RequireUpstream,
        None,
        None,
        false,
        None,
    )?;

    assert_eq!(std::fs::read_to_string(&resolved.queue_path)?, queue_before);
    assert_eq!(std::fs::read_to_string(&resolved.done_path)?, done_before);
    assert_eq!(
        std::fs::read_to_string(&productivity_path)?,
        productivity_before
    );

    let status_paths = git::status_paths(repo_root)?;
    let queue_rel = resolved
        .queue_path
        .strip_prefix(repo_root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let done_rel = resolved
        .done_path
        .strip_prefix(repo_root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let productivity_rel = productivity_path
        .strip_prefix(repo_root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    assert!(
        !status_paths.contains(&queue_rel),
        "queue.json should be restored to HEAD"
    );
    assert!(
        !status_paths.contains(&done_rel),
        "done.json should be restored to HEAD"
    );
    assert!(
        !status_paths.contains(&productivity_rel),
        "productivity.json should be restored to HEAD"
    );

    Ok(())
}
