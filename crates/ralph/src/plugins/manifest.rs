//! Plugin manifest schema and validation.
//!
//! Responsibilities:
//! - Define `PluginManifest` and validate required invariants.
//!
//! Not handled here:
//! - Filesystem discovery (see `discovery`).
//! - Enable/disable policy (see `registry`).

use std::path::{Component, Path};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::plugins::PLUGIN_API_VERSION;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginManifest {
    pub api_version: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<RunnerPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processors: Option<ProcessorPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunnerPlugin {
    /// Path to runner executable, relative to the plugin directory.
    pub bin: String,

    /// If false or omitted, `resume` will be rejected by Ralph for this runner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_resume: Option<bool>,

    /// Default model when none is specified anywhere (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessorPlugin {
    /// Path to processor executable, relative to the plugin directory.
    pub bin: String,

    /// Supported hooks. Valid values: validate_task, pre_prompt, post_run
    pub hooks: Vec<String>,
}

impl PluginManifest {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        if self.api_version != PLUGIN_API_VERSION {
            anyhow::bail!(
                "plugin api_version mismatch: got {}, expected {}",
                self.api_version,
                PLUGIN_API_VERSION
            );
        }
        if self.id.trim().is_empty() {
            anyhow::bail!("plugin id must be non-empty");
        }
        if self.id.contains('/') || self.id.contains('\\') {
            anyhow::bail!("plugin id must not contain path separators");
        }
        if let Some(runner) = &self.runner {
            validate_plugin_bin(&runner.bin, "runner.bin")?;
        }
        if let Some(proc) = &self.processors {
            validate_plugin_bin(&proc.bin, "processors.bin")?;
            for hook in &proc.hooks {
                match hook.as_str() {
                    "validate_task" | "pre_prompt" | "post_run" => {}
                    other => anyhow::bail!("unsupported processor hook: {other}"),
                }
            }
        }
        Ok(())
    }
}

fn validate_plugin_bin(bin: &str, field: &str) -> anyhow::Result<()> {
    if bin.trim().is_empty() {
        anyhow::bail!("plugin {field} must be non-empty");
    }

    let path = Path::new(bin);
    if path.is_absolute() {
        anyhow::bail!("plugin {field} must be relative to the plugin directory");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        anyhow::bail!("plugin {field} must not contain '..'");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> PluginManifest {
        PluginManifest {
            api_version: PLUGIN_API_VERSION,
            id: "test.plugin".to_string(),
            version: "1.0.0".to_string(),
            name: "Test Plugin".to_string(),
            description: Some("A test plugin".to_string()),
            runner: Some(RunnerPlugin {
                bin: "runner.sh".to_string(),
                supports_resume: Some(true),
                default_model: None,
            }),
            processors: None,
        }
    }

    #[test]
    fn validate_accepts_valid_manifest() {
        let m = valid_manifest();
        assert!(m.validate().is_ok());
    }

    #[test]
    fn validate_rejects_wrong_api_version() {
        let mut m = valid_manifest();
        m.api_version = 999;
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("api_version"));
    }

    #[test]
    fn validate_rejects_empty_id() {
        let mut m = valid_manifest();
        m.id = "".to_string();
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn validate_rejects_path_separator_in_id() {
        let mut m = valid_manifest();
        m.id = "foo/bar".to_string();
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("path"));

        m.id = "foo\\bar".to_string();
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("path"));
    }

    #[test]
    fn validate_accepts_supported_hooks() {
        let m = PluginManifest {
            processors: Some(ProcessorPlugin {
                bin: "proc.sh".to_string(),
                hooks: vec![
                    "validate_task".to_string(),
                    "pre_prompt".to_string(),
                    "post_run".to_string(),
                ],
            }),
            ..valid_manifest()
        };
        assert!(m.validate().is_ok());
    }

    #[test]
    fn validate_rejects_unsupported_hook() {
        let m = PluginManifest {
            processors: Some(ProcessorPlugin {
                bin: "proc.sh".to_string(),
                hooks: vec!["unknown_hook".to_string()],
            }),
            ..valid_manifest()
        };
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn validate_rejects_absolute_runner_bin() {
        let mut m = valid_manifest();
        m.runner.as_mut().unwrap().bin = "/tmp/runner.sh".to_string();
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("relative"));
    }

    #[test]
    fn validate_rejects_parent_dir_processor_bin() {
        let m = PluginManifest {
            processors: Some(ProcessorPlugin {
                bin: "../proc.sh".to_string(),
                hooks: vec!["pre_prompt".to_string()],
            }),
            ..valid_manifest()
        };
        let err = m.validate().unwrap_err();
        assert!(err.to_string().contains("must not contain '..'"));
    }
}
