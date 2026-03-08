//! Tests for run phase orchestration helpers.

use super::phase2::cache_phase2_final_response;
use super::phase3::ensure_phase3_completion;
use super::shared::run_ci_gate_with_continue;
use super::{
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
    cfg.agent.git_commit_push_enabled = Some(true);
    cfg.agent.repoprompt_plan_required = Some(false);
    cfg.agent.repoprompt_tool_injection = Some(false);
    cfg.agent.opencode_bin = Some(opencode_bin.display().to_string());
    cfg.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(false),
        argv: None,
    });
    cfg.queue = QueueConfig {
        file: Some(PathBuf::from(".ralph/queue.json")),
        done_file: Some(PathBuf::from(".ralph/done.json")),
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

fn trust_repo(repo_root: &Path) -> Result<()> {
    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    std::fs::write(
        repo_root.join(".ralph/trust.jsonc"),
        "{\n  \"allow_project_commands\": true\n}\n",
    )?;
    Ok(())
}

#[test]
fn phase1_continue_resumes_and_recovers_from_plan_only_violation() -> Result<()> {
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

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
                Ok(runutil::RevertDecision::Continue {
                    message: "continue".to_string(),
                })
            } else {
                Ok(runutil::RevertDecision::Keep)
            }
        }
    });

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: Some(prompt_handler),
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
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
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

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
        Arc::new(|_context: &runutil::RevertPromptContext| Ok(runutil::RevertDecision::Proceed));

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: Some(prompt_handler),
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
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
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let err = execute_phase1_planning(&invocation, 2).expect_err("expected baseline violation");
    assert!(
        err.to_string().contains("baseline dirty path changed"),
        "unexpected error: {err}"
    );

    Ok(())
}

#[test]
fn phase1_allows_jsonc_queue_bookkeeping_changes() -> Result<()> {
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;

    let queue_jsonc = temp.path().join(".ralph/queue.jsonc");
    let done_jsonc = temp.path().join(".ralph/done.jsonc");
    std::fs::write(&queue_jsonc, "{ \"version\": 1, \"tasks\": [] }")?;
    std::fs::write(&done_jsonc, "{ \"version\": 1, \"tasks\": [] }")?;

    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["add", "-f", ".ralph/queue.jsonc", ".ralph/done.jsonc"])
        .status()?;
    anyhow::ensure!(status.success(), "git add failed");
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "add jsonc queue files"])
        .status()?;
    anyhow::ensure!(status.success(), "git commit failed");

    std::fs::write(&queue_jsonc, "{ \"version\": 2, \"tasks\": [] }")?;
    std::fs::write(&done_jsonc, "{ \"version\": 2, \"tasks\": [] }")?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: false,
        allow_dirty_repo: false,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let plan_text = execute_phase1_planning(&invocation, 2)?;
    assert_eq!(plan_text.trim(), "plan content");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();
    anyhow::ensure!(
        paths
            == vec![
                ".ralph/done.jsonc".to_string(),
                ".ralph/queue.jsonc".to_string()
            ],
        "expected jsonc queue bookkeeping paths only, got: {:?}",
        paths
    );

    Ok(())
}

#[test]
fn phase1_allows_arbitrary_ralph_file_changes() -> Result<()> {
    // Synchronize with tests that modify the interrupt flag.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::create_dir_all(temp.path().join(".ralph/state"))?;
    let ralph_state = temp.path().join(".ralph/state/worker.json");
    std::fs::write(&ralph_state, "{ \"v\": 1 }\n")?;

    let add_status = Command::new("git")
        .current_dir(temp.path())
        .args(["add", ".ralph/state/worker.json"])
        .status()?;
    anyhow::ensure!(
        add_status.success(),
        "git add .ralph/state/worker.json failed"
    );
    let commit_status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "add ralph state file"])
        .status()?;
    anyhow::ensure!(
        commit_status.success(),
        "git commit ralph state file failed"
    );

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
state="{root}/.ralph/state/worker.json"
echo '{{"v":2}}' > "$state"
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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: false,
        allow_dirty_repo: false,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let plan_text = execute_phase1_planning(&invocation, 2)?;
    assert_eq!(plan_text.trim(), "plan content");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();
    anyhow::ensure!(
        paths == vec![".ralph/state/worker.json".to_string()],
        "expected only dirty .ralph state path, got: {:?}",
        paths
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
fn ensure_phase3_completion_allows_config_jsonc_changes_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "queue and done"])
        .status()?;
    anyhow::ensure!(status.success(), "git commit failed");

    std::fs::write(
        temp.path().join(".ralph/config.jsonc"),
        "{ \"version\": 1 }",
    )?;
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["add", "-f", ".ralph/config.jsonc"])
        .status()?;
    anyhow::ensure!(status.success(), "git add failed");
    let status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "add config jsonc"])
        .status()?;
    anyhow::ensure!(status.success(), "git commit failed");

    std::fs::write(
        temp.path().join(".ralph/config.jsonc"),
        "{ \"version\": 2 }",
    )?;

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
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

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

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "iteration",
        iteration_completion_block: "block",
        phase3_completion_guidance: "guidance",
        is_final_iteration: false,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    // Non-final iterations should complete without requiring task finalization
    execute_phase3_review(&invocation)?;
    Ok(())
}

#[test]
fn phase3_review_non_final_runs_ci_gate_when_enabled() -> Result<()> {
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    let script = r#"#!/bin/sh
echo '{"sessionID":"sess-123"}'
"#;
    let runner_path = create_fake_runner(temp.path(), "opencode", script)?;

    let mut resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    let ci_marker = temp.path().join("ci-gate-ran.txt");
    trust_repo(temp.path())?;
    resolved.config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            format!(
                "from pathlib import Path; Path(r\"{}\").write_text(\"ok\")",
                ci_marker.display()
            ),
        ]),
    });

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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "iteration",
        iteration_completion_block: "block",
        phase3_completion_guidance: "guidance",
        is_final_iteration: false,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    execute_phase3_review(&invocation)?;

    assert!(ci_marker.exists(), "expected CI gate command to run");
    Ok(())
}

#[test]
fn ci_gate_auto_retries_to_limit_then_falls_back_to_prompt() -> Result<()> {
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
    trust_repo(temp.path())?;
    resolved.config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["false".to_string()]),
    });

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
            Ok(runutil::RevertDecision::Keep)
        }
    });

    let invocation = PhaseInvocation {
        resolved: &resolved,
        settings: &settings,
        bins,
        task_id: "RQ-0001",
        task_title: None,
        base_prompt: "base",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Ask,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: Some(prompt_handler),
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: false,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let continue_session = ContinueSession {
        runner: Runner::Opencode,
        model: settings.model.clone(),
        reasoning_effort: None,
        runner_cli: settings.runner_cli,
        phase_type: super::PhaseType::Implementation,
        session_id: Some("sess-123".to_string()),
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        ci_failure_retry_count: 0,
        task_id: "RQ-0001".to_string(),
        last_ci_error_pattern: None,
        consecutive_same_error_count: 0,
    };

    let err = run_ci_gate_with_continue(&invocation, continue_session, |_output, _elapsed| Ok(()))
        .expect_err("expected CI gate to fail and eventually fall back to Ask-mode handling");

    let count_path = temp.path().join("resume-count.txt");
    let count = std::fs::read_to_string(&count_path)?;
    assert_eq!(count.trim(), CI_GATE_AUTO_RETRY_LIMIT.to_string());

    assert_eq!(prompt_calls.load(Ordering::SeqCst), 1);

    assert!(err.to_string().contains("CI gate failed"));

    Ok(())
}

#[test]
fn phase1_followup_allows_preexisting_iteration_dirty_state() -> Result<()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::write(temp.path().join("impl.txt"), "prior iteration changes")?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
echo "plan content iteration 2" > "$plan"
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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: true,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let plan_text = execute_phase1_planning(&invocation, 3)?;
    assert_eq!(plan_text.trim(), "plan content iteration 2");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();

    anyhow::ensure!(
        paths == vec!["impl.txt".to_string()],
        "expected impl.txt to be dirty (plan cache is gitignored), got: {:?}",
        paths
    );

    Ok(())
}

#[test]
fn phase1_followup_allows_preexisting_dirty_queue_refresh() -> Result<()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::write(
        temp.path().join(".ralph/queue.json"),
        "{\n  \"version\": 1,\n  \"tasks\": []\n}\n",
    )?;
    let add_status = Command::new("git")
        .current_dir(temp.path())
        .args(["add", ".ralph/queue.json"])
        .status()?;
    anyhow::ensure!(add_status.success(), "git add .ralph/queue.json failed");
    let commit_status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "add queue baseline"])
        .status()?;
    anyhow::ensure!(commit_status.success(), "git commit queue baseline failed");

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
queue="{root}/.ralph/queue.json"
cat > "$queue" <<'EOF'
{{
  "version": 1,
  "tasks": [
    {{
      "id": "RQ-0001"
    }}
  ]
}}
EOF
echo "plan content iteration 2" > "$plan"
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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: true,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let plan_text = execute_phase1_planning(&invocation, 3)?;
    assert_eq!(plan_text.trim(), "plan content iteration 2");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();
    anyhow::ensure!(
        paths == vec![".ralph/queue.json".to_string()],
        "expected only dirty queue bookkeeping path, got: {:?}",
        paths
    );

    Ok(())
}

#[test]
fn phase1_followup_allows_preexisting_dirty_arbitrary_ralph_file() -> Result<()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::create_dir_all(temp.path().join(".ralph/state"))?;
    let ralph_state = temp.path().join(".ralph/state/worker.json");
    std::fs::write(&ralph_state, "{ \"v\": 1 }\n")?;
    let add_status = Command::new("git")
        .current_dir(temp.path())
        .args(["add", ".ralph/state/worker.json"])
        .status()?;
    anyhow::ensure!(
        add_status.success(),
        "git add .ralph/state/worker.json failed"
    );
    let commit_status = Command::new("git")
        .current_dir(temp.path())
        .args(["commit", "--quiet", "-m", "add ralph state baseline"])
        .status()?;
    anyhow::ensure!(
        commit_status.success(),
        "git commit ralph state baseline failed"
    );
    std::fs::write(temp.path().join("impl.txt"), "prior iteration changes")?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
state="{root}/.ralph/state/worker.json"
echo '{{"v":2}}' > "$state"
echo "plan content iteration 2" > "$plan"
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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: true,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let plan_text = execute_phase1_planning(&invocation, 3)?;
    assert_eq!(plan_text.trim(), "plan content iteration 2");

    let mut paths = git::status_paths(temp.path())?;
    paths.sort();
    anyhow::ensure!(
        paths
            == vec![
                ".ralph/state/worker.json".to_string(),
                "impl.txt".to_string()
            ],
        "expected dirty .ralph state path + preexisting impl baseline, got: {:?}",
        paths
    );

    Ok(())
}

#[test]
fn phase1_followup_rejects_new_disallowed_dirty_paths() -> Result<()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::write(temp.path().join("impl.txt"), "prior iteration changes")?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
disallowed="{root}/src/new_file.rs"
mkdir -p "$(dirname "$disallowed")"
echo "disallowed" > "$disallowed"
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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: true,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    let err =
        execute_phase1_planning(&invocation, 3).expect_err("expected follow-up phase1 violation");
    assert!(
        err.to_string().contains("Follow-up Phase 1 violation"),
        "expected follow-up violation message, got: {err}"
    );

    Ok(())
}

#[test]
fn phase2_final_three_phase_iteration_skips_duplicate_ci_gate() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/plans"))?;
    std::fs::create_dir_all(temp.path().join(".ralph/cache/phase2_final"))?;

    let script = r#"#!/bin/sh
set -e
echo '{"type":"text","part":{"text":"phase2 complete"}}'
echo '{"sessionID":"sess-phase2"}'
"#;
    let runner_path = create_fake_runner(temp.path(), "opencode", script)?;

    let ci_marker = temp.path().join("ci-gate-ran.txt");
    let mut resolved = resolved_for_repo(temp.path().to_path_buf(), &runner_path);
    trust_repo(temp.path())?;
    resolved.config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            format!(
                "from pathlib import Path; Path(r\"{}\").write_text(\"ci\")",
                ci_marker.display()
            ),
        ]),
    });

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
        task_title: None,
        base_prompt: "base prompt",
        policy: &policy,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        project_type: crate::contracts::ProjectType::Code,
        git_revert_mode: GitRevertMode::Disabled,
        git_commit_push_enabled: true,
        push_policy: crate::commands::run::supervision::PushPolicy::RequireUpstream,
        revert_prompt: None,
        iteration_context: "",
        iteration_completion_block: "",
        phase3_completion_guidance: "",
        is_final_iteration: true,
        is_followup_iteration: false,
        allow_dirty_repo: true,
        post_run_mode: PostRunMode::Normal,
        parallel_target_branch: None,
        notify_on_complete: None,
        notify_sound: None,
        lfs_check: false,
        no_progress: false,
        execution_timings: None,
        plugins: None,
    };

    execute_phase2_implementation(&invocation, 3, "plan text")?;

    assert!(
        !ci_marker.exists(),
        "final three-phase phase2 should skip CI gate; phase3/post-run handles CI"
    );

    Ok(())
}
