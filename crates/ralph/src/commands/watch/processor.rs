//! File processing for the watch command.
//!
//! Responsibilities:
//! - Process pending files and detect comments.
//! - Coordinate between debounce logic, comment detection, and task handling.
//! - Scope removal reconciliation to files processed in the current batch.
//!
//! Not handled here:
//! - File watching or event handling (see `event_loop/mod.rs`).
//! - Low-level comment detection (see `comments.rs`).
//! - Task creation or identity rules (see `tasks.rs` / `identity.rs`).
//!
//! Invariants/assumptions:
//! - Files are skipped if recently processed (within debounce window).
//! - Missing files are treated as zero-comment scans for reconciliation.
//! - Generic read failures do not trigger removal reconciliation.
//! - Old entries in `last_processed` are cleaned up periodically.

use crate::commands::watch::comments::detect_comments;
use crate::commands::watch::debounce::{can_reprocess, cleanup_old_entries};
use crate::commands::watch::state::WatchState;
use crate::commands::watch::tasks::handle_detected_comments;
use crate::commands::watch::types::{DetectedComment, WatchOptions};
use crate::config::Resolved;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Process pending files and detect comments.
pub fn process_pending_files(
    resolved: &Resolved,
    state: &Arc<Mutex<WatchState>>,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
) -> Result<()> {
    let files: Vec<PathBuf> = match state.lock() {
        Ok(mut guard) => guard.take_pending(),
        Err(e) => {
            log::error!("Watch 'state' mutex poisoned, cannot process files: {}", e);
            return Ok(());
        }
    };

    if files.is_empty() {
        return Ok(());
    }

    let debounce = Duration::from_millis(opts.debounce_ms);
    let mut all_comments: Vec<DetectedComment> = Vec::new();
    let mut processed_files: Vec<PathBuf> = Vec::new();

    for file_path in files {
        // Skip if file was recently processed (within debounce window)
        if !can_reprocess(&file_path, last_processed, debounce) {
            continue;
        }

        match detect_comments(&file_path, comment_regex) {
            Ok(comments) => {
                processed_files.push(file_path.clone());
                if !comments.is_empty() {
                    log::debug!(
                        "Detected {} comments in {}",
                        comments.len(),
                        file_path.display()
                    );
                    all_comments.extend(comments);
                }
                // Record when this file was processed
                last_processed.insert(file_path, Instant::now());
            }
            Err(e) if !file_path.exists() => {
                log::debug!(
                    "Treating missing file {} as removed for watch reconciliation: {}",
                    file_path.display(),
                    e
                );
                processed_files.push(file_path.clone());
                last_processed.insert(file_path, Instant::now());
            }
            Err(e) => {
                log::warn!("Failed to process file {}: {}", file_path.display(), e);
            }
        }
    }

    // Periodically clean up old entries to prevent unbounded growth
    cleanup_old_entries(last_processed, debounce);

    if !processed_files.is_empty() {
        handle_detected_comments(resolved, &all_comments, &processed_files, opts)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
