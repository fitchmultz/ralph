//! Prompt command integration tests (prompt preview behaviors and wiring).

use anyhow::Result;
use ralph::commands::prompt::{
    self as prompt_cmd, ScanPromptOptions, TaskBuilderPromptOptions, WorkerMode,
    WorkerPromptOptions,
};
use ralph::contracts::{AgentConfig, Config, ProjectType, QueueConfig};
use std::path::PathBuf;
use tempfile::TempDir;

fn make_resolved(temp: &TempDir) -> ralph::config::Resolved {
    let repo_root = temp.path().to_path_buf();
    let queue_path = repo_root.join(".ralph/queue.json");
    let done_path = repo_root.join(".ralph/done.json");

    let cfg = Config {
        version: 1,
        project_type: Some(ProjectType::Code),
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.json")),
            done_file: Some(PathBuf::from(".ralph/done.json")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
        },
        agent: AgentConfig {
            phases: Some(3),
            require_repoprompt: None,
            repoprompt_plan_required: Some(false),
            repoprompt_tool_injection: Some(false),
            ci_gate_command: Some("make ci".to_string()),
            ci_gate_enabled: Some(true),
            git_commit_push_enabled: Some(true),
            ..Default::default()
        },
    };

    ralph::config::Resolved {
        config: cfg,
        repo_root,
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

fn write_minimal_queue(temp: &TempDir) -> Result<()> {
    let ralph_dir = temp.path().join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    std::fs::write(
        ralph_dir.join("queue.json"),
        r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test",
      "tags": ["t"],
      "scope": ["s"],
      "evidence": ["e"],
      "plan": ["p"],
      "request": "r",
      "created_at": "2026-01-19T00:00:00Z",
      "updated_at": "2026-01-19T00:00:00Z"
    }
  ]
}"#,
    )?;
    Ok(())
}

#[test]
fn worker_phase1_includes_plan_cache_path_and_optional_rp() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: None,
            mode: WorkerMode::Phase1,
            repoprompt_plan_required: true,
            repoprompt_tool_injection: true,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("PLANNING MODE - PHASE 1 OF 3"));
    assert!(prompt.contains(".ralph/cache/plans/RQ-0001.md"));
    assert!(prompt.contains(ralph::prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    Ok(())
}

#[test]
fn worker_single_phase_includes_completion_workflow() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: None,
            mode: WorkerMode::Single,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("single-pass execution mode"));
    assert_eq!(
        prompt
            .match_indices("IMPLEMENTATION COMPLETION CHECKLIST")
            .count(),
        1
    );

    // Regression guard: in supervised runs, Ralph marks tasks as `doing` by modifying
    // .ralph/queue.json, which makes the repo appear dirty. The worker prompt must
    // explicitly allow this bookkeeping-only dirtiness to avoid unnecessary stops.
    assert!(prompt.contains("IMPORTANT EXCEPTION (RALPH BOOKKEEPING)"));
    assert!(prompt.contains(".ralph/queue.json"));
    assert!(prompt.contains(".ralph/done.json"));
    Ok(())
}

#[test]
fn worker_phase2_requires_plan_text() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: true,
            repoprompt_tool_injection: true,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: Some("PLAN BODY".to_string()),
            explain: false,
        },
    )?;

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 3"));
    assert!(prompt.contains("PLAN BODY"));
    assert!(prompt.contains("PHASE 2 HANDOFF CHECKLIST"));
    Ok(())
}

#[test]
fn worker_phase2_uses_placeholder_when_no_plan_found() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 3"));
    assert!(prompt.contains("*No plan file found*"));
    assert!(prompt.contains("No plan file was found at"));
    assert!(prompt.contains("Please proceed with implementation based on the task requirements"));
    assert!(prompt.contains("PHASE 2 HANDOFF CHECKLIST"));
    Ok(())
}

#[test]
fn scan_prompt_replaces_focus_and_can_wrap_rp() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_scan_prompt(
        &resolved,
        ScanPromptOptions {
            focus: "CI gaps".to_string(),
            repoprompt_tool_injection: true,
            explain: false,
        },
    )?;

    assert!(prompt.contains("CI gaps"));
    // The wrap_with_repoprompt_requirement function adds the instruction
    assert!(prompt.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(prompt.contains("You MUST use the available RepoPrompt tools"));
    Ok(())
}

#[test]
fn task_builder_prompt_includes_request_and_hints() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_task_builder_prompt(
        &resolved,
        TaskBuilderPromptOptions {
            request: "Add tests".to_string(),
            hint_tags: "rust,tests".to_string(),
            hint_scope: "crates/ralph".to_string(),
            repoprompt_tool_injection: false,
            explain: false,
        },
    )?;

    assert!(prompt.contains("Add tests"));
    assert!(prompt.contains("rust,tests"));
    assert!(prompt.contains("crates/ralph"));
    Ok(())
}

#[test]
fn worker_phase3_includes_code_review_prompt() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase3,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("CODE REVIEW MODE - PHASE 3 OF 3"));
    assert!(prompt.contains("CODING STANDARDS"));
    assert!(prompt.contains("PRE-FLIGHT OVERRIDE"));
    Ok(())
}

#[test]
fn worker_phase2_includes_iteration_context_for_followup() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 3,
            iteration_index: 2,
            plan_file: None,
            plan_text: Some("PLAN BODY".to_string()),
            explain: false,
        },
    )?;

    assert!(prompt.contains("REFINEMENT CONTEXT"));
    assert!(prompt.contains("ITERATION COMPLETION RULES"));
    Ok(())
}
