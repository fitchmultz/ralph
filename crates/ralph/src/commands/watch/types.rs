//! Types for the watch command.
//!
//! Responsibilities:
//! - Define `CommentType` enum for TODO/FIXME/HACK/XXX comment detection.
//! - Define `DetectedComment` struct for representing found comments.
//! - Define `WatchOptions` struct for watch command configuration.
//!
//! Not handled here:
//! - File watching logic (see `event_loop.rs`).
//! - Comment detection regex building (see `comments.rs`).
//! - Task creation from comments (see `tasks.rs`).
//!
//! Invariants/assumptions:
//! - `CommentType::All` is a wildcard that matches all comment types.
//! - `WatchOptions` is constructed by CLI parsing and passed to `run_watch`.

use std::path::PathBuf;

/// Types of comments to detect in watched files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentType {
    Todo,
    Fixme,
    Hack,
    Xxx,
    All,
}

/// A detected comment in a source file.
#[derive(Debug, Clone)]
pub struct DetectedComment {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub comment_type: CommentType,
    pub content: String,
    pub context: String,
}

/// Options for the watch command.
#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub patterns: Vec<String>,
    pub debounce_ms: u64,
    pub auto_queue: bool,
    pub notify: bool,
    pub ignore_patterns: Vec<String>,
    pub comment_types: Vec<CommentType>,
    pub paths: Vec<PathBuf>,
    pub force: bool,
}
