//! Phase 1 `.ralph` dirty-path allowance tests.

use super::*;

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
        .args(["add", "-f", ".ralph/state/worker.json"])
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
        paths == vec![".ralph/state/worker.json".to_string()],
        "expected only dirty .ralph state path, got: {:?}",
        paths
    );

    Ok(())
}
