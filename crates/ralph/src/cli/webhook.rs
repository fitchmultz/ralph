//! Webhook CLI commands.
//!
//! Purpose:
//! - Webhook CLI commands.
//!
//! Responsibilities:
//! - Provide `ralph webhook test` command for testing webhook configuration.
//! - Provide `ralph webhook status` for diagnostics snapshots.
//! - Provide `ralph webhook replay` for explicit bounded failure replay.
//!
//! Non-scope:
//! - Webhook configuration management (use config files).
//! - Direct HTTP delivery internals (delegated to `crate::webhook`).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};

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
    /// Show webhook delivery diagnostics and recent failures.
    #[command(
        after_long_help = "Examples:\n  ralph webhook status\n  ralph webhook status --recent 10\n  ralph webhook status --format json"
    )]
    Status(StatusArgs),
    /// Replay failed webhook deliveries with explicit targeting.
    #[command(
        after_long_help = "Examples:\n  ralph webhook replay --id wf-1700000000-1 --dry-run\n  ralph webhook replay --event task_completed --limit 5\n  ralph webhook replay --task-id RQ-0814 --max-replay-attempts 3"
    )]
    Replay(ReplayArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum WebhookStatusFormat {
    #[default]
    Text,
    Json,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = WebhookStatusFormat::Text)]
    pub format: WebhookStatusFormat,

    /// Number of recent failure records to include.
    #[arg(long, default_value_t = 20)]
    pub recent: usize,
}

#[derive(Args)]
pub struct ReplayArgs {
    /// Replay a specific failure record ID (repeatable).
    #[arg(long = "id")]
    pub ids: Vec<String>,

    /// Replay failures matching an event name (e.g., task_completed).
    #[arg(long)]
    pub event: Option<String>,

    /// Replay failures matching a task ID.
    #[arg(long)]
    pub task_id: Option<String>,

    /// Maximum matched failures to consider for this invocation.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    /// Maximum allowed replay attempts per failure record.
    #[arg(long, default_value_t = 3)]
    pub max_replay_attempts: u32,

    /// Preview replay candidates without enqueueing.
    #[arg(long)]
    pub dry_run: bool,
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
        WebhookCommand::Status(status_args) => handle_status(status_args, resolved),
        WebhookCommand::Replay(replay_args) => handle_replay(replay_args, resolved),
    }
}

fn handle_test(args: &TestArgs, resolved: &crate::config::Resolved) -> Result<()> {
    use crate::contracts::WebhookEventSubscription;
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

    crate::contracts::validate_webhook_settings(&config)?;

    // For non-task events, temporarily enable them for this test
    // This ensures new events can be tested without modifying config
    if config.events.is_none() {
        // Parse the event string into WebhookEventSubscription
        let event_sub: WebhookEventSubscription =
            serde_json::from_str(&format!("\"{}\"", args.event))
                .map_err(|e| anyhow::anyhow!("Invalid event type '{}': {}", args.event, e))?;
        config.events = Some(vec![event_sub]);
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

fn handle_status(args: &StatusArgs, resolved: &crate::config::Resolved) -> Result<()> {
    let diagnostics = crate::webhook::diagnostics_snapshot(
        &resolved.repo_root,
        &resolved.config.agent.webhook,
        args.recent,
    )?;

    match args.format {
        WebhookStatusFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&diagnostics)?);
        }
        WebhookStatusFormat::Text => {
            println!("Webhook delivery diagnostics");
            println!("  queue depth: {}", diagnostics.queue_depth);
            println!("  queue capacity: {}", diagnostics.queue_capacity);
            println!("  queue policy: {:?}", diagnostics.queue_policy);
            println!("  enqueued total: {}", diagnostics.enqueued_total);
            println!("  delivered total: {}", diagnostics.delivered_total);
            println!("  failed total: {}", diagnostics.failed_total);
            println!("  dropped total: {}", diagnostics.dropped_total);
            println!(
                "  retry attempts total: {}",
                diagnostics.retry_attempts_total
            );
            println!("  failure store: {}", diagnostics.failure_store_path);

            if diagnostics.recent_failures.is_empty() {
                println!("  recent failures: none");
            } else {
                println!("  recent failures:");
                for record in diagnostics.recent_failures {
                    let task = record.task_id.as_deref().unwrap_or("-");
                    println!(
                        "    {} event={} task={} attempts={} replay_count={} at={} error={}",
                        record.id,
                        record.event,
                        task,
                        record.attempts,
                        record.replay_count,
                        record.failed_at,
                        record.error
                    );
                }
            }
        }
    }

    Ok(())
}

fn handle_replay(args: &ReplayArgs, resolved: &crate::config::Resolved) -> Result<()> {
    if args.ids.is_empty() && args.event.is_none() && args.task_id.is_none() {
        bail!("Refusing broad replay. Provide --id, --event, or --task-id.");
    }

    let selector = crate::webhook::ReplaySelector {
        ids: args.ids.clone(),
        event: args.event.clone(),
        task_id: args.task_id.clone(),
        limit: args.limit,
        max_replay_attempts: args.max_replay_attempts,
    };

    let report = crate::webhook::replay_failed_deliveries(
        &resolved.repo_root,
        &resolved.config.agent.webhook,
        &selector,
        args.dry_run,
    )?;

    if report.dry_run {
        println!(
            "Dry-run: matched {}, eligible {}, skipped over replay cap {}",
            report.matched_count, report.eligible_count, report.skipped_max_replay_attempts
        );
    } else {
        println!(
            "Replay complete: matched {}, replayed {}, skipped over replay cap {}, skipped enqueue failures {}",
            report.matched_count,
            report.replayed_count,
            report.skipped_max_replay_attempts,
            report.skipped_enqueue_failures
        );
    }

    if report.candidates.is_empty() {
        println!("No matching failure records.");
    } else {
        println!("Candidates:");
        for candidate in report.candidates {
            let task = candidate.task_id.as_deref().unwrap_or("-");
            println!(
                "  {} event={} task={} attempts={} replay_count={} eligible={} at={}",
                candidate.id,
                candidate.event,
                task,
                candidate.attempts,
                candidate.replay_count,
                candidate.eligible_for_replay,
                candidate.failed_at
            );
        }
    }

    Ok(())
}
