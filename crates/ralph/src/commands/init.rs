//! Initialization workflow for creating `.ralph` state and starter files.
//!
//! Responsibilities:
//! - Orchestrate initialization via `run_init()`.
//! - Provide public types for initialization options and results.
//! - Re-export submodule functionality for CLI layer.
//! - Update `.gitignore` to include `.ralph/workspaces/` for parallel mode hygiene.
//!
//! Submodules:
//! - `readme`: README version management and updates.
//! - `wizard`: Interactive onboarding wizard UI.
//! - `writers`: File creation for queue, done, and config.
//! - `gitignore`: Gitignore update for Ralph workspace directories.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli::init`).
//! - TTY detection (handled by CLI layer).
//!
//! Invariants/assumptions:
//! - Wizard answers are validated before file creation.
//! - Non-interactive mode produces identical output to pre-wizard behavior.
//! - Gitignore updates are idempotent (safe to run multiple times).

use crate::config;
use crate::queue;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

pub mod gitignore;
pub mod readme;
pub mod wizard;
pub mod writers;

// Re-export public items from submodules
pub use readme::{
    ReadmeCheckResult, ReadmeVersionError, check_readme_current, check_readme_current_from_root,
    extract_readme_version,
};

// Re-export README_VERSION from constants for backward compatibility
pub use crate::constants::versions::README_VERSION;
pub use wizard::{WizardAnswers, print_completion_message, run_wizard};
pub use writers::{write_config, write_done, write_queue};

/// Options for initializing Ralph files.
pub struct InitOptions {
    /// Overwrite existing files if they already exist.
    pub force: bool,
    /// Force remove stale locks.
    pub force_lock: bool,
    /// Run interactive onboarding wizard.
    pub interactive: bool,
    /// Update README if it exists (force overwrite with latest template).
    pub update_readme: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInitStatus {
    Created,
    Valid,
    Updated,
}

#[derive(Debug)]
pub struct InitReport {
    pub queue_status: FileInitStatus,
    pub done_status: FileInitStatus,
    pub config_status: FileInitStatus,
    /// (status, version) tuple - version is Some if README was read/created
    pub readme_status: Option<(FileInitStatus, Option<u32>)>,
    /// Paths that were actually used for file creation (may differ from resolved paths)
    pub queue_path: std::path::PathBuf,
    pub done_path: std::path::PathBuf,
    pub config_path: std::path::PathBuf,
}

pub fn run_init(resolved: &config::Resolved, opts: InitOptions) -> Result<InitReport> {
    let ralph_dir = resolved.repo_root.join(".ralph");
    fs::create_dir_all(&ralph_dir).with_context(|| format!("create {}", ralph_dir.display()))?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "init", opts.force_lock)?;

    // Run wizard if interactive mode is enabled
    let wizard_answers = if opts.interactive {
        Some(wizard::run_wizard()?)
    } else {
        None
    };

    // For new projects, always use .jsonc extensions (don't fall back to .json)
    let queue_path = resolved
        .repo_root
        .join(crate::constants::queue::DEFAULT_QUEUE_FILE);
    let done_path = resolved
        .repo_root
        .join(crate::constants::queue::DEFAULT_DONE_FILE);
    let config_path = resolved
        .repo_root
        .join(crate::constants::queue::DEFAULT_CONFIG_FILE);

    let queue_status = writers::write_queue(
        &queue_path,
        opts.force,
        &resolved.id_prefix,
        resolved.id_width,
        wizard_answers.as_ref(),
    )?;
    let done_status = writers::write_done(
        &done_path,
        opts.force,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    let config_status = writers::write_config(&config_path, opts.force, wizard_answers.as_ref())?;

    let mut readme_status = None;
    if crate::prompts::prompts_reference_readme(&resolved.repo_root)? {
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        let (status, version) = readme::write_readme(&readme_path, opts.force, opts.update_readme)?;
        readme_status = Some((status, version));
    }

    // Update .gitignore to include .ralph/workspaces/ for parallel mode hygiene
    // This is idempotent - safe to run multiple times
    if let Err(e) = gitignore::ensure_ralph_gitignore_entries(&resolved.repo_root) {
        log::warn!(
            "Failed to update .gitignore: {}. You may need to manually add '.ralph/workspaces/' to your .gitignore.",
            e
        );
    }

    // Check for pending migrations and warn if any exist
    check_pending_migrations(resolved)?;

    // Print completion message for interactive mode
    if opts.interactive {
        wizard::print_completion_message(wizard_answers.as_ref(), &resolved.queue_path);
    }

    Ok(InitReport {
        queue_status,
        done_status,
        config_status,
        readme_status,
        queue_path,
        done_path,
        config_path,
    })
}

/// Check for pending migrations and display a warning if any exist.
fn check_pending_migrations(resolved: &config::Resolved) -> anyhow::Result<()> {
    use crate::migration::{self, MigrationCheckResult};

    let ctx = match migration::MigrationContext::from_resolved(resolved) {
        Ok(ctx) => ctx,
        Err(e) => {
            log::debug!("Could not create migration context: {}", e);
            return Ok(());
        }
    };

    match migration::check_migrations(&ctx)? {
        MigrationCheckResult::Current => {
            // No migrations pending, nothing to do
        }
        MigrationCheckResult::Pending(migrations) => {
            eprintln!();
            eprintln!(
                "{}",
                format!("⚠ Warning: {} migration(s) pending", migrations.len()).yellow()
            );
            eprintln!("Run {} to apply them.", "ralph migrate --apply".cyan());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, ProjectType};
    use tempfile::TempDir;

    fn resolved_for(dir: &TempDir) -> config::Resolved {
        let repo_root = dir.path().to_path_buf();
        let queue_path = repo_root.join(".ralph/queue.jsonc");
        let done_path = repo_root.join(".ralph/done.jsonc");
        let project_config_path = Some(repo_root.join(".ralph/config.jsonc"));
        config::Resolved {
            config: Config::default(),
            repo_root,
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path,
        }
    }

    #[test]
    fn init_creates_missing_files() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Created);
        assert_eq!(report.done_status, FileInitStatus::Created);
        assert_eq!(report.config_status, FileInitStatus::Created);
        assert!(matches!(
            report.readme_status,
            Some((FileInitStatus::Created, Some(6)))
        ));
        let queue = crate::queue::load_queue(&resolved.queue_path)?;
        assert_eq!(queue.version, 1);
        let done = crate::queue::load_queue(&resolved.done_path)?;
        assert_eq!(done.version, 1);
        let raw_cfg = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&raw_cfg)?;
        assert_eq!(cfg.version, 1);
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        assert!(readme_path.exists());
        let readme_raw = std::fs::read_to_string(readme_path)?;
        assert!(readme_raw.contains("# Ralph runtime files"));
        Ok(())
    }

    #[test]
    fn init_generates_readme_with_correct_archive_command() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        let readme_raw = std::fs::read_to_string(readme_path)?;
        // Verify the correct command is present
        assert!(
            readme_raw.contains("ralph queue archive"),
            "README should contain 'ralph queue archive' command"
        );
        // Verify the stale command is NOT present (regression check)
        assert!(
            !readme_raw.contains("ralph queue done"),
            "README should NOT contain stale 'ralph queue done' command"
        );
        Ok(())
    }

    #[test]
    fn init_skips_existing_when_not_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        let queue_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Keep",
      "tags": ["code"],
      "scope": ["x"],
      "evidence": ["y"],
      "plan": ["z"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.queue_path, queue_json)?;
        let done_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0002",
      "status": "done",
      "title": "Done",
      "tags": ["code"],
      "scope": ["x"],
      "evidence": ["y"],
      "plan": ["z"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z",
      "completed_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.done_path, done_json)?;
        let config_json = r#"{
  "version": 1,
  "queue": {
    "file": ".ralph/queue.json"
  }
}"#;
        std::fs::write(resolved.project_config_path.as_ref().unwrap(), config_json)?;
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Valid);
        assert_eq!(report.done_status, FileInitStatus::Valid);
        assert_eq!(report.config_status, FileInitStatus::Valid);
        assert!(matches!(
            report.readme_status,
            Some((FileInitStatus::Created, Some(6)))
        ));
        let raw = std::fs::read_to_string(&resolved.queue_path)?;
        assert!(raw.contains("Keep"));
        let done_raw = std::fs::read_to_string(&resolved.done_path)?;
        assert!(done_raw.contains("Done"));
        Ok(())
    }

    #[test]
    fn init_overwrites_when_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"docs"}"#,
        )?;
        let report = run_init(
            &resolved,
            InitOptions {
                force: true,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Created);
        assert_eq!(report.done_status, FileInitStatus::Created);
        assert_eq!(report.config_status, FileInitStatus::Created);
        assert!(matches!(
            report.readme_status,
            Some((FileInitStatus::Created, Some(6)))
        ));
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&cfg_raw)?;
        assert_eq!(cfg.project_type, Some(ProjectType::Code));
        assert_eq!(
            cfg.queue.file,
            Some(std::path::PathBuf::from(".ralph/queue.jsonc"))
        );
        assert_eq!(
            cfg.queue.done_file,
            Some(std::path::PathBuf::from(".ralph/done.jsonc"))
        );
        assert_eq!(cfg.queue.id_prefix, Some("RQ".to_string()));
        assert_eq!(cfg.queue.id_width, Some(4));
        assert_eq!(cfg.agent.runner, Some(crate::contracts::Runner::Claude));
        assert_eq!(
            cfg.agent.model,
            Some(crate::contracts::Model::Custom("sonnet".to_string()))
        );
        assert_eq!(
            cfg.agent.reasoning_effort,
            Some(crate::contracts::ReasoningEffort::Medium)
        );
        assert_eq!(cfg.agent.iterations, Some(1));
        assert_eq!(cfg.agent.followup_reasoning_effort, None);
        assert_eq!(cfg.agent.gemini_bin, Some("gemini".to_string()));
        Ok(())
    }

    #[test]
    fn init_creates_json_for_new_install() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Created);
        assert_eq!(report.done_status, FileInitStatus::Created);
        assert_eq!(report.config_status, FileInitStatus::Created);

        // Verify JSON files were created
        let queue_raw = std::fs::read_to_string(&resolved.queue_path)?;
        assert!(queue_raw.contains("{"));
        let done_raw = std::fs::read_to_string(&resolved.done_path)?;
        assert!(done_raw.contains("{"));
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        assert!(cfg_raw.contains("{"));
        Ok(())
    }

    #[test]
    fn init_skips_readme_when_not_referenced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        // Override all prompts to ensure none reference the README.
        let overrides = resolved.repo_root.join(".ralph/prompts");
        fs::create_dir_all(&overrides)?;
        let prompt_files = [
            "worker.md",
            "worker_phase1.md",
            "worker_phase2.md",
            "worker_phase2_handoff.md",
            "worker_phase3.md",
            "worker_single_phase.md",
            "task_builder.md",
            "task_updater.md",
            "scan.md",
            "completion_checklist.md",
            "code_review.md",
            "phase2_handoff_checklist.md",
            "iteration_checklist.md",
        ];
        for file in prompt_files {
            fs::write(overrides.join(file), "no reference")?;
        }

        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;
        assert_eq!(report.readme_status, None);
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        assert!(!readme_path.exists());
        Ok(())
    }

    #[test]
    fn init_fails_on_invalid_existing_queue() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;

        // Create a queue with an invalid ID prefix (WRONG-0001 vs RQ)
        let queue_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "WRONG-0001",
      "status": "todo",
      "title": "Bad ID",
      "tags": [],
      "scope": [],
      "evidence": [],
      "plan": [],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.queue_path, queue_json)?;
        std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"code"}"#,
        )?;

        let result = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("validate existing queue"));
        Ok(())
    }

    #[test]
    fn init_fails_on_invalid_existing_done() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;

        std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;

        // Create a done file with a task that has invalid ID prefix
        let done_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "WRONG-0002",
      "status": "done",
      "title": "Bad ID",
      "tags": [],
      "scope": [],
      "evidence": [],
      "plan": [],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.done_path, done_json)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"code"}"#,
        )?;

        let result = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("validate existing done"));
        Ok(())
    }

    #[test]
    fn init_with_wizard_answers_creates_configured_files() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        let wizard_answers = WizardAnswers {
            runner: crate::contracts::Runner::Codex,
            model: "gpt-5.3-codex".to_string(),
            phases: 2,
            create_first_task: true,
            first_task_title: Some("Test task".to_string()),
            first_task_description: Some("Test description".to_string()),
            first_task_priority: crate::contracts::TaskPriority::High,
        };

        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: false,
            },
        )?;

        // Manually write the queue with wizard answers to test the write_queue function
        writers::write_queue(
            &resolved.queue_path,
            true,
            &resolved.id_prefix,
            resolved.id_width,
            Some(&wizard_answers),
        )?;

        writers::write_config(
            resolved.project_config_path.as_ref().unwrap(),
            true,
            Some(&wizard_answers),
        )?;

        assert_eq!(report.done_status, FileInitStatus::Created);

        // Verify config has correct runner and phases
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&cfg_raw)?;
        assert_eq!(cfg.agent.runner, Some(crate::contracts::Runner::Codex));
        assert_eq!(cfg.agent.phases, Some(2));

        // Verify queue has first task
        let queue = crate::queue::load_queue(&resolved.queue_path)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].title, "Test task");
        assert_eq!(
            queue.tasks[0].priority,
            crate::contracts::TaskPriority::High
        );

        Ok(())
    }

    #[test]
    fn init_update_readme_flag_updates_outdated() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        // Create an existing README with old version
        fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        let old_readme = "<!-- RALPH_README_VERSION: 1 -->\n# Old content";
        fs::write(resolved.repo_root.join(".ralph/README.md"), old_readme)?;
        fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
        fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
        fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1}"#,
        )?;

        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
                interactive: false,
                update_readme: true,
            },
        )?;

        // README should be updated
        assert!(matches!(
            report.readme_status,
            Some((FileInitStatus::Updated, Some(6)))
        ));

        // Content should be new
        let content = std::fs::read_to_string(resolved.repo_root.join(".ralph/README.md"))?;
        assert!(!content.contains("Old content"));
        assert!(content.contains("Ralph runtime files"));
        Ok(())
    }
}
