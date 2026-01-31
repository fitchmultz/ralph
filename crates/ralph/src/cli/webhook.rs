//! Webhook CLI commands.
//!
//! Responsibilities:
//! - Provide `ralph webhook test` command for testing webhook configuration.
//!
//! Does NOT handle:
//! - Webhook configuration management (use config files).
//! - Persistent webhook state.

use anyhow::{bail, Result};
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
        after_long_help = "Examples:\n  ralph webhook test\n  ralph webhook test --event task_created\n  ralph webhook test --url https://example.com/webhook"
    )]
    Test(TestArgs),
}

#[derive(Args)]
pub struct TestArgs {
    /// Event type to send (default: task_created).
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
}

pub fn handle_webhook(args: &WebhookArgs, resolved: &crate::config::Resolved) -> Result<()> {
    match &args.command {
        WebhookCommand::Test(test_args) => handle_test(test_args, resolved),
    }
}

fn handle_test(args: &TestArgs, resolved: &crate::config::Resolved) -> Result<()> {
    use crate::timeutil;
    use crate::webhook::{send_webhook, WebhookEventType};

    let mut config = resolved.config.agent.webhook.clone();

    // Override URL if provided
    if let Some(url) = &args.url {
        config.url = Some(url.clone());
    }

    // Ensure enabled for test
    config.enabled = Some(true);

    // Validate URL exists
    if config.url.is_none() || config.url.as_ref().unwrap().is_empty() {
        bail!("Webhook URL not configured. Set it in config or use --url.");
    }

    // Parse event type
    let event_type = match args.event.as_str() {
        "task_created" => WebhookEventType::TaskCreated,
        "task_started" => WebhookEventType::TaskStarted,
        "task_completed" => WebhookEventType::TaskCompleted,
        "task_failed" => WebhookEventType::TaskFailed,
        "task_status_changed" => WebhookEventType::TaskStatusChanged,
        _ => bail!("Unknown event type: {}. Supported: task_created, task_started, task_completed, task_failed, task_status_changed", args.event),
    };

    println!("Sending test webhook...");
    println!("  URL: {}", config.url.as_ref().unwrap());
    println!("  Event: {}", args.event);
    println!("  Task ID: {}", args.task_id);

    let now = timeutil::now_utc_rfc3339()?;

    send_webhook(
        event_type,
        &args.task_id,
        &args.task_title,
        None,
        None,
        Some("Test webhook from ralph webhook test command"),
        &config,
        &now,
    );

    println!("Test webhook sent successfully.");
    Ok(())
}
