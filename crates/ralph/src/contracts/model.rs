//! Model-related configuration contracts.
//!
//! Responsibilities:
//! - Define the Model enum and model effort settings.
//! - Handle custom serialization for model identifiers.
//!
//! Not handled here:
//! - Runner definitions (see `super::runner`).
//! - Core config structs (see `super::config`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Model {
    #[default]
    Gpt53Codex,
    Gpt53CodexSpark,
    Gpt53,
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
            Model::Gpt53Codex => "gpt-5.3-codex",
            Model::Gpt53CodexSpark => "gpt-5.3-codex-spark",
            Model::Gpt53 => "gpt-5.3",
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
            "gpt-5.3-codex" => Model::Gpt53Codex,
            "gpt-5.3-codex-spark" => Model::Gpt53CodexSpark,
            "gpt-5.3" => Model::Gpt53,
            "gpt-5.2-codex" => Model::Gpt52Codex,
            "gpt-5.2" => Model::Gpt52,
            "zai-coding-plan/glm-4.7" => Model::Glm47,
            other => Model::Custom(other.to_string()),
        })
    }
}

// Manual JsonSchema implementation for Model since it has custom Serialize/Deserialize
impl schemars::JsonSchema for Model {
    fn schema_name() -> Cow<'static, str> {
        "Model".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                {
                    "type": "string",
                    "const": "gpt-5.3-codex",
                    "description": "OpenAI GPT-5.3 Codex (default)"
                },
                {
                    "type": "string",
                    "const": "gpt-5.3-codex-spark",
                    "description": "OpenAI GPT-5.3 Codex Spark (fast)"
                },
                {
                    "type": "string",
                    "const": "gpt-5.3",
                    "description": "OpenAI GPT-5.3"
                },
                {
                    "type": "string",
                    "const": "gpt-5.2-codex",
                    "description": "OpenAI GPT-5.2 Codex"
                },
                {
                    "type": "string",
                    "const": "gpt-5.2",
                    "description": "OpenAI GPT-5.2"
                },
                {
                    "type": "string",
                    "const": "zai-coding-plan/glm-4.7",
                    "description": "ZhipuAI GLM-4.7"
                },
                {
                    "type": "string",
                    "description": "Custom model identifier",
                    "minLength": 1
                }
            ]
        })
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

#[cfg(test)]
mod tests {
    use super::{Model, ModelEffort, ReasoningEffort};

    #[test]
    fn model_parses_known_variants() {
        assert_eq!("gpt-5.3-codex".parse::<Model>().unwrap(), Model::Gpt53Codex);
        assert_eq!(
            "gpt-5.3-codex-spark".parse::<Model>().unwrap(),
            Model::Gpt53CodexSpark
        );
        assert_eq!("gpt-5.3".parse::<Model>().unwrap(), Model::Gpt53);
        assert_eq!("gpt-5.2-codex".parse::<Model>().unwrap(), Model::Gpt52Codex);
        assert_eq!("gpt-5.2".parse::<Model>().unwrap(), Model::Gpt52);
        assert_eq!(
            "zai-coding-plan/glm-4.7".parse::<Model>().unwrap(),
            Model::Glm47
        );
    }

    #[test]
    fn model_parses_custom_values() {
        let custom = "claude-opus-4".parse::<Model>().unwrap();
        assert_eq!(custom, Model::Custom("claude-opus-4".to_string()));
        assert_eq!(custom.as_str(), "claude-opus-4");
    }

    #[test]
    fn model_rejects_empty_string() {
        let result = "".parse::<Model>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));
    }

    #[test]
    fn model_serializes_to_string() {
        let model = Model::Gpt53Codex;
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, "\"gpt-5.3-codex\"");

        let model = Model::Gpt53CodexSpark;
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, "\"gpt-5.3-codex-spark\"");

        let model = Model::Gpt52Codex;
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, "\"gpt-5.2-codex\"");
    }

    #[test]
    fn model_deserializes_from_string() {
        let model: Model = serde_json::from_str("\"sonnet\"").unwrap();
        assert_eq!(model, Model::Custom("sonnet".to_string()));
    }

    #[test]
    fn reasoning_effort_parses_snake_case() {
        let effort: ReasoningEffort = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(effort, ReasoningEffort::Low);
        let effort: ReasoningEffort = serde_json::from_str("\"medium\"").unwrap();
        assert_eq!(effort, ReasoningEffort::Medium);
        let effort: ReasoningEffort = serde_json::from_str("\"high\"").unwrap();
        assert_eq!(effort, ReasoningEffort::High);
        let effort: ReasoningEffort = serde_json::from_str("\"xhigh\"").unwrap();
        assert_eq!(effort, ReasoningEffort::XHigh);
    }

    #[test]
    fn model_effort_converts_to_reasoning_effort() {
        assert_eq!(ModelEffort::Default.as_reasoning_effort(), None);
        assert_eq!(
            ModelEffort::Low.as_reasoning_effort(),
            Some(ReasoningEffort::Low)
        );
        assert_eq!(
            ModelEffort::Medium.as_reasoning_effort(),
            Some(ReasoningEffort::Medium)
        );
        assert_eq!(
            ModelEffort::High.as_reasoning_effort(),
            Some(ReasoningEffort::High)
        );
        assert_eq!(
            ModelEffort::XHigh.as_reasoning_effort(),
            Some(ReasoningEffort::XHigh)
        );
    }

    #[test]
    fn model_json_schema_includes_known_models() {
        use schemars::JsonSchema;

        let schema = Model::json_schema(&mut schemars::SchemaGenerator::default());
        let schema_json = serde_json::to_string(&schema).unwrap();

        // Verify known models are in schema
        assert!(
            schema_json.contains("gpt-5.3-codex"),
            "schema should list gpt-5.3-codex"
        );
        assert!(
            schema_json.contains("gpt-5.3-codex-spark"),
            "schema should list gpt-5.3-codex-spark"
        );
        assert!(
            schema_json.contains("gpt-5.3"),
            "schema should list gpt-5.3"
        );
        assert!(
            schema_json.contains("gpt-5.2-codex"),
            "schema should list gpt-5.2-codex"
        );
        assert!(
            schema_json.contains("gpt-5.2"),
            "schema should list gpt-5.2"
        );
        assert!(
            schema_json.contains("zai-coding-plan/glm-4.7"),
            "schema should list glm-4.7"
        );

        // Verify oneOf structure
        assert!(schema_json.contains("oneOf"), "schema should use oneOf");

        // Verify custom model fallback exists
        assert!(
            schema_json.contains("Custom model identifier"),
            "schema should have custom fallback"
        );
    }
}
