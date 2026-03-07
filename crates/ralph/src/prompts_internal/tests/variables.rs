//! Variable expansion and placeholder validation tests.
//!
//! Responsibilities: validate environment variable expansion, config variable expansion,
//! and placeholder detection.
//! Not handled: prompt rendering or file loading.
//! Invariants/assumptions: env vars with RALPH_TEST_ prefix are safe to manipulate;
//! config paths follow dot notation.

use super::*;

#[test]
fn ensure_no_unresolved_placeholders_passes_when_none_remain() -> Result<()> {
    let rendered = "Hello world";
    assert!(ensure_no_unresolved_placeholders(rendered, "test").is_ok());
    Ok(())
}

#[test]
fn ensure_no_unresolved_placeholders_fails_with_placeholder() -> Result<()> {
    let rendered = "Hello {{MISSING}} world";
    let err = ensure_no_unresolved_placeholders(rendered, "test").unwrap_err();
    assert!(err.to_string().contains("MISSING"));
    assert!(err.to_string().contains("unresolved placeholders"));
    Ok(())
}

#[test]
fn unresolved_placeholders_finds_all_placeholders() {
    let rendered = "Test {{ONE}} and {{TWO}} and {{three}}";
    let placeholders = unresolved_placeholders(rendered);
    assert_eq!(placeholders.len(), 3);
    assert!(placeholders.contains(&"ONE".to_string()));
    assert!(placeholders.contains(&"TWO".to_string()));
    assert!(placeholders.contains(&"THREE".to_string()));
}

#[test]
fn unresolved_placeholders_returns_sorted_unique() {
    let rendered = "Test {{Z}} and {{A}} and {{B}} and {{A}}";
    let placeholders = unresolved_placeholders(rendered);
    assert_eq!(placeholders, vec!["A", "B", "Z"]);
}

#[test]
fn expand_variables_expands_env_var_with_default() -> Result<()> {
    let var_name = "RALPH_TEST_DEFAULT_VAR";
    unsafe { std::env::remove_var(var_name) };
    let template = format!("Value: ${{{}:-default_value}}", var_name);
    let config = default_config();
    let result = expand_variables(&template, &config)?;
    assert_eq!(result, "Value: default_value");
    Ok(())
}

#[test]
fn expand_variables_expands_env_var_when_set() -> Result<()> {
    let var_name = "RALPH_TEST_SET_VAR";
    let template = format!("Value: ${{{}:-default}}", var_name);
    let config = default_config();
    unsafe { std::env::set_var(var_name, "actual_value") };
    let result = expand_variables(&template, &config)?;
    unsafe { std::env::remove_var(var_name) };
    assert_eq!(result, "Value: actual_value");
    Ok(())
}

#[test]
fn expand_variables_leaves_missing_env_var_literal() -> Result<()> {
    let template = "Value: ${MISSING_VAR}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("${MISSING_VAR}"));
    Ok(())
}

#[test]
fn expand_variables_handles_dollar_escape() -> Result<()> {
    let template = "Literal: $${ESCAPED}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert_eq!(result, "Literal: ${ESCAPED}");
    Ok(())
}

#[test]
fn expand_variables_handles_backslash_escape() -> Result<()> {
    let template = "Literal: \\${ESCAPED}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert_eq!(result, "Literal: ${ESCAPED}");
    Ok(())
}

#[test]
fn expand_variables_expands_config_runner() -> Result<()> {
    let template = "Runner: {{config.agent.runner}}";
    let mut config = default_config();
    config.agent.runner = Some(crate::contracts::Runner::Claude);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Runner: Claude"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_model() -> Result<()> {
    let template = "Model: {{config.agent.model}}";
    let mut config = default_config();
    config.agent.model = Some(crate::contracts::Model::Gpt52Codex);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("gpt-5.2-codex"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_queue_id_prefix() -> Result<()> {
    let template = "Prefix: {{config.queue.id_prefix}}";
    let mut config = default_config();
    config.queue.id_prefix = Some("TASK".to_string());
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Prefix: TASK"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_queue_file() -> Result<()> {
    let template = "Queue file: {{config.queue.file}}";
    let mut config = default_config();
    config.queue.file = Some(std::path::PathBuf::from(".ralph/custom_queue.jsonc"));
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Queue file: .ralph/custom_queue.jsonc"));
    Ok(())
}

#[test]
fn expand_variables_uses_default_config_queue_file_when_unset() -> Result<()> {
    let template = "Queue file: {{config.queue.file}}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Queue file: .ralph/queue.jsonc"));
    assert!(!result.contains("{{config.queue.file}}"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_done_file() -> Result<()> {
    let template = "Done file: {{config.queue.done_file}}";
    let mut config = default_config();
    config.queue.done_file = Some(std::path::PathBuf::from(".ralph/custom_done.jsonc"));
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Done file: .ralph/custom_done.jsonc"));
    Ok(())
}

#[test]
fn expand_variables_uses_default_config_done_file_when_unset() -> Result<()> {
    let template = "Done file: {{config.queue.done_file}}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Done file: .ralph/done.jsonc"));
    assert!(!result.contains("{{config.queue.done_file}}"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_ci_gate_display() -> Result<()> {
    let template = "CI: {{config.agent.ci_gate_display}}";
    let mut config = default_config();
    config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(true),
        argv: Some(vec!["cargo".to_string(), "test".to_string()]),
        shell: None,
    });
    let result = expand_variables(template, &config)?;
    assert!(result.contains("CI: cargo test"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_ci_gate_enabled() -> Result<()> {
    let template = "Enabled: {{config.agent.ci_gate_enabled}}";
    let mut config = default_config();
    config.agent.ci_gate = Some(crate::contracts::CiGateConfig {
        enabled: Some(false),
        argv: None,
        shell: None,
    });
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Enabled: false"));
    Ok(())
}

#[test]
fn expand_variables_expands_git_commit_push_enabled() -> Result<()> {
    let template = "Git commit/push: {{config.agent.git_commit_push_enabled}}";
    let mut config = default_config();
    config.agent.git_commit_push_enabled = Some(false);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Git commit/push: false"));
    Ok(())
}

#[test]
fn expand_variables_leaves_non_config_placeholders() -> Result<()> {
    let template = "Request: {{USER_REQUEST}}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("{{USER_REQUEST}}"));
    Ok(())
}

#[test]
fn expand_variables_mixed_env_and_config() -> Result<()> {
    let template = "Model: {{config.agent.model}}, Var: ${TEST:-default}";
    let mut config = default_config();
    config.agent.model = Some(crate::contracts::Model::Gpt52Codex);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("gpt-5.2-codex"));
    assert!(result.contains("Var: default"));
    Ok(())
}

#[test]
fn expand_variables_invalid_config_path_left_literal() -> Result<()> {
    let template = "Value: {{config.invalid.path}}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("{{config.invalid.path}}"));
    Ok(())
}
