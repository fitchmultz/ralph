//! Tutorial phase implementations.
//!
//! Purpose:
//! - Tutorial phase implementations.
//!
//! Responsibilities:
//! - Implement each tutorial phase with prompts and actions.
//! - Call actual CLI commands programmatically.
//! - Display explanations and gather user input.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::prompter::TutorialPrompter;
use super::sandbox::TutorialSandbox;
use anyhow::{Context, Result};
use colored::Colorize;

/// Run the welcome phase.
pub fn phase_welcome(prompter: &dyn TutorialPrompter) -> Result<()> {
    prompter.info("");
    prompter.info(&format!("{}", "Welcome to Ralph!".bright_cyan().bold()));
    prompter.info("");
    prompter.info("Ralph is an AI task queue for structured agent workflows.");
    prompter.info("This tutorial will guide you through the basics in a sandbox environment.");
    prompter.info("");
    prompter.info("You will learn how to:");
    prompter.info("  1. Initialize Ralph in a project");
    prompter.info("  2. Create a task");
    prompter.info("  3. Preview running a task (dry-run)");
    prompter.info("  4. Review the results");
    prompter.info("");

    prompter.pause("Press Enter to continue...")
}

/// Run the sandbox setup phase.
pub fn phase_setup(prompter: &dyn TutorialPrompter) -> Result<TutorialSandbox> {
    prompter.info("");
    prompter.info(&format!("{}", "Setting up sandbox...".bright_yellow()));
    prompter.info("");

    let sandbox = TutorialSandbox::create().context("failed to create tutorial sandbox")?;

    prompter.info(&format!(
        "Created sandbox at: {}",
        sandbox.path.display().to_string().bright_green()
    ));
    prompter.info("The sandbox contains a minimal Rust project for you to experiment with.");
    prompter.info("");

    prompter
        .pause("Press Enter to continue...")
        .map(|_| sandbox)
}

/// Run the init phase.
pub fn phase_init(prompter: &dyn TutorialPrompter, sandbox: &TutorialSandbox) -> Result<()> {
    prompter.info("");
    prompter.info(&format!(
        "{}",
        "Phase 1: Initialize Ralph".bright_cyan().bold()
    ));
    prompter.info("");
    prompter.info("First, let's initialize Ralph in the sandbox project.");
    prompter.info("This creates the .ralph/ directory with config, queue, and done files.");
    prompter.info("");
    prompter.info("Running: ralph init --non-interactive");
    prompter.info("");

    // Call init programmatically
    let original_dir = std::env::current_dir().context("get current dir")?;
    std::env::set_current_dir(&sandbox.path).context("change to sandbox dir")?;

    let result = (|| -> Result<()> {
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
        Ok(())
    })();

    std::env::set_current_dir(&original_dir).context("restore original dir")?;

    result.context("ralph init failed")?;

    prompter.info(&format!("{}", "Initialization complete!".bright_green()));
    prompter.info("");
    prompter.info("Created files:");
    prompter.info(&format!(
        "  - {}",
        sandbox.path.join(".ralph/config.jsonc").display()
    ));
    prompter.info(&format!(
        "  - {}",
        sandbox.path.join(".ralph/queue.jsonc").display()
    ));
    prompter.info(&format!(
        "  - {}",
        sandbox.path.join(".ralph/done.jsonc").display()
    ));
    prompter.info("");

    prompter.pause("Press Enter to continue...")
}

/// Run the task creation phase.
pub fn phase_create_task(
    prompter: &dyn TutorialPrompter,
    sandbox: &TutorialSandbox,
) -> Result<String> {
    prompter.info("");
    prompter.info(&format!(
        "{}",
        "Phase 2: Create a Task".bright_cyan().bold()
    ));
    prompter.info("");
    prompter.info("Now let's add a task to the queue.");
    prompter.info("Tasks describe work for AI agents to complete.");
    prompter.info("");

    // For tutorial, directly add a task to queue.jsonc instead of invoking runner
    let task_id = add_tutorial_task(sandbox)?;

    prompter.info(&format!("Created task: {}", task_id.bright_green()));
    prompter.info("");
    prompter.info("Task details:");
    prompter.info("  ID: RQ-0001");
    prompter.info("  Title: Add a farewell function");
    prompter.info("  Description: Create a farewell() function in src/lib.rs");
    prompter.info("  Priority: medium");
    prompter.info("");

    prompter
        .pause("Press Enter to continue...")
        .map(|_| task_id)
}

/// Add a tutorial task directly to queue.jsonc.
fn add_tutorial_task(sandbox: &TutorialSandbox) -> Result<String> {
    let queue_path = sandbox.path.join(".ralph/queue.jsonc");

    let task = crate::contracts::Task {
        id: "RQ-0001".to_string(),
        title: "Add a farewell function".to_string(),
        description: Some(
            "Create a farewell() function in src/lib.rs that returns a goodbye message."
                .to_string(),
        ),
        status: crate::contracts::TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec!["tutorial".to_string()],
        scope: vec!["src/lib.rs".to_string()],
        evidence: vec![],
        plan: vec![
            "Add farewell function to src/lib.rs".to_string(),
            "Add test for farewell function".to_string(),
        ],
        notes: vec![],
        request: Some("Add a farewell function to the library".to_string()),
        agent: None,
        created_at: Some(crate::timeutil::now_utc_rfc3339_or_fallback()),
        updated_at: Some(crate::timeutil::now_utc_rfc3339_or_fallback()),
        completed_at: None,
        started_at: None,
        estimated_minutes: None,
        actual_minutes: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
    };

    let queue = crate::contracts::QueueFile {
        version: 1,
        tasks: vec![task],
    };

    crate::queue::save_queue(&queue_path, &queue).context("save queue with tutorial task")?;

    Ok("RQ-0001".to_string())
}

/// Run the dry-run phase.
pub fn phase_dry_run(
    prompter: &dyn TutorialPrompter,
    sandbox: &TutorialSandbox,
    task_id: &str,
) -> Result<()> {
    prompter.info("");
    prompter.info(&format!(
        "{}",
        "Phase 3: Preview Running a Task".bright_cyan().bold()
    ));
    prompter.info("");
    prompter.info("Let's preview what happens when we run a task (dry-run mode).");
    prompter.info("Dry-run shows what would happen without actually invoking an AI runner.");
    prompter.info("");
    prompter.info(&format!(
        "Running: ralph run one --id {} --dry-run",
        task_id
    ));
    prompter.info("");

    // Change to sandbox and run dry-run
    let original_dir = std::env::current_dir().context("get current dir")?;
    std::env::set_current_dir(&sandbox.path).context("change to sandbox dir")?;

    let result = (|| -> Result<()> {
        let resolved = crate::config::resolve_from_cwd()?;
        let agent_overrides = crate::agent::AgentOverrides::default();
        crate::commands::run::dry_run_one(&resolved, &agent_overrides, Some(task_id))
    })();

    std::env::set_current_dir(&original_dir).context("restore original dir")?;

    result.context("dry-run failed")?;

    prompter.info("");
    prompter.info("The dry-run shows what the runner would do without actually executing.");
    prompter.info("");

    prompter.pause("Press Enter to continue...")
}

/// Run the review phase.
pub fn phase_review(prompter: &dyn TutorialPrompter, sandbox: &TutorialSandbox) -> Result<()> {
    prompter.info("");
    prompter.info(&format!("{}", "Phase 4: Review".bright_cyan().bold()));
    prompter.info("");
    prompter.info("Let's look at the queue to see your task.");
    prompter.info("");

    prompter.info("Running: ralph queue list");
    prompter.info("");

    // Show queue
    let queue_path = sandbox.path.join(".ralph/queue.jsonc");
    let queue = crate::queue::load_queue(&queue_path).context("load queue for review")?;

    for task in &queue.tasks {
        let status = match task.status {
            crate::contracts::TaskStatus::Todo => "todo".bright_blue(),
            crate::contracts::TaskStatus::Doing => "doing".bright_yellow(),
            crate::contracts::TaskStatus::Done => "done".bright_green(),
            crate::contracts::TaskStatus::Rejected => "rejected".bright_red(),
            crate::contracts::TaskStatus::Draft => "draft".dimmed(),
        };
        prompter.info(&format!(
            "  {} [{}] {}",
            task.id.bright_cyan(),
            status,
            task.title
        ));
    }

    prompter.info("");
    prompter.info("Great job! You've completed the basic Ralph tutorial.");
    prompter.info("");
    prompter.info("Key takeaways:");
    prompter.info("  - ralph init: Set up Ralph in a project");
    prompter.info("  - ralph task: Create tasks for AI agents");
    prompter.info("  - ralph run one: Execute a task");
    prompter.info("  - ralph queue list: View your tasks");
    prompter.info("");
    prompter.info("For more information, see:");
    prompter.info("  - ralph --help");
    prompter.info("  - docs/cli.md");
    prompter.info("  - docs/quick-start.md");
    prompter.info("");

    Ok(())
}

/// Run the cleanup phase.
pub fn phase_cleanup(
    prompter: &dyn TutorialPrompter,
    sandbox: TutorialSandbox,
    keep_sandbox: bool,
) -> Result<()> {
    prompter.info("");
    prompter.info(&format!("{}", "Cleanup".bright_yellow()));
    prompter.info("");

    if keep_sandbox {
        let path = sandbox.preserve();
        prompter.info(&format!(
            "Sandbox preserved at: {}",
            path.display().to_string().bright_green()
        ));
        prompter.info("You can continue experimenting in this directory.");
    } else {
        prompter.info("Cleaning up sandbox directory...");
        // sandbox auto-drops here
    }

    prompter.info("");
    prompter.info(&format!(
        "{}",
        "Tutorial complete! Happy coding!".bright_green().bold()
    ));
    prompter.info("");

    Ok(())
}
