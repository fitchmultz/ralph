//! Tests for run phase orchestration helpers.

use super::phase2::{cache_phase2_final_response, PHASE2_FINAL_RESPONSE_FALLBACK};
use super::phase3::ensure_phase3_completion;
use super::shared::run_ci_gate_with_continue;
use super::{execute_phase1_planning, execute_phase3_review, PhaseInvocation};
use crate::commands::run::supervision::ContinueSession;
use crate::completions;
use crate::contracts::{
    ClaudePermissionMode, Config, GitRevertMode, Model, QueueConfig, QueueFile, ReasoningEffort,
    Runner, Task, TaskPriority, TaskStatus,
};
use crate::queue;
use crate::{git, promptflow, runner, runutil};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

fn git_init(dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["init", "--quiet"])
        .status()?;
    anyhow::ensure!(status.success(), "git init failed");

    let gitignore_path = dir.join(".gitignore");
    std::fs::write(&gitignore_path, ".ralph/lock\n.ralph/cache/\nbin/\n")?;
    Command::new("git")
        .current_dir(dir)
        .args(["add", ".gitignore"])
        .status()?;
    Command::new("git")
        .current_dir(dir)
        .args(["commit", "--quiet", "-m", "add gitignore"])
        .status()?;

    Ok(())
}

fn create_fake_runner(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir(&bin_dir)?;
    let runner_path = bin_dir.join(name);
    std::fs::write(&runner_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    Ok(runner_path)
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

fn resolved_for_repo(repo_root: PathBuf, opencode_bin: &Path) -> crate::config::Resolved {
    let mut cfg = Config::default();
    cfg.agent.runner = Some(Runner::Opencode);
    cfg.agent.model = Some(Model::Custom("zai-coding-plan/glm-4.7".to_string()));
    cfg.agent.reasoning_effort = Some(ReasoningEffort::Medium);
    cfg.agent.phases = Some(2);
    cfg.agent.claude_permission_mode = Some(ClaudePermissionMode::BypassPermissions);
    cfg.agent.git_revert_mode = Some(GitRevertMode::Ask);
    cfg.agent.git_commit_push_enabled = Some(true);
    cfg.agent.repoprompt_plan_required = Some(false);
    cfg.agent.repoprompt_tool_injection = Some(false);
    cfg.agent.opencode_bin = Some(opencode_bin.display().to_string());
    cfg.agent.ci_gate_enabled = Some(false);
    cfg.queue = QueueConfig {
        file: Some(PathBuf::from(".ralph/queue.json")),
        done_file: Some(PathBuf::from(".ralph/done.json")),
        id_prefix: Some("RQ".to_string()),
        id_width: Some(4),
        size_warning_threshold_kb: Some(500),
        task_count_warning_threshold: Some(500),
        max_dependency_depth: Some(10),
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

fn resolved_for_completion(repo_root: PathBuf) -> crate::config::Resolved {
    crate::config::Resolved {
        config: Config::default(),
        repo_root: repo_root.clone(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
    }
}

fn write_queue_and_done(repo_root: &Path, status: TaskStatus) -> Result<()> {
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    let task = Task {
        id: "RQ-0001".to_string(),
        status,
        title: "Test task".to_string(),
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
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
    };

    queue::save_queue(
        &repo_root.join(".ralph/queue.json"),
        &QueueFile {
            version: 1,
            tasks: vec![],
        },
    )?;
    queue::save_queue(
        &repo_root.join(".ralph/done.json"),
        &QueueFile {
            version: 1,
            tasks: vec![task],
        },
    )?;
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["add", ".ralph/queue.json", ".ralph/done.json"])
        .status()?;
    anyhow::ensure!(status.success(), "git add failed");
    Ok(())
}

#[test]
fn phase1_continue_resumes_and_recovers_from_plan_only_violation() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::write(temp.path().join("baseline.txt"), "baseline")?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
dirty="{root}/dirty-file.txt"
if [ -f "$dirty" ]; then
  /bin/rm -f "$dirty"
else
  echo "dirty" > "$dirty"
fi
echo "plan content" > "$plan"
echo '{{"type":"text","part":{{"text":"ok"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
        root = temp.path().display()
    );
    let runner_path = create_fake_runner(temp.path(), "opencode", &script)?;

    let resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    let settings = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Custom("zai-coding-plan/glm-4.7".to_string()),
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let bins = runner::RunnerBinaries {
        codex: "codex",
        opencode: runner_path.to_str().expect("runner path"),
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };
    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };

    let calls = Arc::new(AtomicUsize::new(0));
    let prompt_handler: runutil::RevertPromptHandler = Arc::new({
        let calls = Arc::clone(&calls);
        move |_context: &runutil::RevertPromptContext| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                runutil::RevertDecision::Continue {
                    message: "continue".to_string(),
                }
            } else {
                runutil::RevertDecision::Keep
            }
        }
    });

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        revert_prompt: Some(prompt_handler),
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        allow_dirty_repo: true,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
    };

    let plan_text = execute_phase1_planning(&invocation, 2)?;
    assert_eq!(plan_text.trim(), "plan content");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();

    anyhow::ensure!(
        paths.len() == 1 && paths[0] == "baseline.txt",
        "expected baseline dirty path only, got: {:?}",
        paths
    );

    Ok(())
}

#[test]
fn phase1_proceed_allows_plan_only_violation() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
dirty="{root}/dirty-file.txt"
echo "dirty" > "$dirty"
echo "plan content" > "$plan"
echo '{{"type":"text","part":{{"text":"ok"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
        root = temp.path().display()
    );
    let runner_path = create_fake_runner(temp.path(), "opencode", &script)?;

    let resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    let settings = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Custom("zai-coding-plan/glm-4.7".to_string()),
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let bins = runner::RunnerBinaries {
        codex: "codex",
        opencode: runner_path.to_str().expect("runner path"),
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };
    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };

    let prompt_handler: runutil::RevertPromptHandler =
        Arc::new(|_context: &runutil::RevertPromptContext| runutil::RevertDecision::Proceed);

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        revert_prompt: Some(prompt_handler),
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        allow_dirty_repo: true,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
    };

    let plan_text = execute_phase1_planning(&invocation, 2)?;
    assert_eq!(plan_text.trim(), "plan content");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();

    anyhow::ensure!(
        paths.contains(&"dirty-file.txt".to_string()),
        "expected dirty-file.txt to remain, got: {:?}",
        paths
    );

    Ok(())
}

#[test]
fn phase1_rejects_changes_to_baseline_dirty_paths() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::write(temp.path().join("baseline.txt"), "baseline")?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
baseline="{root}/baseline.txt"
echo "changed" > "$baseline"
echo "plan content" > "$plan"
echo '{{"type":"text","part":{{"text":"ok"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
        root = temp.path().display()
    );
    let runner_path = create_fake_runner(temp.path(), "opencode", &script)?;

    let resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    let settings = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Custom("zai-coding-plan/glm-4.7".to_string()),
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let bins = runner::RunnerBinaries {
        codex: "codex",
        opencode: runner_path.to_str().expect("runner path"),
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };
    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        allow_dirty_repo: true,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
    };

    let err = execute_phase1_planning(&invocation, 2).expect_err("expected baseline violation");
    assert!(
        err.to_string().contains("baseline dirty path changed"),
        "unexpected error: {err}"
    );

    Ok(())
}

#[test]
fn ensure_phase3_completion_requires_clean_repo_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    assert!(ensure_phase3_completion(&resolved, "RQ-0001", true).is_err());
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_queue_files_for_rejected_status_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Rejected)?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(&resolved, "RQ-0001", true)?;
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_config_changes_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "queue and done"])
        .status()?;
    anyhow::ensure!(status.success(), "git commit failed");

    std::fs::write(temp.path().join(".ralph/config.json"), "{ \"version\": 1 }")?;
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["add", "-f", ".ralph/config.json"])
        .status()?;
    anyhow::ensure!(status.success(), "git add failed");
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "add config"])
        .status()?;
    anyhow::ensure!(status.success(), "git commit failed");

    std::fs::write(temp.path().join(".ralph/config.json"), "{ \"version\": 2 }")?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(&resolved, "RQ-0001", true)?;
    Ok(())
}

#[test]
fn ensure_phase3_completion_rejected_still_requires_clean_repo_for_other_changes() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Rejected)?;
    std::fs::write(temp.path().join("notes.txt"), "extra")?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    assert!(ensure_phase3_completion(&resolved, "RQ-0001", true).is_err());
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_dirty_repo_when_disabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(&resolved, "RQ-0001", false)?;
    Ok(())
}

#[test]
fn phase3_review_non_final_skips_completion_enforcement() -> Result<()> {
    let temp = TempDir::new()?;
    let script = r#"#!/bin/sh
echo '{"sessionID":"sess-123"}'
"#;
    let runner_path = create_fake_runner(temp.path(), "opencode", script)?;

    let resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    let settings = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Custom("zai-coding-plan/glm-4.7".to_string()),
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let bins = runner::RunnerBinaries {
        codex: "codex",
        opencode: runner_path.to_str().expect("runner path"),
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };
    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };

    let signal = completions::CompletionSignal {
        task_id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        notes: vec!["note".to_string()],
    };
    completions::write_completion_signal(temp.path(), &signal)?;

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        revert_prompt: None,
        iteration_context: "iteration",
        iteration_completion_block: "block",
        phase3_completion_guidance: "guidance",
        is_final_iteration: false,
        allow_dirty_repo: true,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
    };

    execute_phase3_review(&invocation)?;

    let signal_after = completions::read_completion_signal(temp.path(), "RQ-0001")?;
    assert!(signal_after.is_none());
    Ok(())
}

#[test]
fn phase3_review_non_final_runs_ci_gate_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    let script = r#"#!/bin/sh
echo '{"sessionID":"sess-123"}'
"#;
    let runner_path = create_fake_runner(temp.path(), "opencode", script)?;

    let mut resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    let ci_marker = temp.path().join("ci-gate-ran.txt");
    resolved.config.agent.ci_gate_enabled = Some(true);
    resolved.config.agent.ci_gate_command = Some(format!("echo ok > {}", ci_marker.display()));

    let settings = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Custom("zai-coding-plan/glm-4.7".to_string()),
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let bins = runner::RunnerBinaries {
        codex: "codex",
        opencode: runner_path.to_str().expect("runner path"),
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };
    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        revert_prompt: None,
        iteration_context: "iteration",
        iteration_completion_block: "block",
        phase3_completion_guidance: "guidance",
        is_final_iteration: false,
        allow_dirty_repo: true,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
    };

    execute_phase3_review(&invocation)?;

    assert!(ci_marker.exists(), "expected CI gate command to run");
    Ok(())
}

#[test]
fn ci_gate_auto_retries_twice_then_falls_back_to_prompt() -> Result<()> {
    let temp = TempDir::new()?;

    let script = format!(
        r#"#!/bin/sh
set -e
count="{root}/resume-count.txt"
n=0
if [ -f "$count" ]; then
  read n < "$count"
fi
n=$((n+1))
echo "$n" > "$count"
echo '{{"type":"text","part":{{"text":"resume"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
        root = temp.path().display()
    );
    let runner_path = create_fake_runner(temp.path(), "opencode", &script)?;

    let mut resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    resolved.config.agent.ci_gate_enabled = Some(true);
    resolved.config.agent.ci_gate_command = Some("exit 1".to_string());

    let settings = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Custom("zai-coding-plan/glm-4.7".to_string()),
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let bins = runner::RunnerBinaries {
        codex: "codex",
        opencode: runner_path.to_str().expect("runner path"),
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };
    let policy = promptflow::PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };

    let prompt_calls = Arc::new(AtomicUsize::new(0));
    let prompt_handler: runutil::RevertPromptHandler = Arc::new({
        let prompt_calls = Arc::clone(&prompt_calls);
        move |_context: &runutil::RevertPromptContext| {
            prompt_calls.fetch_add(1, Ordering::SeqCst);
            runutil::RevertDecision::Keep
        }
    });

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        base_prompt: "base",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        revert_prompt: Some(prompt_handler),
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: false,
        allow_dirty_repo: true,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
    };

    let continue_session = ContinueSession {
        runner: Runner::Opencode,
        model: settings.model.clone(),
        reasoning_effort: None,
        session_id: Some("sess-123".to_string()),
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
    };

    let err = run_ci_gate_with_continue(&invocation, continue_session, |_output| Ok(()))
        .expect_err("expected CI gate to fail and eventually fall back to Ask-mode handling");

    let count_path = temp.path().join("resume-count.txt");
    let count = std::fs::read_to_string(&count_path)?;
    assert_eq!(count.trim(), "2");

    assert_eq!(prompt_calls.load(Ordering::SeqCst), 1);

    assert!(err.to_string().contains("CI gate failed"));

    Ok(())
}
