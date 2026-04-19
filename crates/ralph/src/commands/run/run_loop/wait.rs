//! Queue waiting and wakeup handling for the sequential run loop.
//!
//! Responsibilities:
//! - Watch queue/runtime files and wake the run loop when work becomes runnable.
//! - Translate wait outcomes into explicit state-machine exits.
//! - Emit notifications when a previously blocked queue becomes runnable.
//!
//! Not handled here:
//! - Session recovery or per-task execution.
//! - Parallel orchestration.
//!
//! Invariants/assumptions:
//! - Queue/done state is re-evaluated from disk on each wakeup.
//! - File watching is best-effort; deadline scheduling remains the fallback when watch setup fails.

use crate::config;
use crate::contracts::BlockingState;
use crate::{queue, runutil, signal};
use anyhow::Result;

#[derive(Debug)]
pub(super) enum WaitMode {
    BlockedOnly,
    EmptyAllowed,
}

pub(super) enum WaitExit {
    RunnableAvailable {
        summary: crate::queue::operations::QueueRunnabilitySummary,
    },
    QueueStillIdle {
        state: BlockingState,
    },
    TimedOut {
        state: BlockingState,
    },
    StopRequested {
        state: Option<BlockingState>,
    },
}

struct QueueFileWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
}

impl QueueFileWatcher {
    fn new(resolved: &config::Resolved) -> anyhow::Result<Self> {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )?;

        let ralph_dir = resolved.repo_root.join(".ralph");
        if ralph_dir.exists() {
            watcher.watch(&ralph_dir, RecursiveMode::Recursive)?;
        }

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    fn recv_timeout(&self, dur: std::time::Duration) -> Result<(), ()> {
        match self.rx.recv_timeout(dur) {
            Ok(_) => Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(()),
            Err(_) => Err(()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wait_for_work(
    resolved: &config::Resolved,
    include_draft: bool,
    mode: WaitMode,
    blocked_poll_ms: u64,
    empty_poll_ms: u64,
    timeout_seconds: u64,
    notify_when_unblocked: bool,
    loop_webhook_ctx: &crate::webhook::WebhookContext,
) -> Result<WaitExit> {
    use std::time::{Duration, Instant};

    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let blocked_poll_ms = blocked_poll_ms.max(50);
    let empty_poll_ms = empty_poll_ms.max(50);
    let start = Instant::now();
    let control_slice = Duration::from_millis(250);
    let ctrlc = crate::runner::ctrlc_state().ok();
    let watcher = QueueFileWatcher::new(resolved).ok();
    if watcher.is_none() {
        log::debug!("File watcher setup failed, using poll-only mode");
    }

    let poll_ms = match mode {
        WaitMode::BlockedOnly => blocked_poll_ms,
        WaitMode::EmptyAllowed => empty_poll_ms,
    };

    log::info!(
        "Waiting for runnable tasks (mode={:?}, poll={}ms, timeout={}s)...",
        mode,
        poll_ms,
        if timeout_seconds == 0 {
            "none".to_string()
        } else {
            timeout_seconds.to_string()
        }
    );

    let mut last_eval = Instant::now();
    let mut pending_event = true;
    let mut last_state: Option<BlockingState> = None;

    loop {
        if timeout_seconds != 0 && start.elapsed().as_secs() >= timeout_seconds {
            return Ok(WaitExit::TimedOut {
                state: last_state.clone().unwrap_or_else(|| {
                    BlockingState::idle(include_draft)
                        .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback())
                }),
            });
        }

        if signal::stop_signal_exists(&cache_dir) {
            if let Err(e) = signal::clear_stop_signal(&cache_dir) {
                log::warn!("Failed to clear stop signal: {}", e);
            }
            return Ok(WaitExit::StopRequested {
                state: last_state.clone(),
            });
        }

        if ctrlc
            .as_ref()
            .is_some_and(|c| c.interrupted.load(std::sync::atomic::Ordering::SeqCst))
        {
            return Err(runutil::RunAbort::new(
                runutil::RunAbortReason::Interrupted,
                "Ctrl+C pressed while waiting for runnable tasks",
            )
            .into());
        }

        let poll_dur = Duration::from_millis(poll_ms);
        let wait_slice = next_wait_slice(
            start,
            timeout_seconds,
            last_eval,
            poll_dur,
            pending_event,
            control_slice,
        );

        if let Some(ref watcher) = watcher {
            if watcher.recv_timeout(wait_slice).is_ok() {
                pending_event = true;
            }
        } else {
            std::thread::sleep(wait_slice);
        }

        if pending_event || last_eval.elapsed() >= poll_dur {
            pending_event = false;
            last_eval = Instant::now();

            let queue_file = match queue::load_queue(&resolved.queue_path) {
                Ok(q) => q,
                Err(e) => {
                    log::warn!("Failed to load queue while waiting: {}; will retry", e);
                    continue;
                }
            };

            let done = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done)
            };

            let options = queue::RunnableSelectionOptions::new(include_draft, true);
            let report = match crate::queue::operations::queue_runnability_report(
                &queue_file,
                done_ref,
                options,
            ) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!(
                        "Failed to generate runnability report while waiting: {}; will retry",
                        e
                    );
                    continue;
                }
            };
            last_state = report.summary.blocking.clone();

            if report.summary.candidates_total == 0 {
                match mode {
                    WaitMode::BlockedOnly => {
                        return Ok(WaitExit::QueueStillIdle {
                            state: last_state.clone().unwrap_or_else(|| {
                                BlockingState::idle(include_draft).with_observed_at(
                                    crate::timeutil::now_utc_rfc3339_or_fallback(),
                                )
                            }),
                        });
                    }
                    WaitMode::EmptyAllowed => continue,
                }
            }

            if report.summary.runnable_candidates > 0 {
                if notify_when_unblocked {
                    notify_queue_unblocked(&report.summary, resolved, loop_webhook_ctx);
                }
                return Ok(WaitExit::RunnableAvailable {
                    summary: report.summary,
                });
            }
        }
    }
}

fn next_wait_slice(
    start: std::time::Instant,
    timeout_seconds: u64,
    last_eval: std::time::Instant,
    poll_dur: std::time::Duration,
    pending_event: bool,
    control_slice: std::time::Duration,
) -> std::time::Duration {
    use std::time::Duration;

    if pending_event {
        return Duration::from_millis(1);
    }

    let now = std::time::Instant::now();
    let mut wait_for = (last_eval + poll_dur)
        .saturating_duration_since(now)
        .max(Duration::from_millis(1));

    if timeout_seconds != 0 {
        let timeout_deadline = start + Duration::from_secs(timeout_seconds);
        wait_for = wait_for.min(
            timeout_deadline
                .saturating_duration_since(now)
                .max(Duration::from_millis(1)),
        );
    }

    wait_for.min(control_slice).max(Duration::from_millis(1))
}

fn notify_queue_unblocked(
    summary: &crate::queue::operations::QueueRunnabilitySummary,
    resolved: &config::Resolved,
    loop_webhook_ctx: &crate::webhook::WebhookContext,
) {
    let note = format!(
        "ready={} blocked_deps={} blocked_schedule={}",
        summary.runnable_candidates, summary.blocked_by_dependencies, summary.blocked_by_schedule
    );

    let notify_config = crate::notification::NotificationConfig {
        enabled: true,
        notify_on_complete: false,
        notify_on_fail: false,
        notify_on_loop_complete: false,
        suppress_when_active: resolved
            .config
            .agent
            .notification
            .suppress_when_active
            .unwrap_or(true),
        sound_enabled: resolved
            .config
            .agent
            .notification
            .sound_enabled
            .unwrap_or(false),
        sound_path: resolved.config.agent.notification.sound_path.clone(),
        timeout_ms: resolved
            .config
            .agent
            .notification
            .timeout_ms
            .unwrap_or(8000),
    };

    #[cfg(feature = "notifications")]
    {
        use notify_rust::{Notification, Timeout};
        if let Err(e) = Notification::new()
            .summary("Ralph: tasks runnable")
            .body(&note)
            .timeout(Timeout::Milliseconds(notify_config.timeout_ms))
            .show()
        {
            log::debug!("Failed to show unblocked notification: {}", e);
        }

        if notify_config.sound_enabled
            && let Err(e) =
                crate::notification::play_completion_sound(notify_config.sound_path.as_deref())
        {
            log::debug!("Failed to play unblocked sound: {}", e);
        }
    }

    let timestamp = crate::timeutil::now_utc_rfc3339_or_fallback();
    let payload = crate::webhook::WebhookPayload {
        event: "queue_unblocked".to_string(),
        timestamp,
        task_id: None,
        task_title: None,
        previous_status: Some("blocked".to_string()),
        current_status: Some("runnable".to_string()),
        note: Some(note),
        context: loop_webhook_ctx.clone(),
    };
    crate::webhook::send_webhook_payload(payload, &resolved.config.agent.webhook);
}
