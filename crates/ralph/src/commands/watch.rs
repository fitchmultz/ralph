//! Watch command implementation for file monitoring and task detection.
//!
//! Responsibilities:
//! - Set up file system watcher using the notify crate.
//! - Implement debouncing for file change events.
//! - Detect TODO/FIXME/HACK/XXX comments in changed files.
//! - Integrate with notification system for desktop alerts.
//! - Create or suggest tasks based on detected comments.
//! - Respect gitignore patterns for file exclusion.
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
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
        let mut r = running_for_signal.lock().unwrap();
        *r = false;
        // Trigger processing of any pending files
        let mut s = state_for_signal.lock().unwrap();
        s.last_event = Instant::now() - Duration::from_secs(1);
    })
    .context("Failed to set Ctrl+C handler")?;

    // Main event loop
    let comment_regex = build_comment_regex(&opts.comment_types)?;
    let mut processed_files: HashSet<PathBuf> = HashSet::new();

    while *running.lock().unwrap() {
        // Check for events with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if let Some(paths) = get_relevant_paths(&event, &opts) {
                    let mut should_process = false;
                    {
                        let mut state = state.lock().unwrap();
                        for path in paths {
                            if !processed_files.contains(&path) && state.add_file(path.clone()) {
                                should_process = true;
                            }
                        }
                    }
                    if should_process {
                        process_pending_files(
                            resolved,
                            &state,
                            &comment_regex,
                            &opts,
                            &mut processed_files,
                        )?;
                    }
                }
            }
            Ok(Err(e)) => {
                log::warn!("Watch error: {}", e);
            }
            Err(_) => {
                // Timeout - check if we should process pending files
                let should_process = {
                    let state = state.lock().unwrap();
                    !state.pending_files.is_empty()
                        && Instant::now().duration_since(state.last_event)
                            >= state.debounce_duration
                };
                if should_process {
                    process_pending_files(
                        resolved,
                        &state,
                        &comment_regex,
                        &opts,
                        &mut processed_files,
                    )?;
                }
            }
        }
    }

    // Process any remaining files before exit
    process_pending_files(
        resolved,
        &state,
        &comment_regex,
        &opts,
        &mut processed_files,
    )?;

    log::info!("Watch mode stopped.");
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

/// Simple pattern matching (supports * and ? wildcards).
fn matches_pattern(name: &str, pattern: &str) -> bool {
    // Simple glob matching - convert pattern to regex
    let regex_pattern = pattern
        .replace('.', r"\.")
        .replace('*', ".*")
        .replace('?', ".");

    if let Ok(re) = Regex::new(&format!("^{}$", regex_pattern)) {
        re.is_match(name)
    } else {
        name.contains(pattern.trim_matches('*'))
    }
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
    processed_files: &mut HashSet<PathBuf>,
) -> Result<()> {
    let files: Vec<PathBuf> = {
        let mut state = state.lock().unwrap();
        state.take_pending()
    };

    if files.is_empty() {
        return Ok(());
    }

    let mut all_comments: Vec<DetectedComment> = Vec::new();

    for file_path in files {
        if processed_files.contains(&file_path) {
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
                processed_files.insert(file_path);
            }
            Err(e) => {
                log::warn!("Failed to process file {}: {}", file_path.display(), e);
            }
        }
    }

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
}
