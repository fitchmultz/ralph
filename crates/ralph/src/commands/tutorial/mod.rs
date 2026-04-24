//! Interactive tutorial command implementation.
//!
//! Purpose:
//! - Interactive tutorial command implementation.
//!
//! Responsibilities:
//! - Orchestrate tutorial phases in sequence.
//! - Provide options struct for tutorial configuration.
//! - Re-export types for CLI handler.
//!
//! Not handled here:
//! - CLI argument parsing (see cli/tutorial.rs).
//! - Phase implementations (see phases.rs).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

mod phases;
mod prompter;
mod sandbox;

pub use prompter::{
    DialoguerTutorialPrompter, ScriptedResponse, ScriptedTutorialPrompter, TutorialPrompter,
};
pub use sandbox::TutorialSandbox;

use anyhow::Result;

/// Tutorial configuration options.
pub struct TutorialOptions {
    /// Run in interactive mode (require TTY).
    pub interactive: bool,
    /// Keep sandbox after completion.
    pub keep_sandbox: bool,
}

/// Run the interactive tutorial.
pub fn run_tutorial(opts: TutorialOptions) -> Result<()> {
    if opts.interactive {
        run_tutorial_interactive(opts.keep_sandbox)
    } else {
        run_tutorial_non_interactive(opts.keep_sandbox)
    }
}

/// Run tutorial in interactive mode with Dialoguer.
fn run_tutorial_interactive(keep_sandbox: bool) -> Result<()> {
    let prompter = DialoguerTutorialPrompter;

    phases::phase_welcome(&prompter)?;
    let sandbox = phases::phase_setup(&prompter)?;
    phases::phase_init(&prompter, &sandbox)?;
    let task_id = phases::phase_create_task(&prompter, &sandbox)?;
    phases::phase_dry_run(&prompter, &sandbox, &task_id)?;
    phases::phase_review(&prompter, &sandbox)?;
    phases::phase_cleanup(&prompter, sandbox, keep_sandbox)?;

    Ok(())
}

/// Run tutorial with a custom prompter (for testing).
pub fn run_tutorial_with_prompter(
    prompter: &dyn TutorialPrompter,
    keep_sandbox: bool,
) -> Result<()> {
    phases::phase_welcome(prompter)?;
    let sandbox = phases::phase_setup(prompter)?;
    phases::phase_init(prompter, &sandbox)?;
    let task_id = phases::phase_create_task(prompter, &sandbox)?;
    phases::phase_dry_run(prompter, &sandbox, &task_id)?;
    phases::phase_review(prompter, &sandbox)?;
    phases::phase_cleanup(prompter, sandbox, keep_sandbox)?;

    Ok(())
}

/// Run tutorial non-interactively (minimal output, no prompts).
fn run_tutorial_non_interactive(keep_sandbox: bool) -> Result<()> {
    log::info!("Running tutorial in non-interactive mode");

    let sandbox = sandbox::TutorialSandbox::create()?;
    log::info!("Created sandbox at: {}", sandbox.path.display());

    // Run init
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(&sandbox.path)?;

    let resolved = crate::config::resolve_from_cwd()?;
    crate::commands::init::run_init(
        &resolved,
        crate::commands::init::InitOptions {
            force: false,
            force_lock: false,
            interactive: false,
            update_readme: false,
        },
    )?;

    // Add task
    let task_id = "RQ-0001";
    let task = crate::contracts::Task {
        id: task_id.to_string(),
        title: "Tutorial task".to_string(),
        description: Some("A sample tutorial task".to_string()),
        status: crate::contracts::TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec!["tutorial".to_string()],
        scope: vec!["src/lib.rs".to_string()],
        plan: vec!["Add a farewell function".to_string()],
        request: Some("Tutorial task".to_string()),
        created_at: Some(crate::timeutil::now_utc_rfc3339_or_fallback()),
        updated_at: Some(crate::timeutil::now_utc_rfc3339_or_fallback()),
        ..Default::default()
    };
    let queue = crate::contracts::QueueFile {
        version: 1,
        tasks: vec![task],
    };
    crate::queue::save_queue(&sandbox.path.join(".ralph/queue.jsonc"), &queue)?;

    std::env::set_current_dir(&original_dir)?;

    log::info!("Tutorial completed successfully");

    if keep_sandbox {
        let path = sandbox.preserve();
        log::info!("Sandbox preserved at: {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_options_creation() {
        let opts = TutorialOptions {
            interactive: true,
            keep_sandbox: false,
        };
        assert!(opts.interactive);
        assert!(!opts.keep_sandbox);
    }
}
