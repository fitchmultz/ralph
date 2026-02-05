//! Runner model defaults, normalization, validation, and parsing.
//!
//! Responsibilities:
//! - Provide per-runner default models.
//! - Normalize models when a model is incompatible with a selected runner.
//! - Validate runner/model compatibility (notably Codex restrictions).
//! - Parse CLI/config string values into `Model` and `ReasoningEffort`.
//!
//! Does not handle:
//! - Runner execution dispatch (see `runner.rs`).
//! - CLI option resolution (see `runner/execution/cli_options.rs`).
//!
//! Assumptions/invariants:
//! - Codex runner only supports `gpt-5.2-codex` and `gpt-5.2`.
//! - Non-Codex runners must never "inherit" Codex-only defaults.

use anyhow::{Result, anyhow, bail};

use crate::constants::defaults::{
    DEFAULT_CLAUDE_MODEL, DEFAULT_CURSOR_MODEL, DEFAULT_GEMINI_MODEL,
};
use crate::contracts::{Model, ReasoningEffort, Runner};

pub(crate) fn default_model_for_runner(runner: &Runner) -> Model {
    match runner {
        Runner::Codex => Model::Gpt53Codex,
        Runner::Opencode => Model::Glm47,
        Runner::Gemini => Model::Custom(DEFAULT_GEMINI_MODEL.to_string()),
        Runner::Cursor => Model::Custom(DEFAULT_CURSOR_MODEL.to_string()),
        Runner::Claude => Model::Custom(DEFAULT_CLAUDE_MODEL.to_string()),
        Runner::Kimi => Model::Custom("kimi-for-coding".to_string()),
        Runner::Pi => Model::Custom("gpt-5.3".to_string()),
        Runner::Plugin(_) => Model::Custom("gpt-5.3".to_string()),
    }
}

pub(crate) fn resolve_model_for_runner(
    runner: &Runner,
    override_model: Option<Model>,
    task_model: Option<Model>,
    config_model: Option<Model>,
    runner_was_overridden: bool,
) -> Model {
    if let Some(model) = override_model {
        return model;
    }
    if let Some(model) = task_model {
        return normalize_model_for_runner(runner, model);
    }

    if runner_was_overridden {
        return default_model_for_runner(runner);
    }

    match config_model {
        None => default_model_for_runner(runner),
        Some(model) => normalize_model_for_runner(runner, model),
    }
}

pub(super) fn resolve_model_for_phase(
    runner: &Runner,
    cli_phase_model: Option<Model>,
    config_phase_model: Option<Model>,
    cli_global_model: Option<Model>,
    task_model: Option<Model>,
    config_model: Option<Model>,
    runner_was_overridden: bool,
) -> Model {
    if let Some(model) = cli_phase_model {
        return model;
    }
    if let Some(model) = config_phase_model {
        return normalize_model_for_runner(runner, model);
    }
    if let Some(model) = cli_global_model {
        return model;
    }
    if let Some(model) = task_model {
        return normalize_model_for_runner(runner, model);
    }

    if runner_was_overridden {
        return default_model_for_runner(runner);
    }

    match config_model {
        None => default_model_for_runner(runner),
        Some(model) => normalize_model_for_runner(runner, model),
    }
}

fn normalize_model_for_runner(runner: &Runner, model: Model) -> Model {
    if runner == &Runner::Codex {
        match model {
            Model::Gpt53Codex | Model::Gpt53 | Model::Gpt52Codex | Model::Gpt52 => model,
            _ => default_model_for_runner(runner),
        }
    } else if matches!(model, Model::Gpt53Codex | Model::Gpt52Codex) {
        default_model_for_runner(runner)
    } else {
        model
    }
}

pub(crate) fn validate_model_for_runner(runner: &Runner, model: &Model) -> Result<()> {
    if runner == &Runner::Codex {
        match model {
            Model::Gpt53Codex | Model::Gpt53 | Model::Gpt52Codex | Model::Gpt52 => {}
            Model::Glm47 => {
                bail!("model zai-coding-plan/glm-4.7 is not supported for codex runner")
            }
            Model::Custom(name) => bail!(
                "model {} is not supported for codex runner (allowed: gpt-5.3-codex, gpt-5.3, gpt-5.2-codex, gpt-5.2)",
                name
            ),
        }
    }
    Ok(())
}

pub(crate) fn parse_model(value: &str) -> Result<Model> {
    let trimmed = value.trim();
    let model = trimmed.parse::<Model>().map_err(|err| anyhow!(err))?;
    Ok(model)
}

pub(crate) fn parse_reasoning_effort(value: &str) -> Result<ReasoningEffort> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "low" => Ok(ReasoningEffort::Low),
        "medium" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        "xhigh" => Ok(ReasoningEffort::XHigh),
        _ => bail!(
            "unsupported reasoning effort: {} (allowed: low, medium, high, xhigh)",
            value.trim()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_model_for_runner_rejects_glm47_on_codex() {
        let err = validate_model_for_runner(&Runner::Codex, &Model::Glm47).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("zai-coding-plan/glm-4.7"));
    }

    #[test]
    fn validate_model_for_runner_rejects_custom_on_codex() {
        let model = Model::Custom("gemini-3-pro-preview".to_string());
        let err = validate_model_for_runner(&Runner::Codex, &model).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("gemini-3-pro-preview"));
        assert!(msg.contains("gpt-5.3-codex"));
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_gemini() {
        let model = resolve_model_for_runner(&Runner::Gemini, None, None, None, false);
        assert_eq!(model.as_str(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_replaces_codex_default_for_gemini() {
        let model =
            resolve_model_for_runner(&Runner::Gemini, None, None, Some(Model::Gpt52Codex), false);
        assert_eq!(model.as_str(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_codex_when_config_incompatible() {
        let model = resolve_model_for_runner(
            &Runner::Codex,
            None,
            None,
            Some(Model::Custom("sonnet".to_string())),
            false,
        );
        assert_eq!(model, Model::Gpt53Codex);
    }

    #[test]
    fn resolve_model_for_runner_normalizes_task_model_for_codex() {
        let model = resolve_model_for_runner(
            &Runner::Codex,
            None,
            Some(Model::Custom("sonnet".to_string())),
            None,
            false,
        );
        assert_eq!(model, Model::Gpt53Codex);
    }

    #[test]
    fn resolve_model_for_runner_normalizes_task_model_for_opencode() {
        let model = resolve_model_for_runner(
            &Runner::Opencode,
            None,
            Some(Model::Gpt52Codex),
            None,
            false,
        );
        assert_eq!(model, Model::Glm47);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_claude() {
        let model = resolve_model_for_runner(&Runner::Claude, None, None, None, false);
        assert_eq!(model.as_str(), DEFAULT_CLAUDE_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_cursor() {
        let model = resolve_model_for_runner(&Runner::Cursor, None, None, None, false);
        assert_eq!(model.as_str(), DEFAULT_CURSOR_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_kimi() {
        let model = resolve_model_for_runner(&Runner::Kimi, None, None, None, false);
        assert_eq!(model.as_str(), "kimi-for-coding");
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_pi() {
        let model = resolve_model_for_runner(&Runner::Pi, None, None, None, false);
        assert_eq!(model.as_str(), "gpt-5.3");
    }

    #[test]
    fn parse_reasoning_effort_accepts_xhigh() {
        let effort = parse_reasoning_effort(" xhigh ").expect("xhigh effort");
        assert_eq!(effort, ReasoningEffort::XHigh);
    }

    #[test]
    fn parse_reasoning_effort_rejects_minimal() {
        let err = parse_reasoning_effort("minimal").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("allowed: low, medium, high, xhigh"));
    }

    #[test]
    fn resolve_model_for_runner_override_uses_runner_default_when_no_model() {
        let model = resolve_model_for_runner(
            &Runner::Opencode,
            None,
            None,
            Some(Model::Custom("sonnet".to_string())),
            true,
        );
        assert_eq!(model, Model::Glm47);
    }

    #[test]
    fn resolve_model_for_runner_override_with_explicit_model() {
        let model = resolve_model_for_runner(
            &Runner::Opencode,
            Some(Model::Gpt52),
            None,
            Some(Model::Custom("sonnet".to_string())),
            true,
        );
        assert_eq!(model, Model::Gpt52);
    }

    #[test]
    fn resolve_model_for_runner_no_override_uses_config_model() {
        let model =
            resolve_model_for_runner(&Runner::Codex, None, None, Some(Model::Gpt53Codex), false);
        assert_eq!(model, Model::Gpt53Codex);
    }

    #[test]
    fn validate_model_for_runner_accepts_gpt53_for_codex() {
        assert!(validate_model_for_runner(&Runner::Codex, &Model::Gpt53Codex).is_ok());
        assert!(validate_model_for_runner(&Runner::Codex, &Model::Gpt53).is_ok());
    }

    #[test]
    fn validate_model_for_runner_accepts_gpt52_for_codex() {
        // Keep compatibility test for GPT-5.2 models
        assert!(validate_model_for_runner(&Runner::Codex, &Model::Gpt52Codex).is_ok());
        assert!(validate_model_for_runner(&Runner::Codex, &Model::Gpt52).is_ok());
    }
}
