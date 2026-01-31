//! Watch command implementation for file monitoring and task detection.
//!
//! Responsibilities:
//! - Set up file system watcher using the notify crate.
//! - Implement debouncing for file change events.
//! - Detect TODO/FIXME/HACK/XXX comments in changed files.
//! - Integrate with notification system for desktop alerts.
//! - Create or suggest tasks based on detected comments.
//! - Respect gitignore patterns for file exclusion.
//! - Allow reprocessing of files after the debounce window has passed.
//!
//! Not handled here:
//! - CLI argument parsing (see crate::cli::watch).
//! - Runner execution (watch is local-only).
//! - Queue persistence details (see crate::queue).
//!
//! Invariants/assumptions:
//! - File watcher uses debouncing to batch rapid file changes.
//! - Comment detection uses regex patterns for common markers.
//! - Task deduplication prevents duplicate entries for same file/line.
//! - Files can be reprocessed after `debounce_duration` has elapsed since last processing.

use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::notification::{notify_watch_new_task, NotificationConfig};
use crate::queue::{load_queue, save_queue, suggest_new_task_insert_index};
use crate::timeutil;
use anyhow::{Context, Result};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Check if a file can be reprocessed based on when it was last processed.
///
/// A file can be reprocessed if:
/// - It has never been processed before, OR
/// - The time since last processing is >= the debounce duration
fn can_reprocess(
    path: &Path,
    last_processed: &HashMap<PathBuf, Instant>,
    debounce: Duration,
) -> bool {
    match last_processed.get(path) {
        Some(last_time) => Instant::now().duration_since(*last_time) >= debounce,
        None => true,
    }
}

/// Clean up old entries from the last_processed map to prevent unbounded growth.
///
/// Removes entries older than 10x the debounce duration.
fn cleanup_old_entries(last_processed: &mut HashMap<PathBuf, Instant>, debounce: Duration) {
    let cutoff = Instant::now() - debounce * 10;
    last_processed.retain(|_, timestamp| *timestamp >= cutoff);
}

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

/// Internal state for the file watcher.
struct WatchState {
    pending_files: HashSet<PathBuf>,
    last_event: Instant,
    debounce_duration: Duration,
}

impl WatchState {
    fn new(debounce_ms: u64) -> Self {
        Self {
            pending_files: HashSet::new(),
            last_event: Instant::now(),
            debounce_duration: Duration::from_millis(debounce_ms),
        }
    }

    fn add_file(&mut self, path: PathBuf) -> bool {
        self.pending_files.insert(path);
        let now = Instant::now();
        if now.duration_since(self.last_event) >= self.debounce_duration {
            self.last_event = now;
            true
        } else {
            false
        }
    }

    fn take_pending(&mut self) -> Vec<PathBuf> {
        let files: Vec<PathBuf> = self.pending_files.drain().collect();
        self.last_event = Instant::now();
        files
    }
}

/// Run the watch command with the given options.
pub fn run_watch(resolved: &Resolved, opts: WatchOptions) -> Result<()> {
    log::info!("Starting watch mode on {} path(s)...", opts.paths.len());
    log::info!("Patterns: {:?}", opts.patterns);
    log::info!("Debounce: {}ms", opts.debounce_ms);
    log::info!("Auto-queue: {}", opts.auto_queue);

    // Set up channel for file events
    let (tx, rx) = channel::<notify::Result<Event>>();

    // Create watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .context("Failed to create file watcher")?;

    // Watch specified paths
    for path in &opts.paths {
        let mode = if path.is_file() {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        watcher
            .watch(path, mode)
            .with_context(|| format!("Failed to watch path: {}", path.display()))?;
    }

    log::info!("Watch mode active. Press Ctrl+C to stop.");

    // Set up watch state with debouncing
    let state = Arc::new(Mutex::new(WatchState::new(opts.debounce_ms)));
    let state_for_signal = state.clone();

    // Set up Ctrl+C handler
    let running = Arc::new(Mutex::new(true));
    let running_for_signal = running.clone();

    ctrlc::set_handler(move || {
        log::info!("Received interrupt signal, shutting down...");
        match running_for_signal.lock() {
            Ok(mut r) => *r = false,
            Err(e) => {
                log::error!("Watch 'running' mutex poisoned in signal handler: {}", e);
                // Cannot recover; exit will happen via main loop detection
            }
        }
        // Trigger processing of any pending files
        match state_for_signal.lock() {
            Ok(mut s) => s.last_event = Instant::now() - Duration::from_secs(1),
            Err(e) => {
                log::error!("Watch 'state' mutex poisoned in signal handler: {}", e);
            }
        }
    })
    .context("Failed to set Ctrl+C handler")?;

    // Main event loop
    let comment_regex = build_comment_regex(&opts.comment_types)?;
    let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

    run_watch_loop(
        &rx,
        &running,
        &state,
        resolved,
        &comment_regex,
        &opts,
        &mut last_processed,
    )?;

    // Process any remaining files before exit
    process_pending_files(resolved, &state, &comment_regex, &opts, &mut last_processed)?;

    log::info!("Watch mode stopped.");
    Ok(())
}

/// Run the watch event loop until stopped or channel disconnects.
///
/// This is extracted as a separate function for testability.
fn run_watch_loop(
    rx: &std::sync::mpsc::Receiver<notify::Result<Event>>,
    running: &Arc<Mutex<bool>>,
    state: &Arc<Mutex<WatchState>>,
    resolved: &Resolved,
    comment_regex: &Regex,
    opts: &WatchOptions,
    last_processed: &mut HashMap<PathBuf, Instant>,
) -> Result<()> {
    loop {
        // Check running state with poison handling
        let should_continue = match running.lock() {
            Ok(guard) => *guard,
            Err(e) => {
                log::error!("Watch 'running' mutex poisoned, exiting: {}", e);
                break;
            }
        };
        if !should_continue {
            break;
        }
        // Check for events with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if let Some(paths) = get_relevant_paths(&event, opts) {
                    let debounce = opts.debounce_ms;
                    let mut should_process = false;
                    match state.lock() {
                        Ok(mut guard) => {
                            for path in paths {
                                if can_reprocess(
                                    &path,
                                    last_processed,
                                    Duration::from_millis(debounce),
                                ) && guard.add_file(path.clone())
                                {
                                    should_process = true;
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Watch 'state' mutex poisoned, skipping event: {}", e);
                            continue;
                        }
                    }
                    if should_process {
                        process_pending_files(
                            resolved,
                            state,
                            comment_regex,
                            opts,
                            last_processed,
                        )?;
                    }
                }
            }
            Ok(Err(e)) => {
                log::warn!("Watch error: {}", e);
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::info!("Watch channel disconnected, shutting down...");
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // Timeout - check if we should process pending files
                let should_process = match state.lock() {
                    Ok(state) => {
                        !state.pending_files.is_empty()
                            && Instant::now().duration_since(state.last_event)
                                >= state.debounce_duration
                    }
                    Err(e) => {
                        log::error!("Watch 'state' mutex poisoned during timeout check: {}", e);
                        false
                    }
                };
                if should_process {
                    process_pending_files(resolved, state, comment_regex, opts, last_processed)?;
                }
            }
        }
    }
    Ok(())
}

/// Get relevant file paths from a watch event.
fn get_relevant_paths(event: &Event, opts: &WatchOptions) -> Option<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = event
        .paths
        .iter()
        .filter(|p| should_process_file(p, &opts.patterns, &opts.ignore_patterns))
        .cloned()
        .collect();

    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

/// Check if a file should be processed based on patterns and ignore rules.
fn should_process_file(path: &Path, patterns: &[String], ignore_patterns: &[String]) -> bool {
    // Skip directories
    if path.is_dir() {
        return false;
    }

    // Check if file matches any pattern
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Check ignore patterns first
    for ignore in ignore_patterns {
        if matches_pattern(file_name, ignore) {
            return false;
        }
    }

    // Check if in common ignore directories
    let path_str = path.to_string_lossy();
    let ignore_dirs = [
        "/target/",
        "/node_modules/",
        "/.git/",
        "/vendor/",
        "/.ralph/",
    ];
    for dir in &ignore_dirs {
        if path_str.contains(dir) {
            return false;
        }
    }

    // Check if file matches any pattern
    patterns.iter().any(|p| matches_pattern(file_name, p))
}

/// Match a filename against a glob pattern using globset.
///
/// Supports standard glob syntax:
/// - `*` matches any sequence of characters (except `/`)
/// - `?` matches any single character
/// - `[abc]` matches any character in the set
/// - `[a-z]` matches any character in the range
fn matches_pattern(name: &str, pattern: &str) -> bool {
    globset::Glob::new(pattern)
        .map(|g| g.compile_matcher().is_match(name))
        .unwrap_or(false)
}

/// Build regex for detecting comments based on comment types.
fn build_comment_regex(comment_types: &[CommentType]) -> Result<Regex> {
    let mut patterns = Vec::new();

    let has_all = comment_types.contains(&CommentType::All);

    if has_all || comment_types.contains(&CommentType::Todo) {
        patterns.push(r"TODO\s*[:;-]?\s*(.+)$");
    }
    if has_all || comment_types.contains(&CommentType::Fixme) {
        patterns.push(r"FIXME\s*[:;-]?\s*(.+)$");
    }
    if has_all || comment_types.contains(&CommentType::Hack) {
        patterns.push(r"HACK\s*[:;-]?\s*(.+)$");
    }
    if has_all || comment_types.contains(&CommentType::Xxx) {
        patterns.push(r"XXX\s*[:;-]?\s*(.+)$");
    }

    if patterns.is_empty() {
        patterns.push(r"(?:TODO|FIXME|HACK|XXX)\s*[:;-]?\s*(.+)$");
    }

    let combined = patterns.join("|");
    let regex = Regex::new(&format!(r"(?i)({})", combined))
        .context("Failed to compile comment detection regex")?;

    Ok(regex)
}

/// Process pending files and detect comments.
fn process_pending_files(
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

    for file_path in files {
        // Skip if file was recently processed (within debounce window)
        if !can_reprocess(&file_path, last_processed, debounce) {
            continue;
        }

        match detect_comments(&file_path, comment_regex) {
            Ok(comments) => {
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
            Err(e) => {
                log::warn!("Failed to process file {}: {}", file_path.display(), e);
            }
        }
    }

    // Periodically clean up old entries to prevent unbounded growth
    cleanup_old_entries(last_processed, debounce);

    if !all_comments.is_empty() {
        handle_detected_comments(resolved, &all_comments, opts)?;
    }

    Ok(())
}

/// Detect comments in a file.
fn detect_comments(file_path: &Path, regex: &Regex) -> Result<Vec<DetectedComment>> {
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let mut comments = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(captures) = regex.captures(line) {
            // Extract the comment content
            let content = captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
                .or_else(|| captures.get(4))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            if content.is_empty() {
                continue;
            }

            // Determine comment type from the match
            let comment_type = determine_comment_type(line);

            // Get context (surrounding lines)
            let context = extract_context(&content, line_num + 1, file_path);

            comments.push(DetectedComment {
                file_path: file_path.to_path_buf(),
                line_number: line_num + 1,
                comment_type,
                content,
                context,
            });
        }
    }

    Ok(comments)
}

/// Determine the comment type from a line.
fn determine_comment_type(line: &str) -> CommentType {
    let upper = line.to_uppercase();
    if upper.contains("TODO") {
        CommentType::Todo
    } else if upper.contains("FIXME") {
        CommentType::Fixme
    } else if upper.contains("HACK") {
        CommentType::Hack
    } else if upper.contains("XXX") {
        CommentType::Xxx
    } else {
        CommentType::All
    }
}

/// Extract context for a comment.
fn extract_context(content: &str, line_number: usize, file_path: &Path) -> String {
    format!(
        "{}:{} - {}",
        file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown"),
        line_number,
        content.chars().take(100).collect::<String>()
    )
}

/// Handle detected comments by creating tasks or suggesting them.
fn handle_detected_comments(
    resolved: &Resolved,
    comments: &[DetectedComment],
    opts: &WatchOptions,
) -> Result<()> {
    // Load current queue
    let mut queue = load_queue(&resolved.queue_path)?;

    // Track which tasks were created
    let mut created_tasks: Vec<(String, String)> = Vec::new();

    for comment in comments {
        // Check if a similar task already exists
        if task_exists_for_comment(&queue, comment) {
            log::debug!(
                "Skipping duplicate task for {}:{}",
                comment.file_path.display(),
                comment.line_number
            );
            continue;
        }

        let task = create_task_from_comment(comment, resolved)?;

        if opts.auto_queue {
            // Add task to queue
            let insert_at = suggest_new_task_insert_index(&queue);
            queue.tasks.insert(insert_at, task.clone());
            created_tasks.push((task.id.clone(), task.title.clone()));
            log::info!("Created task {}: {}", task.id, task.title);
        } else {
            // Just log the suggestion
            let type_str = format!("{:?}", comment.comment_type).to_uppercase();
            log::info!(
                "[SUGGESTION] {} at {}:{}",
                type_str,
                comment.file_path.display(),
                comment.line_number
            );
            log::info!("  Content: {}", comment.content);
            log::info!("  Suggested task: {}", task.title);
        }
    }

    // Save queue if tasks were created
    if opts.auto_queue && !created_tasks.is_empty() {
        save_queue(&resolved.queue_path, &queue)?;
        log::info!("Added {} task(s) to queue", created_tasks.len());

        // Send notification if enabled
        if opts.notify {
            let config = NotificationConfig::new();
            notify_watch_new_task(created_tasks.len(), &config);
        }
    }

    Ok(())
}

/// Check if a task already exists for a given comment.
fn task_exists_for_comment(queue: &QueueFile, comment: &DetectedComment) -> bool {
    let file_str = comment.file_path.to_string_lossy().to_string();

    queue.tasks.iter().any(|task| {
        // Check if task title or notes reference this file and line
        let title_match = task.title.contains(&file_str)
            || task
                .title
                .contains(&format!("line {}", comment.line_number));

        let notes_match = task.notes.iter().any(|note| {
            note.contains(&file_str) && note.contains(&format!("{}", comment.line_number))
        });

        title_match || notes_match
    })
}

/// Create a task from a detected comment.
fn create_task_from_comment(comment: &DetectedComment, resolved: &Resolved) -> Result<Task> {
    let type_str = format!("{:?}", comment.comment_type).to_uppercase();
    let file_name = comment
        .file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let title = format!(
        "{}: {} in {}",
        type_str,
        comment.content.chars().take(50).collect::<String>(),
        file_name
    );

    let now = timeutil::now_utc_rfc3339_or_fallback();

    // Generate a unique task ID
    let task_id = generate_task_id(&resolved.id_prefix, resolved.id_width, &resolved.queue_path)?;

    let notes = vec![
        format!(
            "Detected in: {}:{}",
            comment.file_path.display(),
            comment.line_number
        ),
        format!("Full content: {}", comment.content),
        format!("Context: {}", comment.context),
    ];

    let tags = vec![
        "watch".to_string(),
        format!("{:?}", comment.comment_type).to_lowercase(),
    ];

    Ok(Task {
        id: task_id,
        status: TaskStatus::Todo,
        title,
        priority: TaskPriority::Medium,
        tags,
        scope: vec![comment.file_path.to_string_lossy().to_string()],
        evidence: Vec::new(),
        plan: Vec::new(),
        notes,
        request: Some(format!("Address {} comment", type_str)),
        agent: None,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        completed_at: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
    })
}

/// Generate a unique task ID.
fn generate_task_id(prefix: &str, width: usize, queue_path: &Path) -> Result<String> {
    let queue = load_queue_or_default(queue_path)?;

    // Find the highest existing ID number
    let mut max_num = 0;
    for task in &queue.tasks {
        if let Some(num_str) = task.id.strip_prefix(prefix) {
            if let Ok(num) = num_str.parse::<u32>() {
                if num > max_num {
                    max_num = num;
                }
            }
        }
    }

    // Generate next ID
    let next_num = max_num + 1;
    Ok(format!("{}{:0width$}", prefix, next_num, width = width))
}

fn load_queue_or_default(path: &Path) -> Result<QueueFile> {
    if !path.exists() {
        return Ok(QueueFile::default());
    }
    load_queue(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pattern_basic() {
        assert!(matches_pattern("test.rs", "*.rs"));
        assert!(matches_pattern("test.rs", "test.*"));
        assert!(!matches_pattern("test.py", "*.rs"));
    }

    #[test]
    fn matches_pattern_question() {
        assert!(matches_pattern("test.rs", "t??t.rs"));
        assert!(!matches_pattern("test.rs", "t?t.rs"));
    }

    #[test]
    fn matches_pattern_regex_metacharacters() {
        // Character class patterns - these would break with the old regex-based implementation
        // Note: *.[rs] matches files ending in .r or .s (single char), not .rs
        assert!(matches_pattern("test.r", "*.[rs]"));
        assert!(matches_pattern("test.s", "*.[rs]"));
        assert!(!matches_pattern("test.rs", "*.[rs]"));
        assert!(!matches_pattern("test.py", "*.[rs]"));

        // Plus sign in filename - + is literal in glob, not a regex quantifier
        assert!(matches_pattern("file+1.txt", "file+*.txt"));
        assert!(matches_pattern("file+123.txt", "file+*.txt"));

        // Parentheses in filename - () are literal in glob, not regex groups
        assert!(matches_pattern("test(1).rs", "test(*).rs"));
        assert!(matches_pattern("test(backup).rs", "test(*).rs"));

        // Dollar signs in filename - $ is literal in glob, not regex anchor
        assert!(matches_pattern("test.$$$", "test.*"));
        assert!(matches_pattern("file.$$$.txt", "file.*.txt"));

        // Caret in filename - ^ is literal in glob, not regex anchor
        assert!(matches_pattern("file^name.txt", "file^name.txt"));
        assert!(matches_pattern("file^name.txt", "file*.txt"));
    }

    #[test]
    fn matches_pattern_character_classes() {
        // Range patterns
        assert!(matches_pattern("file1.txt", "file[0-9].txt"));
        assert!(matches_pattern("file5.txt", "file[0-9].txt"));
        assert!(matches_pattern("file9.txt", "file[0-9].txt"));
        assert!(!matches_pattern("filea.txt", "file[0-9].txt"));

        // Multiple character classes
        assert!(matches_pattern("test_a.rs", "test_[a-z].rs"));
        assert!(matches_pattern("test_z.rs", "test_[a-z].rs"));
        assert!(!matches_pattern("test_1.rs", "test_[a-z].rs"));
    }

    #[test]
    fn matches_pattern_edge_cases() {
        // Empty pattern should only match empty string
        assert!(matches_pattern("", ""));
        assert!(!matches_pattern("test.rs", ""));

        // Invalid glob patterns should return false (not panic)
        // Unclosed character class is invalid in globset
        assert!(!matches_pattern("test.rs", "*.[rs"));

        // Just wildcards
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("a", "?"));
        assert!(!matches_pattern("ab", "?"));
    }

    #[test]
    fn determine_comment_type_detection() {
        assert_eq!(
            determine_comment_type("// TODO: fix this"),
            CommentType::Todo
        );
        assert_eq!(
            determine_comment_type("// FIXME: broken"),
            CommentType::Fixme
        );
        assert_eq!(
            determine_comment_type("// HACK: workaround"),
            CommentType::Hack
        );
        assert_eq!(
            determine_comment_type("// XXX: review needed"),
            CommentType::Xxx
        );
    }

    #[test]
    fn build_comment_regex_compiles() {
        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();
        assert!(regex.is_match("// TODO: fix this"));
        assert!(!regex.is_match("// FIXME: fix this"));

        let regex_all = build_comment_regex(&[CommentType::All]).unwrap();
        assert!(regex_all.is_match("// TODO: fix"));
        assert!(regex_all.is_match("// FIXME: fix"));
        assert!(regex_all.is_match("// HACK: workaround"));
        assert!(regex_all.is_match("// XXX: review"));
    }

    #[test]
    fn extract_context_format() {
        let ctx = extract_context("test content", 42, Path::new("/path/to/file.rs"));
        assert!(ctx.contains("file.rs"));
        assert!(ctx.contains("42"));
        assert!(ctx.contains("test content"));
    }

    #[test]
    fn can_reprocess_new_file() {
        let last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path = Path::new("/test/file.rs");

        // New file should be reprocessable
        assert!(can_reprocess(
            path,
            &last_processed,
            Duration::from_millis(100)
        ));
    }

    #[test]
    fn can_reprocess_after_debounce() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path = PathBuf::from("/test/file.rs");

        // Insert a timestamp from the past (older than debounce)
        last_processed.insert(path.clone(), Instant::now() - Duration::from_millis(200));

        // Should be reprocessable after debounce period
        assert!(can_reprocess(
            &path,
            &last_processed,
            Duration::from_millis(100)
        ));
    }

    #[test]
    fn cannot_reprocess_within_debounce() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path = PathBuf::from("/test/file.rs");

        // Insert current timestamp
        last_processed.insert(path.clone(), Instant::now());

        // Should NOT be reprocessable within debounce period
        assert!(!can_reprocess(
            &path,
            &last_processed,
            Duration::from_millis(100)
        ));
    }

    #[test]
    fn cleanup_old_entries_removes_stale_entries() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let old_path = PathBuf::from("/test/old.rs");
        let recent_path = PathBuf::from("/test/recent.rs");

        // Insert an old entry (older than 10x debounce)
        last_processed.insert(
            old_path.clone(),
            Instant::now() - Duration::from_millis(1500),
        );
        // Insert a recent entry
        last_processed.insert(
            recent_path.clone(),
            Instant::now() - Duration::from_millis(50),
        );

        let debounce = Duration::from_millis(100);
        cleanup_old_entries(&mut last_processed, debounce);

        // Old entry should be removed
        assert!(!last_processed.contains_key(&old_path));
        // Recent entry should remain
        assert!(last_processed.contains_key(&recent_path));
    }

    #[test]
    fn cleanup_old_entries_preserves_recent_entries() {
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();
        let path1 = PathBuf::from("/test/file1.rs");
        let path2 = PathBuf::from("/test/file2.rs");

        // Insert entries within the cleanup window
        last_processed.insert(path1.clone(), Instant::now() - Duration::from_millis(500));
        last_processed.insert(path2.clone(), Instant::now() - Duration::from_millis(300));

        let debounce = Duration::from_millis(100);
        cleanup_old_entries(&mut last_processed, debounce);

        // Both entries should remain (both within 10x debounce = 1000ms)
        assert!(last_processed.contains_key(&path1));
        assert!(last_processed.contains_key(&path2));
    }

    #[test]
    fn watch_loop_exits_on_channel_disconnect() {
        use std::sync::mpsc::channel;

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Create minimal Resolved for testing
        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: PathBuf::from("."),
            queue_path: PathBuf::from(".ralph/queue.json"),
            done_path: PathBuf::from(".ralph/done.json"),
            id_prefix: "RQ-".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        // Spawn the watch loop in a separate thread
        let running_clone = running.clone();
        let state_clone = state.clone();
        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
            .unwrap();
        });

        // Give the loop a moment to start
        std::thread::sleep(Duration::from_millis(50));

        // Drop the sender to simulate channel disconnect
        drop(tx);

        // The loop should exit within a reasonable time (timeout to prevent hanging)
        let result = handle.join();
        assert!(
            result.is_ok(),
            "Watch loop should exit cleanly on channel disconnect"
        );
    }

    #[test]
    fn watch_loop_exits_on_running_mutex_poison() {
        use std::sync::mpsc::channel;

        let (tx, rx) = channel::<notify::Result<Event>>();
        let running = Arc::new(Mutex::new(true));
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Create minimal Resolved for testing
        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: PathBuf::from("."),
            queue_path: PathBuf::from(".ralph/queue.json"),
            done_path: PathBuf::from(".ralph/done.json"),
            id_prefix: "RQ-".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        // Clone for the poisoning thread
        let running_clone = running.clone();

        // Spawn a thread that will panic while holding the running mutex
        let poison_handle = std::thread::spawn(move || {
            let _guard = running_clone.lock().unwrap();
            panic!("Intentional panic to poison running mutex");
        });

        // Wait for the panic
        let _ = poison_handle.join();

        // Now the running mutex is poisoned - verify the watch loop handles it gracefully
        let running_clone2 = running.clone();
        let state_clone = state.clone();
        let handle = std::thread::spawn(move || {
            run_watch_loop(
                &rx,
                &running_clone2,
                &state_clone,
                &resolved,
                &comment_regex,
                &opts,
                &mut last_processed,
            )
        });

        // Give the loop a moment to start and hit the poisoned mutex
        std::thread::sleep(Duration::from_millis(100));

        // Drop the sender to ensure clean exit
        drop(tx);

        // The loop should exit cleanly (not panic)
        let result = handle.join();
        assert!(
            result.is_ok(),
            "Watch loop should exit cleanly on running mutex poison"
        );
    }

    #[test]
    fn process_pending_files_handles_state_mutex_poison() {
        let state = Arc::new(Mutex::new(WatchState::new(100)));

        // Create minimal Resolved for testing
        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: PathBuf::from("."),
            queue_path: PathBuf::from(".ralph/queue.json"),
            done_path: PathBuf::from(".ralph/done.json"),
            id_prefix: "RQ-".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
        };

        let comment_regex = build_comment_regex(&opts.comment_types).unwrap();
        let mut last_processed: HashMap<PathBuf, Instant> = HashMap::new();

        // Clone for the poisoning thread
        let state_clone = state.clone();

        // Spawn a thread that will panic while holding the state mutex
        let poison_handle = std::thread::spawn(move || {
            let _guard = state_clone.lock().unwrap();
            panic!("Intentional panic to poison state mutex");
        });

        // Wait for the panic
        let _ = poison_handle.join();

        // Now the state mutex is poisoned - verify process_pending_files handles it gracefully
        let result = process_pending_files(
            &resolved,
            &state,
            &comment_regex,
            &opts,
            &mut last_processed,
        );

        // Should return Ok, not panic
        assert!(
            result.is_ok(),
            "process_pending_files should handle state mutex poison gracefully"
        );
    }
}
