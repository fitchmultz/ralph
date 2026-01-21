use anyhow::Result;
use ralph::contracts::{AgentConfig, Config, ProjectType, QueueConfig};
use ralph::prompt_cmd::{
    self, ScanPromptOptions, TaskBuilderPromptOptions, WorkerMode, WorkerPromptOptions,
};
use ralph::promptflow;
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
            require_repoprompt: Some(false),
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
fn worker_phase1_includes_markers_and_optional_rp() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: None,
            mode: WorkerMode::Phase1,
            repoprompt_required: true,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("PLANNING MODE - PHASE 1 OF 2"));
    assert!(prompt.contains(promptflow::RALPH_PHASE1_PLAN_BEGIN));
    assert!(prompt.contains(promptflow::RALPH_PHASE1_PLAN_END));
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
            repoprompt_required: false,
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
            repoprompt_required: true,
            plan_file: None,
            plan_text: Some("PLAN BODY".to_string()),
            explain: false,
        },
    )?;

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 2"));
    assert!(prompt.contains("PLAN BODY"));
    assert!(prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
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
            repoprompt_required: false,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 2"));
    assert!(prompt.contains("*No plan file found*"));
    assert!(prompt.contains("No plan file was found at"));
    assert!(prompt.contains("Please proceed with implementation based on the task requirements"));
    assert!(prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
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
            repoprompt_required: true,
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
            repoprompt_required: false,
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
            repoprompt_required: false,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("CODE REVIEW MODE - PHASE 3 OF 3"));
    assert!(prompt.contains("CODING STANDARDS"));
    assert!(prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    assert!(prompt.contains("PRE-FLIGHT OVERRIDE"));
    Ok(())
}
