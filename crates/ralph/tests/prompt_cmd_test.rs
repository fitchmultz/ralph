//! Purpose: thin integration-test hub for prompt command rendering coverage.
//!
//! Responsibilities:
//! - Re-export shared prompt preview types, config contracts, and tempdir helpers for companion modules.
//! - Provide shared resolved-config and queue fixture helpers used across prompt rendering tests.
//! - Delegate worker, scan, and task-builder rendering coverage to focused companion modules.
//!
//! Scope:
//! - Test-suite wiring and shared fixtures only; this root module contains no test functions.
//!
//! Usage:
//! - Companion modules use `use super::*;` to access shared imports, helper fixtures, and prompt option types.
//!
//! Invariants/Assumptions:
//! - Test names, assertions, prompt expectations, and helper behavior remain unchanged from the pre-split monolith.
//! - Production prompt rendering is exercised only through `ralph::commands::prompt` public entrypoints.

use anyhow::Result;
use ralph::cli::scan::ScanMode;
use ralph::commands::prompt::{
    self as prompt_cmd, ScanPromptOptions, TaskBuilderPromptOptions, WorkerMode,
    WorkerPromptOptions,
};
use ralph::contracts::{
    AgentConfig, CiGateConfig, Config, LoopConfig, ParallelConfig, PluginsConfig, ProjectType,
    QueueConfig,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn make_resolved(temp: &TempDir) -> ralph::config::Resolved {
    let repo_root = temp.path().to_path_buf();
    let queue_path = repo_root.join(".ralph/queue.jsonc");
    let done_path = repo_root.join(".ralph/done.jsonc");

    let cfg = Config {
        profiles: None,
        version: 2,
        project_type: Some(ProjectType::Code),
        queue: QueueConfig {
            file: Some(PathBuf::from(".ralph/queue.jsonc")),
            done_file: Some(PathBuf::from(".ralph/done.jsonc")),
            id_prefix: Some("RQ".to_string()),
            id_width: Some(4),
            size_warning_threshold_kb: Some(500),
            task_count_warning_threshold: Some(500),
            max_dependency_depth: Some(10),
            auto_archive_terminal_after_days: None,
            aging_thresholds: None,
        },
        agent: AgentConfig {
            phases: Some(3),
            repoprompt_plan_required: Some(false),
            repoprompt_tool_injection: Some(false),
            ci_gate: Some(CiGateConfig {
                enabled: Some(true),
                argv: Some(vec!["make".to_string(), "ci".to_string()]),
            }),
            git_publish_mode: Some(ralph::contracts::GitPublishMode::CommitAndPush),
            ..Default::default()
        },
        parallel: ParallelConfig::default(),
        loop_field: LoopConfig::default(),
        plugins: PluginsConfig::default(),
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
        ralph_dir.join("queue.jsonc"),
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

#[path = "prompt_cmd_test/scan.rs"]
mod scan;
#[path = "prompt_cmd_test/task_builder.rs"]
mod task_builder;
#[path = "prompt_cmd_test/worker.rs"]
mod worker;
