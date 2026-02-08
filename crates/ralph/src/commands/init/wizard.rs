//! Interactive onboarding wizard for Ralph initialization.
//!
//! Responsibilities:
//! - Display welcome screen and collect user preferences.
//! - Guide users through runner, model, and phase selection.
//! - Optionally create a first task during setup.
//!
//! Not handled here:
//! - File creation (see `super::writers`).
//! - CLI argument parsing (handled by CLI layer).
//!
//! Invariants/assumptions:
//! - Wizard is only run in interactive TTY environments.
//! - User inputs are validated before returning WizardAnswers.

use crate::contracts::{Runner, TaskPriority};
use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select};
use std::path::Path;

/// Answers collected from the interactive wizard.
#[derive(Debug, Clone)]
pub struct WizardAnswers {
    /// Selected AI runner.
    pub runner: Runner,
    /// Selected model (as string for flexibility).
    pub model: String,
    /// Number of phases (1, 2, or 3).
    pub phases: u8,
    /// Whether to create a first task.
    pub create_first_task: bool,
    /// Title for the first task (if created).
    pub first_task_title: Option<String>,
    /// Description/request for the first task (if created).
    pub first_task_description: Option<String>,
    /// Priority for the first task.
    pub first_task_priority: TaskPriority,
}

impl Default for WizardAnswers {
    fn default() -> Self {
        Self {
            runner: Runner::Claude,
            model: "sonnet".to_string(),
            phases: 3,
            create_first_task: false,
            first_task_title: None,
            first_task_description: None,
            first_task_priority: TaskPriority::Medium,
        }
    }
}

/// Run the interactive onboarding wizard and collect user preferences.
pub fn run_wizard() -> Result<WizardAnswers> {
    // Welcome screen
    print_welcome();

    // Runner selection
    let runners = [
        (
            "Claude",
            "Anthropic's Claude Code CLI - Best for complex reasoning",
        ),
        ("Codex", "OpenAI's Codex CLI - Great for code generation"),
        ("OpenCode", "OpenCode agent - Open source alternative"),
        (
            "Gemini",
            "Google's Gemini CLI - Good for large context windows",
        ),
        ("Cursor", "Cursor's agent mode - IDE-integrated workflow"),
    ];

    let runner_idx = Select::new()
        .with_prompt("Select your AI runner")
        .items(
            &runners
                .iter()
                .map(|(name, desc)| format!("{} - {}", name, desc))
                .collect::<Vec<_>>(),
        )
        .default(0)
        .interact()
        .context("failed to get runner selection")?;

    let runner = match runner_idx {
        0 => Runner::Claude,
        1 => Runner::Codex,
        2 => Runner::Opencode,
        3 => Runner::Gemini,
        4 => Runner::Cursor,
        _ => Runner::Claude, // default fallback
    };

    // Model selection based on runner
    let model = select_model(&runner)?;

    // Phase selection
    let phases = select_phases()?;

    // First task creation
    let create_first_task = Confirm::new()
        .with_prompt("Would you like to create your first task now?")
        .default(true)
        .interact()
        .context("failed to get first task confirmation")?;

    let (first_task_title, first_task_description, first_task_priority) = if create_first_task {
        let title: String = Input::new()
            .with_prompt("Task title")
            .allow_empty(false)
            .interact_text()
            .context("failed to get task title")?;

        let description: String = Input::new()
            .with_prompt("Task description (what should be done)")
            .allow_empty(true)
            .interact_text()
            .context("failed to get task description")?;

        let priorities = vec!["Low", "Medium", "High", "Critical"];
        let priority_idx = Select::new()
            .with_prompt("Task priority")
            .items(&priorities)
            .default(1)
            .interact()
            .context("failed to get priority selection")?;

        let priority = match priority_idx {
            0 => TaskPriority::Low,
            1 => TaskPriority::Medium,
            2 => TaskPriority::High,
            3 => TaskPriority::Critical,
            _ => TaskPriority::Medium,
        };

        (Some(title), Some(description), priority)
    } else {
        (None, None, TaskPriority::Medium)
    };

    // Summary and confirmation
    let answers = WizardAnswers {
        runner,
        model,
        phases,
        create_first_task,
        first_task_title,
        first_task_description,
        first_task_priority,
    };

    print_summary(&answers);

    let proceed = Confirm::new()
        .with_prompt("Proceed with setup?")
        .default(true)
        .interact()
        .context("failed to get confirmation")?;

    if !proceed {
        anyhow::bail!("Setup cancelled by user");
    }

    Ok(answers)
}

/// Print the welcome screen with ASCII art.
fn print_welcome() {
    println!();
    println!(
        "{}",
        colored::Colorize::bright_cyan(r"    ____       __        __")
    );
    println!(
        "{}",
        colored::Colorize::bright_cyan(r"   / __ \___  / /_____  / /_____ ___")
    );
    println!(
        "{}",
        colored::Colorize::bright_cyan(r"  / /_/ / _ \/ __/ __ \/ __/ __ `__ \ ")
    );
    println!(
        "{}",
        colored::Colorize::bright_cyan(r" / _, _/  __/ /_/ /_/ / /_/ / / / / /")
    );
    println!(
        "{}",
        colored::Colorize::bright_cyan(r"/_/ |_|\___/\__/ .___/\__/_/ /_/ /_/")
    );
    println!("{}", colored::Colorize::bright_cyan(r"             /_/"));
    println!();
    println!("{}", colored::Colorize::bold("Welcome to Ralph!"));
    println!();
    println!("Ralph is an AI task queue for structured agent workflows.");
    println!("This wizard will help you set up your project and create your first task.");
    println!();
}

/// Select model based on the chosen runner.
fn select_model(runner: &Runner) -> Result<String> {
    let models: Vec<(&str, &str)> = match runner {
        Runner::Claude => vec![
            ("sonnet", "Balanced speed and intelligence (recommended)"),
            ("opus", "Most powerful, best for complex tasks"),
            ("haiku", "Fastest, good for simple tasks"),
            ("custom", "Other model (specify)"),
        ],
        Runner::Codex => vec![
            ("gpt-5.3-codex", "Codex optimized for coding (recommended)"),
            ("gpt-5.3", "General GPT-5.3"),
            ("gpt-5.2-codex", "Codex optimized for coding (legacy)"),
            ("gpt-5.2", "General GPT-5.2 (legacy)"),
            ("custom", "Other model (specify)"),
        ],
        Runner::Gemini => vec![
            (
                "zai-coding-plan/glm-4.7",
                "Default Gemini model (recommended)",
            ),
            ("custom", "Other model (specify)"),
        ],
        _ => vec![
            ("default", "Use runner default"),
            ("custom", "Specify custom model"),
        ],
    };

    let items: Vec<String> = models
        .iter()
        .map(|(name, desc)| format!("{} - {}", name, desc))
        .collect();

    let idx = Select::new()
        .with_prompt("Select model")
        .items(&items)
        .default(0)
        .interact()
        .context("failed to get model selection")?;

    let selected = models[idx].0;

    if selected == "custom" {
        let custom: String = Input::new()
            .with_prompt("Enter model name")
            .allow_empty(false)
            .interact_text()
            .context("failed to get custom model")?;
        Ok(custom)
    } else {
        Ok(selected.to_string())
    }
}

/// Select the number of phases with explanations.
fn select_phases() -> Result<u8> {
    let phase_options = [
        (
            "3-phase (Full)",
            "Plan → Implement + CI → Review + Complete [Recommended]",
        ),
        (
            "2-phase (Standard)",
            "Plan → Implement (faster, less review)",
        ),
        (
            "1-phase (Quick)",
            "Single-pass execution (simple fixes only)",
        ),
    ];

    let items: Vec<String> = phase_options
        .iter()
        .map(|(name, desc)| format!("{} - {}", name, desc))
        .collect();

    let idx = Select::new()
        .with_prompt("Select workflow mode")
        .items(&items)
        .default(0)
        .interact()
        .context("failed to get phase selection")?;

    Ok(match idx {
        0 => 3,
        1 => 2,
        2 => 1,
        _ => 3,
    })
}

/// Print a summary of the wizard answers.
fn print_summary(answers: &WizardAnswers) {
    println!();
    println!("{}", colored::Colorize::bold("Setup Summary:"));
    println!("{}", colored::Colorize::bright_black("──────────────"));
    println!(
        "Runner: {} ({})",
        colored::Colorize::bright_green(format!("{:?}", answers.runner).as_str()),
        answers.model
    );
    println!(
        "Workflow: {}-phase",
        colored::Colorize::bright_green(format!("{}", answers.phases).as_str())
    );

    if answers.create_first_task {
        if let Some(ref title) = answers.first_task_title {
            println!(
                "First Task: {}",
                colored::Colorize::bright_green(title.as_str())
            );
        }
    } else {
        println!("First Task: {}", colored::Colorize::bright_black("(none)"));
    }

    println!();
    println!("Files to create:");
    println!("  - .ralph/config.json");
    println!("  - .ralph/queue.json");
    println!("  - .ralph/done.json");
    println!();
}

/// Print completion message with next steps.
pub fn print_completion_message(answers: Option<&WizardAnswers>, _queue_path: &Path) {
    println!();
    println!(
        "{}",
        colored::Colorize::bright_green("✓ Ralph initialized successfully!")
    );
    println!();
    println!("{}", colored::Colorize::bold("Next steps:"));
    println!("  1. Run 'ralph app open' to open the macOS app (optional)");
    println!("  2. Run 'ralph run one' to execute your first task");
    println!("  3. Edit .ralph/config.json to customize settings");

    if let Some(answers) = answers
        && answers.create_first_task
    {
        println!();
        println!("Your first task is ready to go!");
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wizard_answers_default() {
        let answers = WizardAnswers::default();
        assert_eq!(answers.runner, Runner::Claude);
        assert_eq!(answers.model, "sonnet");
        assert_eq!(answers.phases, 3);
        assert!(!answers.create_first_task);
        assert!(answers.first_task_title.is_none());
        assert!(answers.first_task_description.is_none());
        assert_eq!(answers.first_task_priority, TaskPriority::Medium);
    }
}
