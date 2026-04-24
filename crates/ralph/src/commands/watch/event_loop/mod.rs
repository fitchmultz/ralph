//! Watch event-loop facade.
//!
//! Purpose:
//! - Watch event-loop facade.
//!
//! Responsibilities:
//! - Run the watch loop until stopped or the event channel disconnects.
//! - Route filesystem events and timeout ticks to focused helpers.
//! - Keep the root event-loop module thin and testable.
//!
//! Not handled here:
//! - File watcher setup (see `watch/mod.rs`).
//! - Comment detection or task materialization.
//! - Event-specific debounce bookkeeping details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The loop exits cleanly on channel disconnect or when `running` becomes false.
//! - Mutex poison errors are logged and cause graceful exit or skipped work.
//! - Timeout ticks continue to service pending files even when no new events arrive.

use self::events::handle_watch_event;
use self::timeout::handle_timeout_tick;
use crate::commands::watch::state::WatchState;
use crate::commands::watch::types::WatchOptions;
use crate::config::Resolved;
use anyhow::Result;
use notify::Event;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod events;
mod timeout;

pub(crate) fn run_watch_loop(
    rx: &std::sync::mpsc::Receiver<notify::Result<Event>>,
    running: &Arc<Mutex<bool>>,
    state: &Arc<Mutex<WatchState>>,
    resolved: &Resolved,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
) -> Result<()> {
    loop {
        let should_continue = match running.lock() {
            Ok(guard) => *guard,
            Err(error) => {
                log::error!("Watch 'running' mutex poisoned, exiting: {}", error);
                break;
            }
        };
        if !should_continue {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => handle_watch_event(
                &event,
                state,
                resolved,
                comment_regex,
                opts,
                last_processed,
                Instant::now(),
            )?,
            Ok(Err(error)) => {
                log::warn!("Watch error: {}", error);
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::info!("Watch channel disconnected, shutting down...");
                break;
            }
            Err(RecvTimeoutError::Timeout) => handle_timeout_tick(
                state,
                resolved,
                comment_regex,
                opts,
                last_processed,
                Instant::now(),
            )?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
