//! Runner list command.
//!
//! Responsibilities:
//! - List all available runners with brief descriptions.

use anyhow::Result;
use serde::Serialize;

use crate::cli::runner::RunnerFormat;
use crate::contracts::Runner;
use crate::runner::default_model_for_runner;

#[derive(Debug, Clone, Serialize)]
pub struct RunnerInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub default_model: String,
}

fn get_all_runners() -> Vec<RunnerInfo> {
    vec![
        RunnerInfo {
            id: "claude".into(),
            name: "Anthropic Claude Code".into(),
            provider: "Anthropic".into(),
            default_model: default_model_for_runner(&Runner::Claude)
                .as_str()
                .to_string(),
        },
        RunnerInfo {
            id: "codex".into(),
            name: "OpenAI Codex CLI".into(),
            provider: "OpenAI".into(),
            default_model: default_model_for_runner(&Runner::Codex)
                .as_str()
                .to_string(),
        },
        RunnerInfo {
            id: "opencode".into(),
            name: "Opencode".into(),
            provider: "Flexible".into(),
            default_model: default_model_for_runner(&Runner::Opencode)
                .as_str()
                .to_string(),
        },
        RunnerInfo {
            id: "gemini".into(),
            name: "Google Gemini CLI".into(),
            provider: "Google".into(),
            default_model: default_model_for_runner(&Runner::Gemini)
                .as_str()
                .to_string(),
        },
        RunnerInfo {
            id: "cursor".into(),
            name: "Cursor Agent".into(),
            provider: "Cursor".into(),
            default_model: default_model_for_runner(&Runner::Cursor)
                .as_str()
                .to_string(),
        },
        RunnerInfo {
            id: "kimi".into(),
            name: "Kimi CLI".into(),
            provider: "Moonshot AI".into(),
            default_model: default_model_for_runner(&Runner::Kimi).as_str().to_string(),
        },
        RunnerInfo {
            id: "pi".into(),
            name: "Pi Coding Agent".into(),
            provider: "Pi".into(),
            default_model: default_model_for_runner(&Runner::Pi).as_str().to_string(),
        },
    ]
}

pub fn handle_list(format: RunnerFormat) -> Result<()> {
    let runners = get_all_runners();

    match format {
        RunnerFormat::Text => {
            println!("Available runners:\n");
            for r in &runners {
                println!("  {:12} {} (default: {})", r.id, r.name, r.default_model);
            }
            println!("\nUse 'ralph runner capabilities <id>' for details.");
        }
        RunnerFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&runners)?);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_all_runners_returns_all_built_ins() {
        let runners = get_all_runners();
        assert_eq!(runners.len(), 7);

        let ids: Vec<_> = runners.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"claude"));
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"gemini"));
        assert!(ids.contains(&"cursor"));
        assert!(ids.contains(&"kimi"));
        assert!(ids.contains(&"pi"));
    }

    #[test]
    fn runner_info_has_required_fields() {
        let runners = get_all_runners();
        for r in &runners {
            assert!(!r.id.is_empty(), "runner {} has empty id", r.name);
            assert!(!r.name.is_empty(), "runner {} has empty name", r.id);
            assert!(!r.provider.is_empty(), "runner {} has empty provider", r.id);
            assert!(
                !r.default_model.is_empty(),
                "runner {} has empty default_model",
                r.id
            );
        }
    }
}
