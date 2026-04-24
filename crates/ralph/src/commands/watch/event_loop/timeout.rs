//! Timeout-tick handling for the watch loop.
//!
//! Purpose:
//! - Timeout-tick handling for the watch loop.
//!
//! Responsibilities:
//! - Decide when pending files have aged past the debounce window.
//! - Trigger pending-file processing during idle receive timeouts.
//!
//! Not handled here:
//! - Immediate filesystem-event ingestion.
//! - File watcher setup.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Timeout ticks are the fallback path when no new notify events arrive.
//! - Mutex poison on watch state is logged and treated as a no-op tick.

use crate::commands::watch::processor::process_pending_files;
use crate::commands::watch::state::WatchState;
use crate::commands::watch::types::WatchOptions;
use crate::config::Resolved;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn pending_files_ready_at(state: &WatchState, now: Instant) -> bool {
    !state.pending_files.is_empty()
        && now.duration_since(state.last_event) >= state.debounce_duration
}

pub(super) fn handle_timeout_tick(
    state: &Arc<Mutex<WatchState>>,
    resolved: &Resolved,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
    now: Instant,
) -> Result<()> {
    let should_process = match state.lock() {
        Ok(guard) => pending_files_ready_at(&guard, now),
        Err(error) => {
            log::error!(
                "Watch 'state' mutex poisoned during timeout check: {}",
                error
            );
            false
        }
    };

    if should_process {
        process_pending_files(resolved, state, comment_regex, opts, last_processed)?;
    }

    Ok(())
}
