//! Filesystem-event handling for the watch loop.
//!
//! Purpose:
//! - Filesystem-event handling for the watch loop.
//!
//! Responsibilities:
//! - Filter incoming notify events down to relevant watched paths.
//! - Update pending-file state and debounce bookkeeping.
//! - Trigger processing immediately when debounce rules allow.
//!
//! Not handled here:
//! - Timeout-based pending-file draining.
//! - File watcher setup.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Relevant paths are already scoped by watch options.
//! - Mutex poison on watch state is logged and treated as a skipped event.

use crate::commands::watch::debounce::can_reprocess_at;
use crate::commands::watch::paths::get_relevant_paths;
use crate::commands::watch::processor::process_pending_files;
use crate::commands::watch::state::WatchState;
use crate::commands::watch::types::WatchOptions;
use crate::config::Resolved;
use anyhow::Result;
use notify::Event;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) fn handle_watch_event(
    event: &Event,
    state: &Arc<Mutex<WatchState>>,
    resolved: &Resolved,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
    now: Instant,
) -> Result<()> {
    if let Some(paths) = get_relevant_paths(event, opts) {
        let debounce = Duration::from_millis(opts.debounce_ms);
        let mut should_process = false;
        match state.lock() {
            Ok(mut guard) => {
                for path in paths {
                    if can_reprocess_at(&path, last_processed, debounce, now)
                        && guard.add_file_at(path.clone(), now)
                    {
                        should_process = true;
                    }
                }
            }
            Err(error) => {
                log::error!("Watch 'state' mutex poisoned, skipping event: {}", error);
                return Ok(());
            }
        }

        if should_process {
            process_pending_files(resolved, state, comment_regex, opts, last_processed)?;
        }
    }

    Ok(())
}
