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
use crate::contracts::model::{Model, ReasoningEffort};
use crate::contracts::runner::{
    ClaudePermissionMode, Runner, RunnerApprovalMode, RunnerCliConfigRoot, RunnerCliOptionsPatch,
    RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
    UnsupportedOptionPolicy,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// Submodules
mod agent;
mod enums;
mod loop_;
mod notification;
mod parallel;
mod phase;
mod plugin;
mod queue;
mod retry;
#[cfg(test)]
mod tests;
mod webhook;

// Re-exports from submodules (backward compatibility)
pub use agent::AgentConfig;
pub use enums::{GitRevertMode, ProjectType, ScanPromptVersion};
pub use loop_::LoopConfig;
pub use notification::NotificationConfig;
pub use parallel::{ConflictPolicy, ParallelConfig, ParallelMergeMethod, ParallelMergeWhen};
pub use phase::{PhaseOverrideConfig, PhaseOverrides};
pub use plugin::{PluginConfig, PluginProcessorConfig, PluginRunnerConfig, PluginsConfig};
pub use queue::{QueueAgingThresholds, QueueConfig};
pub use retry::RunnerRetryConfig;
pub use webhook::{WebhookConfig, WebhookQueuePolicy};

/* ----------------------------- Config (JSON) ----------------------------- */
/*
Config is layered:
- Global config (defaults)
- Project config (overrides)
Merge is leaf-wise: project values override global values when the project value is Some(...).
To make that merge unambiguous, leaf fields are Option<T>.
*/

/// Root configuration struct for Ralph.
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

    /// Run loop waiting configuration (daemon/continuous mode).
    #[serde(rename = "loop")]
    pub loop_field: LoopConfig,

    /// Plugin configuration (enable/disable + per-plugin settings).
    pub plugins: PluginsConfig,

    /// Optional named profiles for quick workflow switching.
    ///
    /// Each profile is an AgentConfig-shaped patch applied over `agent` when selected.
    /// Profile values override base config but are overridden by CLI flags and task.agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profiles: Option<BTreeMap<String, AgentConfig>>,
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
                auto_archive_terminal_after_days: None,
                aging_thresholds: Some(QueueAgingThresholds {
                    warning_days: Some(7),
                    stale_days: Some(14),
                    rotten_days: Some(30),
                }),
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
                runner_retry: RunnerRetryConfig::default(),
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
            loop_field: LoopConfig {
                wait_when_empty: Some(false),
                empty_poll_ms: Some(30_000),
                wait_when_blocked: Some(false),
                wait_poll_ms: Some(1000),
                wait_timeout_seconds: Some(0),
                notify_when_unblocked: Some(false),
            },
            plugins: PluginsConfig::default(),
            profiles: None,
        }
    }
}
