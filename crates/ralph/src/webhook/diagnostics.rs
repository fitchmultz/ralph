//! Webhook diagnostics and replay support.
//!
//! Responsibilities:
//! - Track in-process webhook delivery metrics (queue depth, totals, retries).
//! - Persist failed delivery records to a bounded repo-local failure store.
//! - Provide status snapshots and bounded replay helpers for CLI commands.
//!
//! Does NOT handle:
//! - Guaranteed delivery semantics across process restarts.
//! - External observability backends or remote metric exports.
//! - Authentication/endpoint registration management.
//!
//! Invariants/assumptions:
//! - Failure store writes are best-effort for runtime delivery failures.
//! - Failure records never include webhook secrets/headers.
//! - Replay is always explicit and bounded by caller-provided selectors/caps.

use super::{
    WebhookMessage, WebhookPayload, enqueue_webhook_payload_for_replay, resolve_webhook_config,
};
use crate::contracts::{WebhookConfig, WebhookQueuePolicy};
use crate::fsutil;
use crate::redaction;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const WEBHOOK_FAILURE_STORE_RELATIVE_PATH: &str = ".ralph/cache/webhooks/failures.json";
const MAX_WEBHOOK_FAILURE_RECORDS: usize = 200;
const MAX_FAILURE_ERROR_CHARS: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookFailureRecord {
    pub id: String,
    pub failed_at: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub error: String,
    pub attempts: u32,
    pub replay_count: u32,
    pub payload: WebhookPayload,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookDiagnostics {
    pub queue_depth: usize,
    pub queue_capacity: usize,
    pub queue_policy: WebhookQueuePolicy,
    pub enqueued_total: u64,
    pub delivered_total: u64,
    pub failed_total: u64,
    pub dropped_total: u64,
    pub retry_attempts_total: u64,
    pub failure_store_path: String,
    pub recent_failures: Vec<WebhookFailureRecord>,
}

#[derive(Debug, Clone)]
pub struct ReplaySelector {
    pub ids: Vec<String>,
    pub event: Option<String>,
    pub task_id: Option<String>,
    pub limit: usize,
    pub max_replay_attempts: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayCandidate {
    pub id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub failed_at: String,
    pub attempts: u32,
    pub replay_count: u32,
    pub error: String,
    pub eligible_for_replay: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayReport {
    pub dry_run: bool,
    pub matched_count: usize,
    pub eligible_count: usize,
    pub replayed_count: usize,
    pub skipped_max_replay_attempts: usize,
    pub skipped_enqueue_failures: usize,
    pub candidates: Vec<ReplayCandidate>,
}

#[derive(Debug, Clone)]
struct SelectedReplayRecord {
    id: String,
    replay_count: u32,
    payload: WebhookPayload,
}

#[derive(Debug, Default)]
struct WebhookMetrics {
    queue_depth: AtomicUsize,
    queue_capacity: AtomicUsize,
    enqueued_total: AtomicU64,
    delivered_total: AtomicU64,
    failed_total: AtomicU64,
    dropped_total: AtomicU64,
    retry_attempts_total: AtomicU64,
}

static METRICS: OnceLock<WebhookMetrics> = OnceLock::new();
static FAILURE_STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static NEXT_FAILURE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn metrics() -> &'static WebhookMetrics {
    METRICS.get_or_init(WebhookMetrics::default)
}

fn failure_store_lock() -> &'static Mutex<()> {
    FAILURE_STORE_LOCK.get_or_init(|| Mutex::new(()))
}

pub fn set_queue_capacity(capacity: usize) {
    metrics().queue_capacity.store(capacity, Ordering::SeqCst);
}

pub fn note_queue_dequeue() {
    let depth = &metrics().queue_depth;
    let _ = depth.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
        Some(current.saturating_sub(1))
    });
}

pub fn note_enqueue_success() {
    let state = metrics();
    state.enqueued_total.fetch_add(1, Ordering::SeqCst);
    state.queue_depth.fetch_add(1, Ordering::SeqCst);
}

pub fn note_dropped_message() {
    metrics().dropped_total.fetch_add(1, Ordering::SeqCst);
}

pub fn note_retry_attempt() {
    metrics()
        .retry_attempts_total
        .fetch_add(1, Ordering::SeqCst);
}

pub fn note_delivery_success() {
    metrics().delivered_total.fetch_add(1, Ordering::SeqCst);
}

pub fn note_delivery_failure(msg: &WebhookMessage, err: &anyhow::Error, attempts: u32) {
    metrics().failed_total.fetch_add(1, Ordering::SeqCst);

    if let Err(write_err) = persist_failed_delivery(msg, err, attempts) {
        log::warn!("Failed to persist webhook failure record: {write_err:#}");
    }
}

pub fn failure_store_path(repo_root: &Path) -> PathBuf {
    repo_root.join(WEBHOOK_FAILURE_STORE_RELATIVE_PATH)
}

pub fn diagnostics_snapshot(
    repo_root: &Path,
    config: &WebhookConfig,
    recent_limit: usize,
) -> Result<WebhookDiagnostics> {
    let path = failure_store_path(repo_root);
    let records = load_failure_records(&path)?;
    let limit = if recent_limit == 0 {
        records.len()
    } else {
        recent_limit
    };
    let recent_failures = records.into_iter().rev().take(limit).collect::<Vec<_>>();

    let state = metrics();
    let configured_capacity = config
        .queue_capacity
        .map(|value| value.clamp(1, 10000) as usize)
        .unwrap_or(500);
    let queue_capacity = match state.queue_capacity.load(Ordering::SeqCst) {
        0 => configured_capacity,
        value => value,
    };

    Ok(WebhookDiagnostics {
        queue_depth: state.queue_depth.load(Ordering::SeqCst),
        queue_capacity,
        queue_policy: config.queue_policy.unwrap_or_default(),
        enqueued_total: state.enqueued_total.load(Ordering::SeqCst),
        delivered_total: state.delivered_total.load(Ordering::SeqCst),
        failed_total: state.failed_total.load(Ordering::SeqCst),
        dropped_total: state.dropped_total.load(Ordering::SeqCst),
        retry_attempts_total: state.retry_attempts_total.load(Ordering::SeqCst),
        failure_store_path: path.display().to_string(),
        recent_failures,
    })
}

pub fn replay_failed_deliveries(
    repo_root: &Path,
    config: &WebhookConfig,
    selector: &ReplaySelector,
    dry_run: bool,
) -> Result<ReplayReport> {
    if selector.ids.is_empty() && selector.event.is_none() && selector.task_id.is_none() {
        bail!(
            "refusing unbounded replay (would redeliver all failures)\n\
             this could overwhelm external systems or re-trigger side effects\n\
             \n\
             examples:\n\
             \n\
             ralph webhook replay --id <failure-id>     # replay specific failure\n\
             ralph webhook replay --event task.done     # replay all task.done failures\n\
             ralph webhook replay --task-id RQ-0001     # replay failures for a task\n\
             ralph webhook replay --id a,b,c --limit 5  # replay up to 5 specific failures"
        );
    }

    if selector.max_replay_attempts == 0 {
        bail!("max_replay_attempts must be greater than 0");
    }

    if !dry_run {
        let resolved = resolve_webhook_config(config);
        if !resolved.enabled {
            bail!("Webhook replay requires agent.webhook.enabled=true");
        }
        if resolved
            .url
            .as_deref()
            .is_none_or(|url| url.trim().is_empty())
        {
            bail!("Webhook replay requires agent.webhook.url to be configured");
        }
    }

    let path = failure_store_path(repo_root);

    let limit = if selector.limit == 0 {
        usize::MAX
    } else {
        selector.limit
    };

    let id_filter = selector
        .ids
        .iter()
        .map(std::string::String::as_str)
        .collect::<HashSet<_>>();

    let (selected_records, candidates) = {
        let _guard = failure_store_lock()
            .lock()
            .map_err(|_| anyhow!("failed to acquire webhook failure store lock"))?;
        let records = load_failure_records_unlocked(&path)?;

        let mut selected_records = Vec::new();
        let mut candidates = Vec::new();

        for record in records.iter().rev() {
            if selected_records.len() >= limit {
                break;
            }
            if !id_filter.is_empty() && !id_filter.contains(record.id.as_str()) {
                continue;
            }
            if let Some(event_filter) = selector.event.as_deref()
                && record.event != event_filter
            {
                continue;
            }
            if let Some(task_filter) = selector.task_id.as_deref()
                && record.task_id.as_deref() != Some(task_filter)
            {
                continue;
            }

            let eligible = record.replay_count < selector.max_replay_attempts;
            candidates.push(ReplayCandidate {
                id: record.id.clone(),
                event: record.event.clone(),
                task_id: record.task_id.clone(),
                failed_at: record.failed_at.clone(),
                attempts: record.attempts,
                replay_count: record.replay_count,
                error: record.error.clone(),
                eligible_for_replay: eligible,
            });
            selected_records.push(SelectedReplayRecord {
                id: record.id.clone(),
                replay_count: record.replay_count,
                payload: record.payload.clone(),
            });
        }

        (selected_records, candidates)
    };

    let matched_count = candidates.len();
    let eligible_count = candidates
        .iter()
        .filter(|candidate| candidate.eligible_for_replay)
        .count();

    if dry_run {
        return Ok(ReplayReport {
            dry_run,
            matched_count,
            eligible_count,
            replayed_count: 0,
            skipped_max_replay_attempts: matched_count.saturating_sub(eligible_count),
            skipped_enqueue_failures: 0,
            candidates,
        });
    }

    let mut replayed_count = 0usize;
    let mut skipped_max_replay_attempts = 0usize;
    let mut skipped_enqueue_failures = 0usize;
    let mut replayed_ids = Vec::new();

    for record in selected_records {
        if record.replay_count >= selector.max_replay_attempts {
            skipped_max_replay_attempts += 1;
            continue;
        }

        if enqueue_webhook_payload_for_replay(record.payload, config) {
            replayed_ids.push(record.id);
            replayed_count += 1;
        } else {
            skipped_enqueue_failures += 1;
        }
    }

    if !replayed_ids.is_empty() {
        update_replay_counts(&path, &replayed_ids)?;
    }

    Ok(ReplayReport {
        dry_run,
        matched_count,
        eligible_count,
        replayed_count,
        skipped_max_replay_attempts,
        skipped_enqueue_failures,
        candidates,
    })
}

fn update_replay_counts(path: &Path, replayed_ids: &[String]) -> Result<()> {
    let replayed_set = replayed_ids
        .iter()
        .map(std::string::String::as_str)
        .collect::<HashSet<_>>();

    let _guard = failure_store_lock()
        .lock()
        .map_err(|_| anyhow!("failed to acquire webhook failure store lock"))?;
    let mut records = load_failure_records_unlocked(path)?;
    for record in &mut records {
        if replayed_set.contains(record.id.as_str()) {
            record.replay_count = record.replay_count.saturating_add(1);
        }
    }
    write_failure_records_unlocked(path, &records)
}

fn persist_failed_delivery(msg: &WebhookMessage, err: &anyhow::Error, attempts: u32) -> Result<()> {
    let repo_root = match resolve_repo_root_from_runtime(msg) {
        Some(path) => path,
        None => {
            log::debug!("Unable to resolve repo root for webhook failure persistence");
            return Ok(());
        }
    };

    let path = failure_store_path(&repo_root);
    persist_failed_delivery_at_path(&path, msg, err, attempts)
}

fn persist_failed_delivery_at_path(
    path: &Path,
    msg: &WebhookMessage,
    err: &anyhow::Error,
    attempts: u32,
) -> Result<()> {
    let _guard = failure_store_lock()
        .lock()
        .map_err(|_| anyhow!("failed to acquire webhook failure store lock"))?;

    let mut records = load_failure_records_unlocked(path)?;
    records.push(WebhookFailureRecord {
        id: next_failure_id(),
        failed_at: crate::timeutil::now_utc_rfc3339_or_fallback(),
        event: msg.payload.event.clone(),
        task_id: msg.payload.task_id.clone(),
        error: sanitize_error(err),
        attempts,
        replay_count: 0,
        payload: msg.payload.clone(),
    });

    if records.len() > MAX_WEBHOOK_FAILURE_RECORDS {
        let retain_from = records.len().saturating_sub(MAX_WEBHOOK_FAILURE_RECORDS);
        records.drain(..retain_from);
    }

    write_failure_records_unlocked(path, &records)
}

fn load_failure_records(path: &Path) -> Result<Vec<WebhookFailureRecord>> {
    let _guard = failure_store_lock()
        .lock()
        .map_err(|_| anyhow!("failed to acquire webhook failure store lock"))?;
    load_failure_records_unlocked(path)
}

fn load_failure_records_unlocked(path: &Path) -> Result<Vec<WebhookFailureRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("read webhook failure store {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str::<Vec<WebhookFailureRecord>>(&content)
        .with_context(|| format!("parse webhook failure store {}", path.display()))
}

fn write_failure_records_unlocked(path: &Path, records: &[WebhookFailureRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "create webhook failure store directory {}",
                parent.display()
            )
        })?;
    }

    let rendered = serde_json::to_string_pretty(records).context("serialize webhook failures")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write webhook failure store {}", path.display()))
}

fn resolve_repo_root_from_runtime(msg: &WebhookMessage) -> Option<PathBuf> {
    if let Some(repo_root) = msg.payload.context.repo_root.as_deref() {
        let repo_root = PathBuf::from(repo_root);
        if repo_root.exists() {
            return Some(crate::config::find_repo_root(&repo_root));
        }
        log::debug!(
            "webhook payload repo_root does not exist; falling back to current directory: {}",
            repo_root.display()
        );
    }

    let cwd = std::env::current_dir().ok()?;
    Some(crate::config::find_repo_root(&cwd))
}

fn next_failure_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_FAILURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("wf-{nanos}-{sequence}")
}

fn sanitize_error(err: &anyhow::Error) -> String {
    let redacted = redaction::redact_text(&err.to_string());
    let trimmed = redacted.trim();
    if trimmed.chars().count() <= MAX_FAILURE_ERROR_CHARS {
        return trimmed.to_string();
    }

    let truncated = trimmed
        .chars()
        .take(MAX_FAILURE_ERROR_CHARS)
        .collect::<String>();
    format!("{truncated}…")
}

#[cfg(test)]
pub(crate) fn write_failure_records_for_tests(
    repo_root: &Path,
    records: &[WebhookFailureRecord],
) -> Result<()> {
    let path = failure_store_path(repo_root);
    let _guard = failure_store_lock()
        .lock()
        .map_err(|_| anyhow!("failed to acquire webhook failure store lock"))?;
    write_failure_records_unlocked(&path, records)
}

#[cfg(test)]
pub(crate) fn load_failure_records_for_tests(
    repo_root: &Path,
) -> Result<Vec<WebhookFailureRecord>> {
    let path = failure_store_path(repo_root);
    load_failure_records(&path)
}

#[cfg(test)]
pub(crate) fn persist_failed_delivery_for_tests(
    repo_root: &Path,
    msg: &WebhookMessage,
    err: &anyhow::Error,
    attempts: u32,
) -> Result<()> {
    let path = failure_store_path(repo_root);
    persist_failed_delivery_at_path(&path, msg, err, attempts)
}

#[cfg(test)]
pub(crate) fn reset_webhook_metrics_for_tests() {
    let state = metrics();
    state.queue_depth.store(0, Ordering::SeqCst);
    state.queue_capacity.store(0, Ordering::SeqCst);
    state.enqueued_total.store(0, Ordering::SeqCst);
    state.delivered_total.store(0, Ordering::SeqCst);
    state.failed_total.store(0, Ordering::SeqCst);
    state.dropped_total.store(0, Ordering::SeqCst);
    state.retry_attempts_total.store(0, Ordering::SeqCst);
}
