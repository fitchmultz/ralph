//! Core data contracts for Ralph configuration and queue/task JSON structures.

#![allow(clippy::struct_excessive_bools)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

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

    /// Claude permission mode for tool and edit approval.
    /// AcceptEdits: auto-approves file edits only
    /// BypassPermissions: skip all permission prompts (YOLO mode)
    pub claude_permission_mode: Option<ClaudePermissionMode>,

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
        if other.phases.is_some() {
            self.phases = other.phases;
        }
        if other.claude_permission_mode.is_some() {
            self.claude_permission_mode = other.claude_permission_mode;
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
pub enum Runner {
    Codex,
    Opencode,
    Gemini,
    Cursor,
    #[default]
    Claude,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaudePermissionMode {
    #[default]
    AcceptEdits,
    BypassPermissions,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Model {
    #[default]
    Gpt52Codex,
    Gpt52,
    Glm47,
    Custom(String),
}

impl Serialize for Model {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Model {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

impl Model {
    pub fn as_str(&self) -> &str {
        match self {
            Model::Gpt52Codex => "gpt-5.2-codex",
            Model::Gpt52 => "gpt-5.2",
            Model::Glm47 => "zai-coding-plan/glm-4.7",
            Model::Custom(value) => value.as_str(),
        }
    }
}

impl std::str::FromStr for Model {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("model cannot be empty");
        }
        Ok(match trimmed {
            "gpt-5.2-codex" => Model::Gpt52Codex,
            "gpt-5.2" => Model::Gpt52,
            "zai-coding-plan/glm-4.7" => Model::Glm47,
            other => Model::Custom(other.to_string()),
        })
    }
}

// Manual JsonSchema implementation for Model since it has custom Serialize/Deserialize
impl schemars::JsonSchema for Model {
    fn schema_name() -> String {
        "Model".to_string()
    }

    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            ..Default::default()
        }
        .into()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    #[default]
    Medium,
    High,
    #[serde(rename = "xhigh")]
    #[schemars(rename = "xhigh")]
    XHigh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelEffort {
    #[default]
    Default,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    #[schemars(rename = "xhigh")]
    XHigh,
}

impl ModelEffort {
    pub fn as_reasoning_effort(self) -> Option<ReasoningEffort> {
        match self {
            ModelEffort::Default => None,
            ModelEffort::Low => Some(ReasoningEffort::Low),
            ModelEffort::Medium => Some(ReasoningEffort::Medium),
            ModelEffort::High => Some(ReasoningEffort::High),
            ModelEffort::XHigh => Some(ReasoningEffort::XHigh),
        }
    }
}

/* --------------------------- QueueFile (JSON) ---------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueFile {
    pub version: u32,

    #[serde(default)]
    pub tasks: Vec<Task>,
}

/* ------------------------------ Task (JSON) ------------------------------ */

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,

    #[serde(default)]
    pub status: TaskStatus,

    pub title: String,

    #[serde(default)]
    pub priority: TaskPriority,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub scope: Vec<String>,

    #[serde(default)]
    pub evidence: Vec<String>,

    #[serde(default)]
    pub plan: Vec<String>,

    #[serde(default)]
    pub notes: Vec<String>,

    /// Original human request that created the task (Task Builder / Scan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,

    /// Optional per-task agent override (runner/model/model_effort/iterations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<TaskAgent>,

    /// RFC3339 UTC timestamps as strings to keep the contract tool-agnostic.
    #[schemars(required)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[schemars(required)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,

    /// Task IDs that this task depends on (must be Done or Rejected before this task can run).
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Custom user-defined fields (key-value pairs for extensibility).
    #[serde(default)]
    pub custom_fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    #[default]
    Todo,
    Doing,
    Done,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Critical,
    High,
    #[default]
    Medium,
    Low,
}

// Custom PartialOrd implementation: Critical > High > Medium > Low
impl PartialOrd for TaskPriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// Custom Ord implementation: Critical > High > Medium > Low (semantically)
// Higher priority = Greater in comparison, so Critical > High > Medium > Low
impl Ord for TaskPriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare by weight: higher weight = higher priority = Greater
        self.weight().cmp(&other.weight())
    }
}

impl TaskPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskPriority::Critical => "critical",
            TaskPriority::High => "high",
            TaskPriority::Medium => "medium",
            TaskPriority::Low => "low",
        }
    }

    pub fn weight(self) -> u8 {
        match self {
            TaskPriority::Critical => 3,
            TaskPriority::High => 2,
            TaskPriority::Medium => 1,
            TaskPriority::Low => 0,
        }
    }

    /// Cycle to the next priority in ascending order, wrapping after Critical.
    pub fn cycle(self) -> Self {
        match self {
            TaskPriority::Low => TaskPriority::Medium,
            TaskPriority::Medium => TaskPriority::High,
            TaskPriority::High => TaskPriority::Critical,
            TaskPriority::Critical => TaskPriority::Low,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Todo => "todo",
            TaskStatus::Doing => "doing",
            TaskStatus::Done => "done",
            TaskStatus::Rejected => "rejected",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskAgent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<Runner>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,

    /// Per-task reasoning effort override for Codex models. Default falls back to config.
    #[serde(default, skip_serializing_if = "model_effort_is_default")]
    #[schemars(schema_with = "model_effort_schema")]
    pub model_effort: ModelEffort,

    /// Number of iterations to run for this task (overrides config).
    #[schemars(range(min = 1))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u8>,

    /// Reasoning effort override for follow-up iterations (iterations > 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followup_reasoning_effort: Option<ReasoningEffort>,
}

fn model_effort_is_default(value: &ModelEffort) -> bool {
    matches!(value, ModelEffort::Default)
}

fn model_effort_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    let mut schema = <ModelEffort as JsonSchema>::json_schema(gen);
    if let schemars::schema::Schema::Object(ref mut schema_object) = schema {
        schema_object.metadata().default = Some(json!("default"));
    }
    schema
}

/* ------------------------------ Defaults -------------------------------- */

impl Default for QueueFile {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: Vec::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            project_type: Some(ProjectType::Code),
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
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
                phases: Some(3),
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                repoprompt_plan_required: Some(false),
                repoprompt_tool_injection: Some(false),
                ci_gate_command: Some("make ci".to_string()),
                ci_gate_enabled: Some(true),
                git_revert_mode: Some(GitRevertMode::Ask),
                git_commit_push_enabled: Some(true),
                update_task_before_run: Some(false),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GitRevertMode, TaskPriority};

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
    fn task_priority_cycle_wraps_through_all_values() {
        assert_eq!(TaskPriority::Low.cycle(), TaskPriority::Medium);
        assert_eq!(TaskPriority::Medium.cycle(), TaskPriority::High);
        assert_eq!(TaskPriority::High.cycle(), TaskPriority::Critical);
        assert_eq!(TaskPriority::Critical.cycle(), TaskPriority::Low);
    }

    #[test]
    fn agent_config_merge_from_merges_update_task_before_run_leafwise() {
        let mut base = super::AgentConfig {
            update_task_before_run: Some(false),
            ..Default::default()
        };

        let other = super::AgentConfig {
            update_task_before_run: Some(true),
            ..Default::default()
        };

        base.merge_from(other);
        assert_eq!(base.update_task_before_run, Some(true));

        // None should not override an already-set value.
        base.merge_from(super::AgentConfig::default());
        assert_eq!(base.update_task_before_run, Some(true));
    }
}
