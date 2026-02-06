//! Webhook CLI commands.
//!
//! Responsibilities:
//! - Provide `ralph webhook test` command for testing webhook configuration.
//!
//! Does NOT handle:
//! - Webhook configuration management (use config files).
//! - Persistent webhook state.

use anyhow::{Result, bail};
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct WebhookArgs {
    #[command(subcommand)]
    pub command: WebhookCommand,
}

#[derive(Subcommand)]
pub enum WebhookCommand {
    /// Test webhook configuration by sending a test event.
    #[command(
        after_long_help = "Examples:\n  ralph webhook test\n  ralph webhook test --event task_created\n  ralph webhook test --event phase_started --print-json\n  ralph webhook test --url https://example.com/webhook"
    )]
    Test(TestArgs),
}

#[derive(Args)]
pub struct TestArgs {
    /// Event type to send (default: task_created).
    /// Supported: task_created, task_started, task_completed, task_failed, task_status_changed,
    ///            loop_started, loop_stopped, phase_started, phase_completed
    #[arg(short, long, default_value = "task_created")]
    pub event: String,

    /// Override webhook URL (uses config if not specified).
    #[arg(short, long)]
    pub url: Option<String>,

    /// Task ID to use in test payload (default: TEST-0001).
    #[arg(long, default_value = "TEST-0001")]
    pub task_id: String,

    /// Task title to use in test payload.
    #[arg(long, default_value = "Test webhook notification")]
    pub task_title: String,

    /// Print the JSON payload that would be sent (without sending).
    #[arg(long)]
    pub print_json: bool,

    /// Pretty-print the JSON payload (only used with --print-json).
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub pretty: bool,
}

pub fn handle_webhook(args: &WebhookArgs, resolved: &crate::config::Resolved) -> Result<()> {
    match &args.command {
        WebhookCommand::Test(test_args) => handle_test(test_args, resolved),
    }
}

fn handle_test(args: &TestArgs, resolved: &crate::config::Resolved) -> Result<()> {
    use crate::timeutil;
    use crate::webhook::{WebhookContext, WebhookEventType, WebhookPayload, send_webhook_payload};
    use std::str::FromStr;

    let mut config = resolved.config.agent.webhook.clone();

    // Override URL if provided
    if let Some(url) = &args.url {
        config.url = Some(url.clone());
    }

    // Ensure enabled for test
    config.enabled = Some(true);

    // For non-task events, temporarily enable them for this test
    // This ensures new events can be tested without modifying config
    if config.events.is_none() {
        config.events = Some(vec![args.event.clone()]);
    }

    // Parse event type using FromStr
    let event_type = WebhookEventType::from_str(&args.event)?;

    // Build the payload
    let now = timeutil::now_utc_rfc3339()?;

    let note = Some("Test webhook from ralph webhook test command".to_string());

    let (task_id, task_title, previous_status, current_status, context) = match event_type {
        WebhookEventType::LoopStarted | WebhookEventType::LoopStopped => {
            // Loop events don't have task association
            (
                None,
                None,
                None,
                None,
                WebhookContext {
                    repo_root: Some(resolved.repo_root.display().to_string()),
                    branch: crate::git::current_branch(&resolved.repo_root).ok(),
                    commit: crate::session::get_git_head_commit(&resolved.repo_root),
                    ..Default::default()
                },
            )
        }
        WebhookEventType::PhaseStarted | WebhookEventType::PhaseCompleted => {
            // Phase events have task context plus phase metadata
            (
                Some(args.task_id.clone()),
                Some(args.task_title.clone()),
                None,
                None,
                WebhookContext {
                    runner: Some("claude".to_string()),
                    model: Some("sonnet".to_string()),
                    phase: Some(2),
                    phase_count: Some(3),
                    duration_ms: Some(15000),
                    repo_root: Some(resolved.repo_root.display().to_string()),
                    branch: crate::git::current_branch(&resolved.repo_root).ok(),
                    commit: crate::session::get_git_head_commit(&resolved.repo_root),
                    ci_gate: Some("passed".to_string()),
                },
            )
        }
        WebhookEventType::TaskStarted => (
            Some(args.task_id.clone()),
            Some(args.task_title.clone()),
            Some("todo".to_string()),
            Some("doing".to_string()),
            WebhookContext::default(),
        ),
        WebhookEventType::TaskCompleted => (
            Some(args.task_id.clone()),
            Some(args.task_title.clone()),
            Some("doing".to_string()),
            Some("done".to_string()),
            WebhookContext::default(),
        ),
        WebhookEventType::TaskFailed => (
            Some(args.task_id.clone()),
            Some(args.task_title.clone()),
            Some("doing".to_string()),
            Some("rejected".to_string()),
            WebhookContext::default(),
        ),
        WebhookEventType::TaskStatusChanged => (
            Some(args.task_id.clone()),
            Some(args.task_title.clone()),
            Some("todo".to_string()),
            Some("doing".to_string()),
            WebhookContext::default(),
        ),
        _ => {
            // Task events
            (
                Some(args.task_id.clone()),
                Some(args.task_title.clone()),
                None,
                None,
                WebhookContext::default(),
            )
        }
    };

    let payload = WebhookPayload {
        event: event_type.as_str().to_string(),
        timestamp: now.clone(),
        task_id,
        task_title,
        previous_status,
        current_status,
        note,
        context,
    };

    // Print JSON if requested
    if args.print_json {
        let json = if args.pretty {
            serde_json::to_string_pretty(&payload)?
        } else {
            serde_json::to_string(&payload)?
        };
        println!("{}", json);
        return Ok(());
    }

    // Validate URL exists before sending
    if config.url.is_none() || config.url.as_ref().unwrap().is_empty() {
        bail!("Webhook URL not configured. Set it in config or use --url.");
    }

    println!("Sending test webhook...");
    println!("  URL: {}", config.url.as_ref().unwrap());
    println!("  Event: {}", args.event);
    if payload.task_id.is_some() {
        println!("  Task ID: {}", args.task_id);
    }

    send_webhook_payload(payload, &config);

    println!("Test webhook sent successfully.");
    Ok(())
}
