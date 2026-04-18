//! Phase 1 plan-only and dirty-path guardrail tests.

use super::*;

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
        git_revert_mode: GitRevertMode::Ask,
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
        git_revert_mode: GitRevertMode::Ask,
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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

    git_status_ok(
        temp.path(),
        &["add", "-f", ".ralph/queue.jsonc", ".ralph/done.jsonc"],
        "git add queue bookkeeping failed",
    )?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "add jsonc queue files"],
        "git commit queue bookkeeping failed",
    )?;

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
        git_revert_mode: GitRevertMode::Ask,
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
