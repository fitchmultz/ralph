//! Phase 1 follow-up dirty-state tests.
//!
//! Purpose:
//! - Phase 1 follow-up dirty-state tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

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
        queue_lock: None,
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
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
        temp.path().join(".ralph/queue.jsonc"),
        "{\n  \"version\": 1,\n  \"tasks\": []\n}\n",
    )?;
    git_status_ok(
        temp.path(),
        &["add", "-f", ".ralph/queue.jsonc"],
        "git add .ralph/queue.jsonc failed",
    )?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "add queue baseline"],
        "git commit queue baseline failed",
    )?;

    let script = format!(
        r#"#!/bin/sh
set -e
plan="{root}/.ralph/cache/plans/RQ-0001.md"
queue="{root}/.ralph/queue.jsonc"
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
        queue_lock: None,
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
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
        paths == vec![".ralph/queue.jsonc".to_string()],
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
    git_status_ok(
        temp.path(),
        &["add", "-f", ".ralph/state/worker.json"],
        "git add .ralph/state/worker.json failed",
    )?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "add ralph state baseline"],
        "git commit ralph state baseline failed",
    )?;
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
        queue_lock: None,
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
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
        queue_lock: None,
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
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
