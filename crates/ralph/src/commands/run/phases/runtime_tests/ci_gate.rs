//! CI retry and final-iteration scenario coverage.

use super::*;
use crate::commands::run::PhaseType;

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
        queue_lock: None,
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
        git_publish_mode: crate::contracts::GitPublishMode::CommitAndPush,
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
        phase_type: PhaseType::Implementation,
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

    execute_phase2_implementation(&invocation, 3, "plan text")?;

    assert!(
        !ci_marker.exists(),
        "final three-phase phase2 should skip CI gate; phase3/post-run handles CI"
    );

    Ok(())
}
