//! Phase 3 completion scenario coverage.

use super::*;

#[test]
fn ensure_phase3_completion_requires_clean_repo_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    assert!(
        ensure_phase3_completion(
            &resolved,
            "RQ-0001",
            crate::contracts::GitPublishMode::CommitAndPush
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_queue_files_for_rejected_status_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Rejected)?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(
        &resolved,
        "RQ-0001",
        crate::contracts::GitPublishMode::CommitAndPush,
    )?;
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_config_changes_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "queue and done"],
        "git commit queue and done failed",
    )?;

    std::fs::write(
        temp.path().join(".ralph/config.jsonc"),
        "{ \"version\": 1 }",
    )?;
    git_status_ok(
        temp.path(),
        &["add", "-f", ".ralph/config.jsonc"],
        "git add config.jsonc failed",
    )?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "add config"],
        "git commit config.jsonc failed",
    )?;

    std::fs::write(
        temp.path().join(".ralph/config.jsonc"),
        "{ \"version\": 2 }",
    )?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(
        &resolved,
        "RQ-0001",
        crate::contracts::GitPublishMode::CommitAndPush,
    )?;
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_config_jsonc_changes_when_enabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "queue and done"],
        "git commit queue and done failed",
    )?;

    std::fs::write(
        temp.path().join(".ralph/config.jsonc"),
        "{ \"version\": 1 }",
    )?;
    git_status_ok(
        temp.path(),
        &["add", "-f", ".ralph/config.jsonc"],
        "git add config.jsonc failed",
    )?;
    git_status_ok(
        temp.path(),
        &["commit", "--quiet", "-m", "add config jsonc"],
        "git commit config jsonc failed",
    )?;

    std::fs::write(
        temp.path().join(".ralph/config.jsonc"),
        "{ \"version\": 2 }",
    )?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(
        &resolved,
        "RQ-0001",
        crate::contracts::GitPublishMode::CommitAndPush,
    )?;
    Ok(())
}

#[test]
fn ensure_phase3_completion_rejected_still_requires_clean_repo_for_other_changes() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Rejected)?;
    std::fs::write(temp.path().join("notes.txt"), "extra")?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    assert!(
        ensure_phase3_completion(
            &resolved,
            "RQ-0001",
            crate::contracts::GitPublishMode::CommitAndPush
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn ensure_phase3_completion_allows_dirty_repo_when_disabled() -> Result<()> {
    let temp = TempDir::new()?;
    git_init(temp.path())?;
    write_queue_and_done(temp.path(), TaskStatus::Done)?;

    let resolved = resolved_for_completion(temp.path().to_path_buf());
    ensure_phase3_completion(&resolved, "RQ-0001", crate::contracts::GitPublishMode::Off)?;
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
