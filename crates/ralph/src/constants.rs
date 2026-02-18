//! Centralized constants for the Ralph CLI.
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

    /// Number of output tail lines to display.
    pub const OUTPUT_TAIL_LINES: usize = 20;

    /// Maximum characters per output tail line.
    pub const OUTPUT_TAIL_LINE_MAX_CHARS: usize = 200;
}

/// Operational limits and thresholds.
pub mod limits {
    /// Auto-retry limit for CI gate failures.
    pub const CI_GATE_AUTO_RETRY_LIMIT: u8 = 5;

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

    /// How long a finished-without-PR record should block selection for transient failure reasons.
    ///
    /// This avoids permanent blockers while still preventing tight retry loops when PR creation
    /// is temporarily failing (auth, rate limiting, GitHub outage, etc.).
    pub const PARALLEL_FINISHED_WITHOUT_PR_BLOCKER_TTL: Duration =
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

/// File paths and directory names.
pub mod paths {
    /// Session state filename.
    pub const SESSION_FILENAME: &str = "session.json";

    /// Stop signal filename.
    pub const STOP_SIGNAL_FILE: &str = "stop_requested";

    /// Migration history file path.
    pub const MIGRATION_HISTORY_PATH: &str = ".ralph/cache/migrations.json";

    /// Productivity stats filename.
    pub const STATS_FILENAME: &str = "productivity.json";

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
    pub const README_VERSION: u32 = 5;

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
