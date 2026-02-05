//! Runner-related configuration contracts.
//!
//! Responsibilities:
//! - Define runner identity (`Runner`) as a string-serialized value (built-ins + plugins).
//! - Define runner CLI normalization types (approval/sandbox/plan/etc).
//!
//! Not handled here:
//! - Plugin discovery / registry (see `crate::plugins`).
//! - Runner execution dispatch (see `crate::runner`).
//!
//! Invariants/assumptions:
//! - `Runner` MUST serialize to a single string token for config/CLI stability.
//! - Unknown tokens are treated as plugin runner ids (non-empty, trimmed).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::contracts::model::{Model, ReasoningEffort};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum Runner {
    Codex,
    Opencode,
    Gemini,
    Cursor,
    #[default]
    Claude,
    Kimi,
    Pi,
    Plugin(String),
}

impl Runner {
    /// Returns the string representation of the runner.
    pub fn as_str(&self) -> &str {
        match self {
            Runner::Codex => "codex",
            Runner::Opencode => "opencode",
            Runner::Gemini => "gemini",
            Runner::Cursor => "cursor",
            Runner::Claude => "claude",
            Runner::Kimi => "kimi",
            Runner::Pi => "pi",
            Runner::Plugin(id) => id.as_str(),
        }
    }

    pub fn id(&self) -> &str {
        self.as_str()
    }

    pub fn is_plugin(&self) -> bool {
        matches!(self, Runner::Plugin(_))
    }
}

impl std::fmt::Display for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

impl std::str::FromStr for Runner {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let token = value.trim();
        if token.is_empty() {
            return Err("runner must be non-empty");
        }
        Ok(match token.to_lowercase().as_str() {
            "codex" => Runner::Codex,
            "opencode" => Runner::Opencode,
            "gemini" => Runner::Gemini,
            "cursor" => Runner::Cursor,
            "claude" => Runner::Claude,
            "kimi" => Runner::Kimi,
            "pi" => Runner::Pi,
            _ => Runner::Plugin(token.to_string()),
        })
    }
}

// Keep config/CLI stable: serialize as a single string token.
impl Serialize for Runner {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for Runner {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse::<Runner>().map_err(serde::de::Error::custom)
    }
}

// Schema: treat as string; docs enumerate built-ins, but allow arbitrary plugin ids.
impl JsonSchema for Runner {
    fn schema_name() -> String {
        "Runner".to_string()
    }

    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = <String as JsonSchema>::json_schema(generator);
        if let schemars::schema::Schema::Object(obj) = &mut schema {
            obj.metadata().description.get_or_insert_with(|| {
                "Runner id (built-ins: codex, opencode, gemini, cursor, claude, kimi, pi; \
                     plugin runners: any other non-empty string)"
                    .to_string()
            });
            // Set examples for the schema
            obj.metadata().examples = vec![
                serde_json::Value::String("claude".to_string()),
                serde_json::Value::String("acme.super_runner".to_string()),
            ];
        }
        schema
    }
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
pub enum RunnerOutputFormat {
    /// Newline-delimited JSON objects (required for Ralph's streaming parser).
    #[default]
    StreamJson,
    /// JSON output (may not be streaming; currently treated as unsupported by Ralph execution).
    Json,
    /// Plain text output (currently treated as unsupported by Ralph execution).
    Text,
}

impl std::str::FromStr for RunnerOutputFormat {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_token(value).as_str() {
            "stream_json" => Ok(RunnerOutputFormat::StreamJson),
            "json" => Ok(RunnerOutputFormat::Json),
            "text" => Ok(RunnerOutputFormat::Text),
            _ => Err("output_format must be 'stream_json', 'json', or 'text'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerVerbosity {
    Quiet,
    #[default]
    Normal,
    Verbose,
}

impl std::str::FromStr for RunnerVerbosity {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_token(value).as_str() {
            "quiet" => Ok(RunnerVerbosity::Quiet),
            "normal" => Ok(RunnerVerbosity::Normal),
            "verbose" => Ok(RunnerVerbosity::Verbose),
            _ => Err("verbosity must be 'quiet', 'normal', or 'verbose'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerApprovalMode {
    /// Do not apply any approval flags; runner defaults apply.
    Default,
    /// Attempt to auto-approve edits but not all tool actions (runner-specific).
    AutoEdits,
    /// Bypass approvals / run headless (runner-specific).
    #[default]
    Yolo,
    /// Strict safety mode. Warning: some runners may become interactive and hang.
    Safe,
}

impl std::str::FromStr for RunnerApprovalMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_token(value).as_str() {
            "default" => Ok(RunnerApprovalMode::Default),
            "auto_edits" => Ok(RunnerApprovalMode::AutoEdits),
            "yolo" => Ok(RunnerApprovalMode::Yolo),
            "safe" => Ok(RunnerApprovalMode::Safe),
            _ => Err("approval_mode must be 'default', 'auto_edits', 'yolo', or 'safe'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerSandboxMode {
    #[default]
    Default,
    Enabled,
    Disabled,
}

impl std::str::FromStr for RunnerSandboxMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_token(value).as_str() {
            "default" => Ok(RunnerSandboxMode::Default),
            "enabled" => Ok(RunnerSandboxMode::Enabled),
            "disabled" => Ok(RunnerSandboxMode::Disabled),
            _ => Err("sandbox must be 'default', 'enabled', or 'disabled'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerPlanMode {
    #[default]
    Default,
    Enabled,
    Disabled,
}

impl std::str::FromStr for RunnerPlanMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_token(value).as_str() {
            "default" => Ok(RunnerPlanMode::Default),
            "enabled" => Ok(RunnerPlanMode::Enabled),
            "disabled" => Ok(RunnerPlanMode::Disabled),
            _ => Err("plan_mode must be 'default', 'enabled', or 'disabled'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedOptionPolicy {
    Ignore,
    #[default]
    Warn,
    Error,
}

impl std::str::FromStr for UnsupportedOptionPolicy {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_enum_token(value).as_str() {
            "ignore" => Ok(UnsupportedOptionPolicy::Ignore),
            "warn" => Ok(UnsupportedOptionPolicy::Warn),
            "error" => Ok(UnsupportedOptionPolicy::Error),
            _ => Err("unsupported_option_policy must be 'ignore', 'warn', or 'error'"),
        }
    }
}

fn normalize_enum_token(value: &str) -> String {
    value.trim().to_lowercase().replace('-', "_")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RunnerCliConfigRoot {
    /// Default normalized runner CLI options applied to all runners (unless overridden).
    pub defaults: RunnerCliOptionsPatch,

    /// Optional per-runner overrides, merged leaf-wise over `defaults`.
    pub runners: BTreeMap<Runner, RunnerCliOptionsPatch>,
}

impl RunnerCliConfigRoot {
    pub fn merge_from(&mut self, other: Self) {
        self.defaults.merge_from(other.defaults);
        for (runner, patch) in other.runners {
            self.runners
                .entry(runner)
                .and_modify(|existing| existing.merge_from(patch.clone()))
                .or_insert(patch);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RunnerCliOptionsPatch {
    /// Desired output format for runner execution.
    pub output_format: Option<RunnerOutputFormat>,

    /// Desired verbosity (when supported by the runner).
    pub verbosity: Option<RunnerVerbosity>,

    /// Desired approval/permission behavior.
    pub approval_mode: Option<RunnerApprovalMode>,

    /// Desired sandbox behavior (when supported by the runner).
    pub sandbox: Option<RunnerSandboxMode>,

    /// Desired plan/read-only behavior (when supported by the runner).
    pub plan_mode: Option<RunnerPlanMode>,

    /// Policy for unsupported options (warn/error/ignore).
    pub unsupported_option_policy: Option<UnsupportedOptionPolicy>,
}

impl RunnerCliOptionsPatch {
    pub fn merge_from(&mut self, other: Self) {
        if other.output_format.is_some() {
            self.output_format = other.output_format;
        }
        if other.verbosity.is_some() {
            self.verbosity = other.verbosity;
        }
        if other.approval_mode.is_some() {
            self.approval_mode = other.approval_mode;
        }
        if other.sandbox.is_some() {
            self.sandbox = other.sandbox;
        }
        if other.plan_mode.is_some() {
            self.plan_mode = other.plan_mode;
        }
        if other.unsupported_option_policy.is_some() {
            self.unsupported_option_policy = other.unsupported_option_policy;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct MergeRunnerConfig {
    pub runner: Option<Runner>,
    pub model: Option<Model>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl MergeRunnerConfig {
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

#[cfg(test)]
mod tests {
    use super::{
        Runner, RunnerApprovalMode, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
        RunnerVerbosity, UnsupportedOptionPolicy,
    };

    #[test]
    fn runner_cli_enums_from_str_accept_hyphenated_tokens() {
        assert_eq!(
            "stream-json".parse::<RunnerOutputFormat>().unwrap(),
            RunnerOutputFormat::StreamJson
        );
        assert_eq!(
            "auto-edits".parse::<RunnerApprovalMode>().unwrap(),
            RunnerApprovalMode::AutoEdits
        );
        assert_eq!(
            "verbose".parse::<RunnerVerbosity>().unwrap(),
            RunnerVerbosity::Verbose
        );
        assert_eq!(
            "disabled".parse::<RunnerSandboxMode>().unwrap(),
            RunnerSandboxMode::Disabled
        );
        assert_eq!(
            "enabled".parse::<RunnerPlanMode>().unwrap(),
            RunnerPlanMode::Enabled
        );
        assert_eq!(
            "error".parse::<UnsupportedOptionPolicy>().unwrap(),
            UnsupportedOptionPolicy::Error
        );
    }

    #[test]
    fn runner_parses_built_ins() {
        assert_eq!("codex".parse::<Runner>().unwrap(), Runner::Codex);
        assert_eq!("opencode".parse::<Runner>().unwrap(), Runner::Opencode);
        assert_eq!("gemini".parse::<Runner>().unwrap(), Runner::Gemini);
        assert_eq!("cursor".parse::<Runner>().unwrap(), Runner::Cursor);
        assert_eq!("claude".parse::<Runner>().unwrap(), Runner::Claude);
        assert_eq!("kimi".parse::<Runner>().unwrap(), Runner::Kimi);
        assert_eq!("pi".parse::<Runner>().unwrap(), Runner::Pi);
    }

    #[test]
    fn runner_parses_plugin_id() {
        assert_eq!(
            "acme.super_runner".parse::<Runner>().unwrap(),
            Runner::Plugin("acme.super_runner".to_string())
        );
        assert_eq!(
            "my-custom-runner".parse::<Runner>().unwrap(),
            Runner::Plugin("my-custom-runner".to_string())
        );
    }

    #[test]
    fn runner_rejects_empty() {
        assert!("".parse::<Runner>().is_err());
        assert!("   ".parse::<Runner>().is_err());
    }

    #[test]
    fn runner_serde_roundtrip_is_string() {
        let runner = Runner::Plugin("acme.runner".to_string());
        let json = serde_json::to_string(&runner).unwrap();
        assert_eq!(json, "\"acme.runner\"");
        let back: Runner = serde_json::from_str(&json).unwrap();
        assert_eq!(runner, back);
    }

    #[test]
    fn runner_built_in_serde_roundtrip() {
        let runner = Runner::Claude;
        let json = serde_json::to_string(&runner).unwrap();
        assert_eq!(json, "\"claude\"");
        let back: Runner = serde_json::from_str(&json).unwrap();
        assert_eq!(runner, back);
    }

    #[test]
    fn runner_display_uses_id() {
        assert_eq!(Runner::Codex.to_string(), "codex");
        assert_eq!(
            Runner::Plugin("custom.runner".to_string()).to_string(),
            "custom.runner"
        );
    }

    #[test]
    fn runner_is_plugin_detects_plugin_variant() {
        assert!(!Runner::Codex.is_plugin());
        assert!(!Runner::Claude.is_plugin());
        assert!(Runner::Plugin("x".to_string()).is_plugin());
    }
}
