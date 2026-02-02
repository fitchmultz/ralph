//! Configuration contracts for Ralph.
//!
//! Responsibilities:
//! - Define config structs/enums and their merge behavior.
//! - Provide defaults and schema helpers for configuration serialization.
//!
//! Not handled here:
//! - Reading/writing config files or CLI parsing (see `crate::config`).
//! - Queue/task contract definitions (see `super::queue` and `super::task`).
//! - Runner definitions (see `super::runner`).
//! - Model definitions (see `super::model`).
//!
//! Invariants/assumptions:
//! - Config merge is leaf-wise: `Some` values override, `None` does not.
//! - Serde/schemars attributes define the config contract.

use crate::constants::defaults::DEFAULT_ID_WIDTH;
use crate::constants::limits::{
    DEFAULT_SIZE_WARNING_THRESHOLD_KB, DEFAULT_TASK_COUNT_WARNING_THRESHOLD,
};
use crate::constants::timeouts::DEFAULT_SESSION_TIMEOUT_HOURS;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Re-export types moved to runner.rs and model.rs for backward compatibility
pub use crate::contracts::model::{Model, ReasoningEffort};
pub use crate::contracts::runner::{
    ClaudePermissionMode, MergeRunnerConfig, Runner, RunnerApprovalMode, RunnerCliConfigRoot,
    RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
    UnsupportedOptionPolicy,
};

/* ----------------------------- Config (JSON) ----------------------------- */
/*
Config is layered:
- Global config (defaults)
- Project config (overrides)
Merge is leaf-wise: project values override global values when the project value is Some(...).
To make that merge unambiguous, leaf fields are Option<T>.
*/

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Schema version for config.
    pub version: u32,

    /// "code" or "docs". Drives prompt defaults and small workflow decisions.
    pub project_type: Option<ProjectType>,

    /// Queue-related configuration.
    pub queue: QueueConfig,

    /// Agent runner defaults (Claude, Codex, OpenCode, Gemini, or Cursor).
    pub agent: AgentConfig,

    /// Parallel run-loop configuration.
    pub parallel: ParallelConfig,

    /// TUI-specific configuration.
    pub tui: TuiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct QueueConfig {
    /// Path to the JSON queue file, relative to repo root.
    pub file: Option<PathBuf>,

    /// Path to the JSON done archive file, relative to repo root.
    pub done_file: Option<PathBuf>,

    /// ID prefix (default: "RQ").
    pub id_prefix: Option<String>,

    /// Zero pad width for the numeric suffix (default: 4 -> RQ-0001).
    pub id_width: Option<u8>,

    /// Warning threshold for queue file size in KB (default: 500).
    #[schemars(range(min = 100, max = 10000))]
    pub size_warning_threshold_kb: Option<u32>,

    /// Warning threshold for number of tasks in queue (default: 500).
    #[schemars(range(min = 50, max = 5000))]
    pub task_count_warning_threshold: Option<u32>,

    /// Maximum allowed dependency chain depth before warning (default: 10).
    #[schemars(range(min = 1, max = 100))]
    pub max_dependency_depth: Option<u8>,
}

impl QueueConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.file.is_some() {
            self.file = other.file;
        }
        if other.done_file.is_some() {
            self.done_file = other.done_file;
        }
        if other.id_prefix.is_some() {
            self.id_prefix = other.id_prefix;
        }
        if other.id_width.is_some() {
            self.id_width = other.id_width;
        }
        if other.size_warning_threshold_kb.is_some() {
            self.size_warning_threshold_kb = other.size_warning_threshold_kb;
        }
        if other.task_count_warning_threshold.is_some() {
            self.task_count_warning_threshold = other.task_count_warning_threshold;
        }
        if other.max_dependency_depth.is_some() {
            self.max_dependency_depth = other.max_dependency_depth;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    /// Which harness to use by default.
    pub runner: Option<Runner>,

    /// Default model.
    pub model: Option<Model>,

    /// Default reasoning effort (only meaningful for Codex models).
    pub reasoning_effort: Option<ReasoningEffort>,

    /// Number of iterations to run for each task (default: 1).
    #[schemars(range(min = 1))]
    pub iterations: Option<u8>,

    /// Reasoning effort override for follow-up iterations (iterations > 1).
    /// Only meaningful for Codex models.
    pub followup_reasoning_effort: Option<ReasoningEffort>,

    /// Override the codex executable name/path (default is "codex" if None).
    pub codex_bin: Option<String>,

    /// Override the opencode executable name/path (default is "opencode" if None).
    pub opencode_bin: Option<String>,

    /// Override the gemini executable name/path (default is "gemini" if None).
    pub gemini_bin: Option<String>,

    /// Override the claude executable name/path (default is "claude" if None).
    pub claude_bin: Option<String>,

    /// Override the cursor agent executable name/path (default is "agent" if None).
    ///
    /// NOTE: Cursor's runner binary name is `agent` (not `cursor`).
    pub cursor_bin: Option<String>,

    /// Override the kimi executable name/path (default is "kimi" if None).
    pub kimi_bin: Option<String>,

    /// Override the pi executable name/path (default is "pi" if None).
    pub pi_bin: Option<String>,

    /// Claude permission mode for tool and edit approval.
    /// AcceptEdits: auto-approves file edits only
    /// BypassPermissions: skip all permission prompts (YOLO mode)
    pub claude_permission_mode: Option<ClaudePermissionMode>,

    /// Normalized runner CLI behavior overrides (output/approval/sandbox/etc).
    ///
    /// This is additive: existing runner-specific fields remain supported.
    pub runner_cli: Option<RunnerCliConfigRoot>,

    /// Per-phase overrides for runner, model, and reasoning effort.
    ///
    /// Allows specifying different settings for each phase (1, 2, 3).
    /// Phase-specific values override the global agent settings.
    pub phase_overrides: Option<PhaseOverrides>,

    /// Additional instruction files to inject at the top of every prompt sent to runner CLIs.
    ///
    /// Paths may be absolute, `~/`-prefixed, or repo-root relative. Missing files are treated as
    /// configuration errors. To include repo-local AGENTS.md, add `"AGENTS.md"` to this list.
    pub instruction_files: Option<Vec<PathBuf>>,

    /// Require RepoPrompt usage during planning (inject context_builder instructions).
    pub repoprompt_plan_required: Option<bool>,

    /// Inject RepoPrompt tooling reminder block into prompts.
    pub repoprompt_tool_injection: Option<bool>,

    /// CI gate command to run (default: "make ci").
    pub ci_gate_command: Option<String>,

    /// Enable or disable the CI gate entirely (default: true).
    pub ci_gate_enabled: Option<bool>,

    /// Controls automatic git revert behavior when runner or supervision errors occur.
    pub git_revert_mode: Option<GitRevertMode>,

    /// Enable automatic git commit and push after successful runs (default: true).
    pub git_commit_push_enabled: Option<bool>,

    /// Number of execution phases (1, 2, or 3).
    /// 1 = single-pass, 2 = plan+implement, 3 = plan+implement+review.
    #[schemars(range(min = 1, max = 3))]
    pub phases: Option<u8>,

    /// If true, automatically run `ralph task update <TASK_ID>` once per task
    /// immediately before the supervisor marks the task as `doing` and starts execution.
    ///
    /// Default: false (opt-in).
    pub update_task_before_run: Option<bool>,

    /// If true, fail the run when pre-run task update fails.
    /// If false (default), log a warning and continue with original task data.
    pub fail_on_prerun_update_error: Option<bool>,

    /// Desktop notification configuration for task completion.
    pub notification: NotificationConfig,

    /// Webhook configuration for HTTP task event notifications.
    pub webhook: WebhookConfig,

    /// Session timeout in hours for crash recovery (default: 24).
    /// Sessions older than this threshold are considered stale and require
    /// explicit user confirmation to resume.
    #[schemars(range(min = 1))]
    pub session_timeout_hours: Option<u64>,

    /// Scan prompt version to use (v1 or v2, default: v2).
    pub scan_prompt_version: Option<ScanPromptVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ParallelConfig {
    /// Number of workers to run concurrently when parallel mode is enabled.
    #[schemars(range(min = 2))]
    pub workers: Option<u8>,

    /// When to merge PRs (as created or after all tasks complete).
    pub merge_when: Option<ParallelMergeWhen>,

    /// Merge method to use for PRs.
    pub merge_method: Option<ParallelMergeMethod>,

    /// Automatically create PRs for completed tasks.
    pub auto_pr: Option<bool>,

    /// Automatically merge PRs when eligible.
    pub auto_merge: Option<bool>,

    /// Create draft PRs when a worker fails.
    pub draft_on_failure: Option<bool>,

    /// Policy for handling merge conflicts.
    pub conflict_policy: Option<ConflictPolicy>,

    /// Number of merge retries before giving up.
    #[schemars(range(min = 1))]
    pub merge_retries: Option<u8>,

    /// Root directory for parallel workspaces (relative to repo root if not absolute).
    pub workspace_root: Option<PathBuf>,

    /// Branch name prefix for parallel workers (e.g., "ralph/").
    pub branch_prefix: Option<String>,

    /// Delete branches after merge.
    pub delete_branch_on_merge: Option<bool>,

    /// Runner overrides for merge conflict resolution.
    pub merge_runner: Option<MergeRunnerConfig>,
}

impl ParallelConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.workers.is_some() {
            self.workers = other.workers;
        }
        if other.merge_when.is_some() {
            self.merge_when = other.merge_when;
        }
        if other.merge_method.is_some() {
            self.merge_method = other.merge_method;
        }
        if other.auto_pr.is_some() {
            self.auto_pr = other.auto_pr;
        }
        if other.auto_merge.is_some() {
            self.auto_merge = other.auto_merge;
        }
        if other.draft_on_failure.is_some() {
            self.draft_on_failure = other.draft_on_failure;
        }
        if other.conflict_policy.is_some() {
            self.conflict_policy = other.conflict_policy;
        }
        if other.merge_retries.is_some() {
            self.merge_retries = other.merge_retries;
        }
        if other.workspace_root.is_some() {
            self.workspace_root = other.workspace_root;
        }
        if other.branch_prefix.is_some() {
            self.branch_prefix = other.branch_prefix;
        }
        if other.delete_branch_on_merge.is_some() {
            self.delete_branch_on_merge = other.delete_branch_on_merge;
        }
        if let Some(other_merge_runner) = other.merge_runner {
            match &mut self.merge_runner {
                Some(existing) => existing.merge_from(other_merge_runner),
                None => self.merge_runner = Some(other_merge_runner),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ParallelMergeWhen {
    #[default]
    AsCreated,
    AfterAll,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ParallelMergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConflictPolicy {
    #[default]
    AutoResolve,
    RetryLater,
    Reject,
}

impl AgentConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.runner.is_some() {
            self.runner = other.runner;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.reasoning_effort.is_some() {
            self.reasoning_effort = other.reasoning_effort;
        }
        if other.iterations.is_some() {
            self.iterations = other.iterations;
        }
        if other.followup_reasoning_effort.is_some() {
            self.followup_reasoning_effort = other.followup_reasoning_effort;
        }
        if other.codex_bin.is_some() {
            self.codex_bin = other.codex_bin;
        }
        if other.opencode_bin.is_some() {
            self.opencode_bin = other.opencode_bin;
        }
        if other.gemini_bin.is_some() {
            self.gemini_bin = other.gemini_bin;
        }
        if other.claude_bin.is_some() {
            self.claude_bin = other.claude_bin;
        }
        if other.cursor_bin.is_some() {
            self.cursor_bin = other.cursor_bin;
        }
        if other.kimi_bin.is_some() {
            self.kimi_bin = other.kimi_bin;
        }
        if other.pi_bin.is_some() {
            self.pi_bin = other.pi_bin;
        }
        if other.phases.is_some() {
            self.phases = other.phases;
        }
        if other.claude_permission_mode.is_some() {
            self.claude_permission_mode = other.claude_permission_mode;
        }
        if let Some(other_runner_cli) = other.runner_cli {
            match &mut self.runner_cli {
                Some(existing) => existing.merge_from(other_runner_cli),
                None => self.runner_cli = Some(other_runner_cli),
            }
        }
        // Merge phase_overrides
        if let Some(other_phase_overrides) = other.phase_overrides {
            match &mut self.phase_overrides {
                Some(existing) => existing.merge_from(other_phase_overrides),
                None => self.phase_overrides = Some(other_phase_overrides),
            }
        }
        if other.instruction_files.is_some() {
            self.instruction_files = other.instruction_files;
        }
        if other.repoprompt_plan_required.is_some() {
            self.repoprompt_plan_required = other.repoprompt_plan_required;
        }
        if other.repoprompt_tool_injection.is_some() {
            self.repoprompt_tool_injection = other.repoprompt_tool_injection;
        }
        if other.ci_gate_command.is_some() {
            self.ci_gate_command = other.ci_gate_command;
        }
        if other.ci_gate_enabled.is_some() {
            self.ci_gate_enabled = other.ci_gate_enabled;
        }
        if other.git_revert_mode.is_some() {
            self.git_revert_mode = other.git_revert_mode;
        }
        if other.git_commit_push_enabled.is_some() {
            self.git_commit_push_enabled = other.git_commit_push_enabled;
        }
        if other.update_task_before_run.is_some() {
            self.update_task_before_run = other.update_task_before_run;
        }
        if other.fail_on_prerun_update_error.is_some() {
            self.fail_on_prerun_update_error = other.fail_on_prerun_update_error;
        }
        self.notification.merge_from(other.notification);
        self.webhook.merge_from(other.webhook);
        if other.session_timeout_hours.is_some() {
            self.session_timeout_hours = other.session_timeout_hours;
        }
        if other.scan_prompt_version.is_some() {
            self.scan_prompt_version = other.scan_prompt_version;
        }
    }
}

/// Per-phase configuration overrides for runner, model, and reasoning effort.
///
/// All fields are optional to support leaf-wise merging:
/// - `Some(value)` overrides the parent config
/// - `None` means "inherit from parent"
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PhaseOverrideConfig {
    /// Runner to use for this phase (overrides global agent.runner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<Runner>,

    /// Model to use for this phase (overrides global agent.model)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,

    /// Reasoning effort for this phase (overrides global agent.reasoning_effort)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl PhaseOverrideConfig {
    /// Leaf-wise merge: other.Some overrides self, other.None preserves self
    pub fn merge_from(&mut self, other: Self) {
        if other.runner.is_some() {
            self.runner = other.runner;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.reasoning_effort.is_some() {
            self.reasoning_effort = other.reasoning_effort;
        }
    }
}

/// Phase overrides container for Phase 1/2/3 execution.
///
/// Per-phase configuration for Phase 1/2/3 execution.
///
/// Invariants/assumptions:
/// - Overrides are defined per phase only; there is no shared `defaults` layer inside
///   `agent.phase_overrides`. Use global `agent.runner` / `agent.model` /
///   `agent.reasoning_effort` for shared defaults.
/// - Merging is leaf-wise: `Some(value)` overrides, `None` inherits.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PhaseOverrides {
    /// Phase 1 specific overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase1: Option<PhaseOverrideConfig>,

    /// Phase 2 specific overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase2: Option<PhaseOverrideConfig>,

    /// Phase 3 specific overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase3: Option<PhaseOverrideConfig>,
}

impl PhaseOverrides {
    /// Merge other into self following leaf-wise semantics:
    /// Merge each specific phase override
    pub fn merge_from(&mut self, other: Self) {
        // Merge phase1
        match (&mut self.phase1, other.phase1) {
            (Some(existing), Some(new)) => existing.merge_from(new),
            (None, Some(new)) => self.phase1 = Some(new),
            _ => {}
        }

        // Merge phase2
        match (&mut self.phase2, other.phase2) {
            (Some(existing), Some(new)) => existing.merge_from(new),
            (None, Some(new)) => self.phase2 = Some(new),
            _ => {}
        }

        // Merge phase3
        match (&mut self.phase3, other.phase3) {
            (Some(existing), Some(new)) => existing.merge_from(new),
            (None, Some(new)) => self.phase3 = Some(new),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    #[default]
    Code,
    Docs,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitRevertMode {
    #[default]
    Ask,
    Enabled,
    Disabled,
}

impl std::str::FromStr for GitRevertMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "ask" => Ok(GitRevertMode::Ask),
            "enabled" => Ok(GitRevertMode::Enabled),
            "disabled" => Ok(GitRevertMode::Disabled),
            _ => Err("git_revert_mode must be 'ask', 'enabled', or 'disabled'"),
        }
    }
}

/// Behavior for auto-archiving terminal tasks (Done/Rejected) when set via TUI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutoArchiveBehavior {
    /// Never auto-archive (current behavior).
    #[default]
    Never,
    /// Ask before archiving.
    Prompt,
    /// Archive immediately without prompt.
    Always,
}

impl std::str::FromStr for AutoArchiveBehavior {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "never" => Ok(AutoArchiveBehavior::Never),
            "prompt" => Ok(AutoArchiveBehavior::Prompt),
            "always" => Ok(AutoArchiveBehavior::Always),
            _ => Err("auto_archive_behavior must be 'never', 'prompt', or 'always'"),
        }
    }
}

/// Scan prompt version to use for scan operations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanPromptVersion {
    /// Version 1: Original rule-based scan prompts with fixed minimum task counts.
    V1,
    /// Version 2: Rubric-based scan prompts with quality-focused STOP CONDITION (default).
    #[default]
    V2,
}

/// TUI-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TuiConfig {
    /// Auto-archive behavior for terminal tasks (Done/Rejected) when set via TUI.
    pub auto_archive_terminal: Option<AutoArchiveBehavior>,
    /// Enable celebration animations on task completion (default: true).
    pub celebrations_enabled: Option<bool>,
    /// Enable productivity stats tracking (default: true).
    pub stats_enabled: Option<bool>,
}

impl TuiConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.auto_archive_terminal.is_some() {
            self.auto_archive_terminal = other.auto_archive_terminal;
        }
        if other.celebrations_enabled.is_some() {
            self.celebrations_enabled = other.celebrations_enabled;
        }
        if other.stats_enabled.is_some() {
            self.stats_enabled = other.stats_enabled;
        }
    }
}

/// Desktop notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationConfig {
    /// Enable desktop notifications on task completion (default: true).
    /// This is the legacy/compatibility field; prefer `notify_on_complete`.
    pub enabled: Option<bool>,

    /// Enable desktop notifications on task completion (default: true).
    pub notify_on_complete: Option<bool>,

    /// Enable desktop notifications on task failure (default: true).
    pub notify_on_fail: Option<bool>,

    /// Enable desktop notifications when loop mode completes (default: true).
    pub notify_on_loop_complete: Option<bool>,

    /// Suppress notifications when TUI is active (default: true).
    pub suppress_when_active: Option<bool>,

    /// Enable sound alerts with notifications (default: false).
    pub sound_enabled: Option<bool>,

    /// Custom sound file path (platform-specific format).
    /// If not set, uses platform default sounds.
    pub sound_path: Option<String>,

    /// Notification timeout in milliseconds (default: 8000).
    #[schemars(range(min = 1000, max = 60000))]
    pub timeout_ms: Option<u32>,
}

impl NotificationConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if other.notify_on_complete.is_some() {
            self.notify_on_complete = other.notify_on_complete;
        }
        if other.notify_on_fail.is_some() {
            self.notify_on_fail = other.notify_on_fail;
        }
        if other.notify_on_loop_complete.is_some() {
            self.notify_on_loop_complete = other.notify_on_loop_complete;
        }
        if other.suppress_when_active.is_some() {
            self.suppress_when_active = other.suppress_when_active;
        }
        if other.sound_enabled.is_some() {
            self.sound_enabled = other.sound_enabled;
        }
        if other.sound_path.is_some() {
            self.sound_path = other.sound_path;
        }
        if other.timeout_ms.is_some() {
            self.timeout_ms = other.timeout_ms;
        }
    }
}

/// Webhook configuration for HTTP task event notifications.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WebhookConfig {
    /// Enable webhook notifications (default: false).
    pub enabled: Option<bool>,

    /// Webhook endpoint URL (required when enabled).
    pub url: Option<String>,

    /// Secret key for HMAC-SHA256 signature generation.
    /// When set, webhooks include an X-Ralph-Signature header.
    pub secret: Option<String>,

    /// Events to subscribe to (default: all).
    /// Supported: task_created, task_started, task_completed, task_failed, task_status_changed
    pub events: Option<Vec<String>>,

    /// Request timeout in seconds (default: 30, max: 300).
    #[schemars(range(min = 1, max = 300))]
    pub timeout_secs: Option<u32>,

    /// Number of retry attempts for failed deliveries (default: 3, max: 10).
    #[schemars(range(min = 0, max = 10))]
    pub retry_count: Option<u32>,

    /// Retry backoff base in milliseconds (default: 1000, max: 30000).
    #[schemars(range(min = 100, max = 30000))]
    pub retry_backoff_ms: Option<u32>,
}

impl WebhookConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if other.url.is_some() {
            self.url = other.url;
        }
        if other.secret.is_some() {
            self.secret = other.secret;
        }
        if other.events.is_some() {
            self.events = other.events;
        }
        if other.timeout_secs.is_some() {
            self.timeout_secs = other.timeout_secs;
        }
        if other.retry_count.is_some() {
            self.retry_count = other.retry_count;
        }
        if other.retry_backoff_ms.is_some() {
            self.retry_backoff_ms = other.retry_backoff_ms;
        }
    }

    /// Check if a specific event type is enabled.
    pub fn is_event_enabled(&self, event: &str) -> bool {
        if !self.enabled.unwrap_or(false) {
            return false;
        }
        match &self.events {
            None => true, // All events enabled by default
            Some(events) => events.iter().any(|e| e == event || e == "*"),
        }
    }
}

/* ------------------------------ Defaults -------------------------------- */

impl Default for Config {
    fn default() -> Self {
        use std::collections::BTreeMap;
        Self {
            version: 1,
            project_type: Some(ProjectType::Code),
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(DEFAULT_ID_WIDTH as u8),
                size_warning_threshold_kb: Some(DEFAULT_SIZE_WARNING_THRESHOLD_KB),
                task_count_warning_threshold: Some(DEFAULT_TASK_COUNT_WARNING_THRESHOLD),
                max_dependency_depth: Some(10),
            },
            agent: AgentConfig {
                runner: Some(Runner::Claude),
                model: Some(Model::Custom("sonnet".to_string())),
                reasoning_effort: Some(ReasoningEffort::Medium),
                iterations: Some(1),
                followup_reasoning_effort: None,
                codex_bin: Some("codex".to_string()),
                opencode_bin: Some("opencode".to_string()),
                gemini_bin: Some("gemini".to_string()),
                claude_bin: Some("claude".to_string()),
                cursor_bin: Some("agent".to_string()),
                kimi_bin: Some("kimi".to_string()),
                pi_bin: Some("pi".to_string()),
                phases: Some(3),
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                runner_cli: Some(RunnerCliConfigRoot {
                    defaults: RunnerCliOptionsPatch {
                        output_format: Some(RunnerOutputFormat::StreamJson),
                        verbosity: Some(RunnerVerbosity::Normal),
                        approval_mode: Some(RunnerApprovalMode::Yolo),
                        sandbox: Some(RunnerSandboxMode::Default),
                        plan_mode: Some(RunnerPlanMode::Default),
                        unsupported_option_policy: Some(UnsupportedOptionPolicy::Warn),
                    },
                    runners: BTreeMap::from([
                        (
                            Runner::Codex,
                            RunnerCliOptionsPatch {
                                sandbox: Some(RunnerSandboxMode::Disabled),
                                ..RunnerCliOptionsPatch::default()
                            },
                        ),
                        (
                            Runner::Claude,
                            RunnerCliOptionsPatch {
                                verbosity: Some(RunnerVerbosity::Verbose),
                                ..RunnerCliOptionsPatch::default()
                            },
                        ),
                        (
                            Runner::Kimi,
                            RunnerCliOptionsPatch {
                                approval_mode: Some(RunnerApprovalMode::Yolo),
                                ..RunnerCliOptionsPatch::default()
                            },
                        ),
                        (
                            Runner::Pi,
                            RunnerCliOptionsPatch {
                                approval_mode: Some(RunnerApprovalMode::Yolo),
                                ..RunnerCliOptionsPatch::default()
                            },
                        ),
                    ]),
                }),
                phase_overrides: None,
                instruction_files: None,
                repoprompt_plan_required: Some(false),
                repoprompt_tool_injection: Some(false),
                ci_gate_command: Some("make ci".to_string()),
                ci_gate_enabled: Some(true),
                git_revert_mode: Some(GitRevertMode::Ask),
                git_commit_push_enabled: Some(true),
                update_task_before_run: Some(false),
                fail_on_prerun_update_error: Some(false),
                notification: NotificationConfig {
                    enabled: Some(true),
                    notify_on_complete: Some(true),
                    notify_on_fail: Some(true),
                    notify_on_loop_complete: Some(true),
                    suppress_when_active: Some(true),
                    sound_enabled: Some(false),
                    sound_path: None,
                    timeout_ms: Some(8000),
                },
                webhook: WebhookConfig::default(),
                session_timeout_hours: Some(DEFAULT_SESSION_TIMEOUT_HOURS),
                scan_prompt_version: Some(ScanPromptVersion::V2),
            },
            parallel: ParallelConfig {
                workers: None,
                merge_when: Some(ParallelMergeWhen::AsCreated),
                merge_method: Some(ParallelMergeMethod::Squash),
                auto_pr: Some(true),
                auto_merge: Some(true),
                draft_on_failure: Some(true),
                conflict_policy: Some(ConflictPolicy::AutoResolve),
                merge_retries: Some(5),
                workspace_root: None,
                branch_prefix: Some("ralph/".to_string()),
                delete_branch_on_merge: Some(true),
                merge_runner: None,
            },
            tui: TuiConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentConfig, GitRevertMode, Model, NotificationConfig, PhaseOverrideConfig, PhaseOverrides,
        ReasoningEffort, Runner, WebhookConfig,
    };

    #[test]
    fn parallel_config_rejects_legacy_worktree_root_key() {
        let raw = r#"{
            "version": 1,
            "parallel": { "worktree_root": ".ralph/worktrees/custom" }
        }"#;
        let err = serde_json::from_str::<super::Config>(raw).unwrap_err();
        assert!(err.to_string().contains("worktree_root"));
    }

    #[test]
    fn git_revert_mode_parses_snake_case() {
        let mode: GitRevertMode = serde_json::from_str("\"ask\"").expect("ask");
        assert_eq!(mode, GitRevertMode::Ask);
        let mode: GitRevertMode = serde_json::from_str("\"enabled\"").expect("enabled");
        assert_eq!(mode, GitRevertMode::Enabled);
        let mode: GitRevertMode = serde_json::from_str("\"disabled\"").expect("disabled");
        assert_eq!(mode, GitRevertMode::Disabled);
    }

    #[test]
    fn git_revert_mode_from_str_rejects_invalid() {
        let err = "wat".parse::<GitRevertMode>().expect_err("invalid");
        assert!(err.contains("git_revert_mode"));
    }

    #[test]
    fn agent_config_merge_from_merges_update_task_before_run_leafwise() {
        let mut base = AgentConfig {
            update_task_before_run: Some(false),
            ..Default::default()
        };

        let other = AgentConfig {
            update_task_before_run: Some(true),
            ..Default::default()
        };

        base.merge_from(other);
        assert_eq!(base.update_task_before_run, Some(true));

        // None should not override an already-set value.
        base.merge_from(AgentConfig::default());
        assert_eq!(base.update_task_before_run, Some(true));
    }

    #[test]
    fn agent_config_merge_from_merges_fail_on_prerun_update_error_leafwise() {
        let mut base = AgentConfig {
            fail_on_prerun_update_error: Some(false),
            ..Default::default()
        };

        let other = AgentConfig {
            fail_on_prerun_update_error: Some(true),
            ..Default::default()
        };

        base.merge_from(other);
        assert_eq!(base.fail_on_prerun_update_error, Some(true));

        // None should not override an already-set value.
        base.merge_from(AgentConfig::default());
        assert_eq!(base.fail_on_prerun_update_error, Some(true));
    }

    #[test]
    fn test_phase_override_config_merge_from() {
        let mut base = PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: None,
            reasoning_effort: Some(ReasoningEffort::Medium),
        };

        let override_config = PhaseOverrideConfig {
            runner: Some(Runner::Claude),
            model: Some(Model::Custom("claude-opus-4".to_string())),
            reasoning_effort: None,
        };

        base.merge_from(override_config);

        assert_eq!(base.runner, Some(Runner::Claude)); // overridden
        assert_eq!(base.model, Some(Model::Custom("claude-opus-4".to_string()))); // set
        assert_eq!(base.reasoning_effort, Some(ReasoningEffort::Medium)); // preserved
    }

    #[test]
    fn test_phase_overrides_merge_from() {
        let mut base = PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Codex),
                model: Some(Model::Custom("o3-mini".to_string())),
                reasoning_effort: None,
            }),
            phase2: None,
            phase3: None,
        };

        let override_config = PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: None,
                model: Some(Model::Custom("claude-sonnet".to_string())),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
            phase2: Some(PhaseOverrideConfig {
                runner: Some(Runner::Gemini),
                model: None,
                reasoning_effort: None,
            }),
            phase3: None,
        };

        base.merge_from(override_config);

        // phase1 merged
        assert_eq!(base.phase1.as_ref().unwrap().runner, Some(Runner::Codex)); // preserved
        assert_eq!(
            base.phase1.as_ref().unwrap().model,
            Some(Model::Custom("claude-sonnet".to_string()))
        ); // overridden
        assert_eq!(
            base.phase1.as_ref().unwrap().reasoning_effort,
            Some(ReasoningEffort::High)
        ); // set

        // phase2 set from override
        assert_eq!(base.phase2.as_ref().unwrap().runner, Some(Runner::Gemini));

        // phase3 still None
        assert!(base.phase3.is_none());
    }

    #[test]
    fn test_agent_config_phase_overrides_merge() {
        let mut base = AgentConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Custom("o3-mini".to_string())),
            reasoning_effort: Some(ReasoningEffort::Medium),
            phases: Some(3),
            iterations: None,
            followup_reasoning_effort: None,
            codex_bin: None,
            opencode_bin: None,
            gemini_bin: None,
            claude_bin: None,
            cursor_bin: None,
            kimi_bin: None,
            pi_bin: None,
            claude_permission_mode: None,
            runner_cli: None,
            phase_overrides: Some(PhaseOverrides {
                phase1: None,
                phase2: None,
                phase3: None,
            }),
            instruction_files: None,
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: None,
            ci_gate_enabled: None,
            git_revert_mode: None,
            git_commit_push_enabled: None,
            update_task_before_run: None,
            fail_on_prerun_update_error: None,
            notification: NotificationConfig::default(),
            webhook: WebhookConfig::default(),
            session_timeout_hours: None,
            scan_prompt_version: None,
        };

        let override_config = AgentConfig {
            runner: Some(Runner::Claude),
            model: Some(Model::Custom("claude-sonnet".to_string())),
            reasoning_effort: Some(ReasoningEffort::High),
            phases: None,
            iterations: None,
            followup_reasoning_effort: None,
            codex_bin: None,
            opencode_bin: None,
            gemini_bin: None,
            claude_bin: None,
            cursor_bin: None,
            kimi_bin: None,
            pi_bin: None,
            claude_permission_mode: None,
            runner_cli: None,
            phase_overrides: Some(PhaseOverrides {
                phase1: Some(PhaseOverrideConfig {
                    runner: None,
                    model: Some(Model::Custom("claude-opus-4".to_string())),
                    reasoning_effort: Some(ReasoningEffort::XHigh),
                }),
                phase2: None,
                phase3: None,
            }),
            instruction_files: None,
            repoprompt_plan_required: None,
            repoprompt_tool_injection: None,
            ci_gate_command: None,
            ci_gate_enabled: None,
            git_revert_mode: None,
            git_commit_push_enabled: None,
            update_task_before_run: None,
            fail_on_prerun_update_error: None,
            notification: NotificationConfig::default(),
            webhook: WebhookConfig::default(),
            session_timeout_hours: None,
            scan_prompt_version: None,
        };

        base.merge_from(override_config);

        // Verify global settings merged
        assert_eq!(base.runner, Some(Runner::Claude));
        assert_eq!(base.model, Some(Model::Custom("claude-sonnet".to_string())));
        assert_eq!(base.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(base.phases, Some(3)); // preserved

        // Verify phase_overrides merged
        let phase1 = base
            .phase_overrides
            .as_ref()
            .unwrap()
            .phase1
            .as_ref()
            .unwrap();
        assert_eq!(phase1.runner, None); // preserved (None in override)
        assert_eq!(
            phase1.model,
            Some(Model::Custom("claude-opus-4".to_string()))
        );
        assert_eq!(phase1.reasoning_effort, Some(ReasoningEffort::XHigh));
    }
}
