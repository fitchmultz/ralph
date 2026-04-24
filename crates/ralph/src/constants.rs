//! Centralized constants for the Ralph CLI.
//!
//! Purpose:
//! - Centralized constants for the Ralph CLI.
//!
//! This module consolidates all magic numbers, limits, and default values
//! to improve maintainability and prevent drift between duplicated values.
//!
//! Responsibilities:
//! - Provide a single source of truth for compile-time constants.
//! - Organize constants by domain (buffers, limits, timeouts, UI, etc.).
//! - Prevent accidental drift between duplicated constant definitions.
//!
//! Not handled here:
//! - Runtime configuration values (see `crate::config`).
//! - User-customizable thresholds (see `crate::contracts::Config`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All constants are `pub` within their submodule; visibility is controlled by module exports.
//! - Constants that appear in multiple places must be defined here and imported elsewhere.

/// Buffer size limits for output handling and memory management.
pub mod buffers {
    /// Maximum size for ANSI buffer (10MB).
    /// Reduced from 100MB to prevent memory pressure during runner execution.
    pub const MAX_ANSI_BUFFER_SIZE: usize = 10 * 1024 * 1024;

    /// Maximum line length before truncation (10MB).
    /// Reduced from 100MB to prevent memory pressure with runaway runner output.
    pub const MAX_LINE_LENGTH: usize = 10 * 1024 * 1024;

    /// Maximum buffer size for stream processing (10MB).
    /// Reduced from 100MB to prevent memory pressure during long-running tasks.
    pub const MAX_BUFFER_SIZE: usize = 10 * 1024 * 1024;

    /// Maximum log lines to retain for interactive log viewers.
    pub const MAX_LOG_LINES: usize = 10_000;

    /// Maximum tool value length before truncation.
    pub const TOOL_VALUE_MAX_LEN: usize = 160;

    /// Maximum instruction file size (128KB).
    pub const MAX_INSTRUCTION_BYTES: usize = 128 * 1024;

    /// Maximum stdout capture size for timeouts (128KB).
    pub const TIMEOUT_STDOUT_CAPTURE_MAX_BYTES: usize = 128 * 1024;

    /// Maximum bounded capture size for managed subprocess stdout/stderr tails.
    pub const MANAGED_SUBPROCESS_CAPTURE_MAX_BYTES: usize = 256 * 1024;

    /// Maximum bounded capture size for CI gate stdout/stderr tails.
    pub const MANAGED_SUBPROCESS_CI_CAPTURE_MAX_BYTES: usize = 4 * 1024 * 1024;

    /// Number of output tail lines to display.
    pub const OUTPUT_TAIL_LINES: usize = 20;

    /// Maximum characters per output tail line.
    pub const OUTPUT_TAIL_LINE_MAX_CHARS: usize = 200;
}

/// Operational limits and thresholds.
pub mod limits {
    /// Auto-retry limit for CI gate failures.
    pub const CI_GATE_AUTO_RETRY_LIMIT: u8 = 10;

    /// Maximum automatic recovery attempts after runner signal termination.
    pub const MAX_SIGNAL_RESUMES: u8 = 5;

    /// Number of consecutive CI failures with the same error pattern before escalation.
    /// After this many identical failures, Ralph stops retrying and requires intervention.
    pub const CI_FAILURE_ESCALATION_THRESHOLD: u8 = 3;

    /// Maximum consecutive failures before aborting run loop.
    pub const MAX_CONSECUTIVE_FAILURES: u32 = 50;

    /// Maximum IDs to generate per invocation.
    pub const MAX_COUNT: usize = 100;

    /// Maximum lock cleanup retry attempts.
    pub const MAX_RETRIES: u32 = 3;

    /// Minimum environment variable value length for redaction.
    pub const MIN_ENV_VALUE_LEN: usize = 6;

    /// Default queue file size warning threshold (KB).
    pub const DEFAULT_SIZE_WARNING_THRESHOLD_KB: u32 = 500;

    /// Default task count warning threshold.
    pub const DEFAULT_TASK_COUNT_WARNING_THRESHOLD: u32 = 500;

    /// Maximum LFS pointer file size (bytes).
    pub const MAX_POINTER_SIZE: u64 = 1024;

    /// Maximum number of queue backup files to retain in `.ralph/cache`.
    pub const MAX_QUEUE_BACKUP_FILES: usize = 50;

    /// Maximum number of undo snapshots to retain in `.ralph/cache/undo`.
    pub const MAX_UNDO_SNAPSHOTS: usize = 20;
}

/// Timeout and interval durations.
pub mod timeouts {
    use std::time::Duration;

    /// Default session timeout in hours.
    pub const DEFAULT_SESSION_TIMEOUT_HOURS: u64 = 24;

    /// Spinner animation update interval in milliseconds.
    pub const SPINNER_UPDATE_INTERVAL_MS: u64 = 80;

    /// Temporary file retention period (7 days).
    ///
    /// Files older than this are cleaned up:
    /// - On CLI startup (main.rs)
    /// - When building runner commands (with_temp_prompt_file)
    /// - When running `ralph cleanup` command
    ///
    /// Default: 7 days. This balances keeping safeguard dumps available for
    /// debugging against preventing indefinite accumulation.
    pub const TEMP_RETENTION: Duration = Duration::from_secs(60 * 60 * 24 * 7);

    /// Lock cleanup retry delays in milliseconds.
    pub const DELAYS_MS: [u64; 3] = [10, 50, 100];

    /// Managed subprocess timeout for short probes (doctor, availability checks).
    pub const MANAGED_SUBPROCESS_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

    /// Managed subprocess timeout for short-lived metadata probes.
    pub const MANAGED_SUBPROCESS_METADATA_TIMEOUT: Duration = Duration::from_secs(10);

    /// Managed subprocess timeout for standard git operations.
    pub const MANAGED_SUBPROCESS_GIT_TIMEOUT: Duration = Duration::from_secs(120);

    /// Managed subprocess timeout for GitHub CLI operations.
    pub const MANAGED_SUBPROCESS_GH_TIMEOUT: Duration = Duration::from_secs(180);

    /// Managed subprocess timeout for processor plugin hooks.
    pub const MANAGED_SUBPROCESS_PLUGIN_TIMEOUT: Duration = Duration::from_secs(300);

    /// Managed subprocess timeout for app launches routed through platform launchers.
    pub const MANAGED_SUBPROCESS_APP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);

    /// Managed subprocess timeout for notification media playback commands.
    pub const MANAGED_SUBPROCESS_MEDIA_PLAYBACK_TIMEOUT: Duration = Duration::from_secs(20);

    /// Managed subprocess timeout for CI gate execution.
    pub const MANAGED_SUBPROCESS_CI_TIMEOUT: Duration = Duration::from_secs(60 * 30);

    /// Grace period after SIGINT before escalating to SIGKILL for managed subprocesses.
    pub const MANAGED_SUBPROCESS_INTERRUPT_GRACE: Duration = Duration::from_secs(2);

    /// Polling cadence for managed subprocess wait loops.
    pub const MANAGED_SUBPROCESS_POLL_INTERVAL: Duration = Duration::from_millis(50);

    /// Best-effort reap timeout after a managed subprocess receives SIGKILL.
    pub const MANAGED_SUBPROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(5);

    /// Polling cadence for cancellation-aware retry waits.
    pub const MANAGED_RETRY_POLL_INTERVAL: Duration = Duration::from_millis(50);

    /// How long terminal worker records are retained before stale pruning.
    ///
    /// This prevents immediate task reselection while avoiding permanent capacity blockers.
    pub const PARALLEL_TERMINAL_WORKER_TTL: Duration =
        Duration::from_secs(60 * 60 * DEFAULT_SESSION_TIMEOUT_HOURS);
}

/// UI layout and dimension constants.
pub mod ui {
    /// Threshold for narrow layout mode.
    pub const NARROW_LAYOUT_WIDTH: u16 = 90;

    /// Minimum width for board view.
    pub const BOARD_MIN_WIDTH: u16 = 100;

    /// Gutter width between columns.
    pub const COLUMN_GUTTER: u16 = 1;

    /// Number of task builder fields.
    pub const TASK_BUILDER_FIELD_COUNT: usize = 7;
}

/// Queue configuration constants.
pub mod queue {
    /// Default queue ID prefix.
    pub const DEFAULT_ID_PREFIX: &str = "RQ";

    /// Default queue file path (relative to repo root).
    pub const DEFAULT_QUEUE_FILE: &str = ".ralph/queue.jsonc";

    /// Default done file path (relative to repo root).
    pub const DEFAULT_DONE_FILE: &str = ".ralph/done.jsonc";

    /// Default config file path (relative to repo root).
    pub const DEFAULT_CONFIG_FILE: &str = ".ralph/config.jsonc";

    /// Default maximum dependency depth.
    pub const DEFAULT_MAX_DEPENDENCY_DEPTH: u8 = 10;

    /// Aging threshold: warning days (low priority).
    pub const AGING_WARNING_DAYS: u32 = 7;

    /// Aging threshold: stale days (medium priority).
    pub const AGING_STALE_DAYS: u32 = 14;

    /// Aging threshold: rotten days (high priority).
    pub const AGING_ROTTEN_DAYS: u32 = 30;
}

/// Git-related constants.
pub mod git {
    /// Default branch prefix for parallel execution.
    pub const DEFAULT_BRANCH_PREFIX: &str = "ralph/";

    /// Sample task ID for branch validation.
    pub const SAMPLE_TASK_ID: &str = "RQ-0001";
}

/// Runner-related constants.
pub mod runner {
    /// Default CI gate command.
    pub const DEFAULT_CI_GATE_COMMAND: &str = "make ci";

    /// Supported phase values (1-3).
    pub const MIN_PHASES: u8 = 1;
    pub const MAX_PHASES: u8 = 3;

    /// Minimum iterations value.
    pub const MIN_ITERATIONS: u8 = 1;
    pub const MIN_ITERATIONS_U32: u32 = 1;

    /// Minimum workers for parallel execution.
    pub const MIN_PARALLEL_WORKERS: u8 = 2;

    /// Minimum merge retries.
    pub const MIN_MERGE_RETRIES: u8 = 1;
}

/// File paths and directory names.
pub mod paths {
    /// Session state filename.
    pub const SESSION_FILENAME: &str = "session.jsonc";

    /// Stop signal filename.
    pub const STOP_SIGNAL_FILE: &str = "stop_requested";

    /// Migration history file path.
    pub const MIGRATION_HISTORY_PATH: &str = ".ralph/cache/migrations.jsonc";

    /// Productivity stats filename.
    pub const STATS_FILENAME: &str = "productivity.jsonc";

    /// Ralph temp directory name.
    pub const RALPH_TEMP_DIR_NAME: &str = "ralph";

    /// Legacy prompt temp file prefix.
    pub const LEGACY_PROMPT_PREFIX: &str = "ralph_prompt_";

    /// Ralph temp file prefix.
    pub const RALPH_TEMP_PREFIX: &str = "ralph_";

    /// Worker prompt override path.
    pub const WORKER_OVERRIDE_PATH: &str = ".ralph/prompts/worker.md";

    /// Scan prompt override path.
    pub const SCAN_OVERRIDE_PATH: &str = ".ralph/prompts/scan.md";

    /// Task builder prompt override path.
    pub const TASK_BUILDER_OVERRIDE_PATH: &str = ".ralph/prompts/task_builder.md";

    /// Environment variable for raw dump mode.
    pub const ENV_RAW_DUMP: &str = "RALPH_RAW_DUMP";

    /// Environment variable for the runner actually used (set by Ralph when spawning runners).
    /// Used for analytics tracking in task custom fields.
    pub const ENV_RUNNER_USED: &str = "RALPH_RUNNER_USED";

    /// Environment variable for the model actually used (set by Ralph when spawning runners).
    /// Used for analytics tracking in task custom fields.
    pub const ENV_MODEL_USED: &str = "RALPH_MODEL_USED";
}

/// Version constants for schemas and templates.
pub mod versions {
    /// README template version.
    pub const README_VERSION: u32 = 7;

    /// Session state schema version.
    pub const SESSION_STATE_VERSION: u32 = 1;

    /// Migration history schema version.
    pub const HISTORY_VERSION: u32 = 1;

    /// Productivity stats schema version.
    pub const STATS_SCHEMA_VERSION: u32 = 1;

    /// Execution history schema version.
    pub const EXECUTION_HISTORY_VERSION: u32 = 1;

    /// Template version string.
    pub const TEMPLATE_VERSION: &str = "1.0.0";
}

/// Default values for models and configuration.
pub mod defaults {
    /// Default Gemini model name.
    pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3-flash-preview";

    /// Default Claude model name.
    pub const DEFAULT_CLAUDE_MODEL: &str = "sonnet";

    /// Default Cursor model name.
    pub const DEFAULT_CURSOR_MODEL: &str = "auto";

    /// Opencode prompt file message.
    pub const OPENCODE_PROMPT_FILE_MESSAGE: &str = "Follow the attached prompt file verbatim.";

    /// Fallback message for Phase 2 final response.
    pub const PHASE2_FINAL_RESPONSE_FALLBACK: &str = "(Phase 2 final response unavailable.)";

    /// Git LFS pointer file prefix.
    pub const LFS_POINTER_PREFIX: &str = "version https://git-lfs.github.com/spec/v1";

    /// Redaction placeholder text.
    pub const REDACTED: &str = "[REDACTED]";

    /// Sentinel RFC3339 timestamp used only when formatting "now" fails.
    ///
    /// This value is intentionally "obviously wrong" in modern persisted data so
    /// fallback usage is detectable during debugging/audits.
    pub const FALLBACK_RFC3339: &str = "1970-01-01T00:00:00.000000000Z";

    /// Default task ID width.
    pub const DEFAULT_ID_WIDTH: usize = 4;
}

/// UI symbols and emoji.
pub mod symbols {
    /// Celebration sparkles emoji.
    pub const SPARKLES: &str = "✨";

    /// Streak fire emoji.
    pub const FIRE: &str = "🔥";

    /// Achievement star emoji.
    pub const STAR: &str = "⭐";

    /// Completion checkmark.
    pub const CHECKMARK: &str = "✓";
}

/// Spinner animation frames.
pub mod spinners {
    /// Default braille spinner frames.
    pub const DEFAULT_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
}

/// Milestone and achievement thresholds.
pub mod milestones {
    /// Task count milestones for achievements.
    pub const MILESTONE_THRESHOLDS: &[u64] = &[10, 50, 100, 250, 500, 1000, 2500, 5000];
}

/// AGENTS.md section requirements.
pub mod agents_md {
    /// Required sections in AGENTS.md.
    pub const REQUIRED_SECTIONS: &[&str] =
        &["Non-Negotiables", "Repository Map", "Build, Test, and CI"];

    /// Recommended sections for a complete AGENTS.md.
    pub const RECOMMENDED_SECTIONS: &[&str] = &[
        "Non-Negotiables",
        "Repository Map",
        "Build, Test, and CI",
        "Testing",
        "Workflow Contracts",
        "Configuration",
        "Git Hygiene",
        "Documentation Maintenance",
        "Troubleshooting",
    ];
}

/// Custom field keys for analytics/observability data.
pub mod custom_fields {
    /// Key for the runner actually used (observational, not intent).
    pub const RUNNER_USED: &str = "runner_used";

    /// Key for the model actually used (observational, not intent).
    pub const MODEL_USED: &str = "model_used";
}

/// Error message templates for consistent error formatting.
pub mod error_messages {
    /// Config update instruction suffix.
    pub const CONFIG_UPDATE_INSTRUCTION: &str = "Update .ralph/config.jsonc";

    /// Template for invalid config value errors.
    pub fn invalid_config_value(
        field: &str,
        value: impl std::fmt::Display,
        reason: &str,
    ) -> String {
        format!("Invalid {field}: {value}. {reason}. Update .ralph/config.jsonc.")
    }
}

/// Status classification keywords for theme/styling.
pub mod status_keywords {
    /// Keywords indicating error status.
    pub const ERROR: &[&str] = &[
        "error", "fail", "failed", "denied", "timeout", "cancel", "canceled",
    ];

    /// Keywords indicating in-progress/warning status.
    pub const IN_PROGRESS: &[&str] = &[
        "running",
        "started",
        "pending",
        "queued",
        "in_progress",
        "working",
    ];

    /// Keywords indicating success status.
    pub const SUCCESS: &[&str] = &[
        "completed",
        "success",
        "succeeded",
        "ok",
        "done",
        "finished",
    ];
}
