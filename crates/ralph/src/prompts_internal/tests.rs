//! Prompt loading, rendering, and validation tests.

use super::*;
use crate::contracts::{Config, ProjectType};
use std::fs;
use tempfile::TempDir;

fn default_config() -> Config {
    Config::default()
}

#[test]
fn render_worker_prompt_replaces_interactive_instructions() -> Result<()> {
    let template = "Hello\n{{INTERACTIVE_INSTRUCTIONS}}\n";
    let config = default_config();
    let rendered = render_worker_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(!rendered.contains("{{INTERACTIVE_INSTRUCTIONS}}"));
    Ok(())
}

#[test]
fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
    let template = "FOCUS:\n{{USER_FOCUS}}\n";
    let config = default_config();
    let rendered = render_scan_prompt(template, "hello world", ProjectType::Code, &config)?;
    assert!(rendered.contains("hello world"));
    assert!(!rendered.contains("{{USER_FOCUS}}"));
    Ok(())
}

#[test]
fn render_task_builder_prompt_replaces_placeholders() -> Result<()> {
    let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let config = default_config();
    let rendered = render_task_builder_prompt(
        template,
        "do thing",
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("do thing"));
    assert!(rendered.contains("code"));
    assert!(rendered.contains("repo"));
    assert!(!rendered.contains("{{USER_REQUEST}}"));
    Ok(())
}

#[test]
fn load_worker_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_prompt(dir.path())?;
    assert!(prompt.contains("# MISSION"));
    Ok(())
}

#[test]
fn load_worker_phase1_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase1_prompt(dir.path())?;
    assert!(prompt.contains("# PLANNING MODE"));
    Ok(())
}

#[test]
fn load_worker_phase2_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase2_prompt(dir.path())?;
    assert!(prompt.contains("# IMPLEMENTATION MODE"));
    Ok(())
}

#[test]
fn load_worker_phase3_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase3_prompt(dir.path())?;
    assert!(prompt.contains("# CODE REVIEW MODE"));
    Ok(())
}

#[test]
fn load_worker_single_phase_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_single_phase_prompt(dir.path())?;
    assert!(prompt.contains("single-pass execution mode"));
    Ok(())
}

#[test]
fn default_task_builder_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_task_builder_prompt(dir.path())?;
    assert!(prompt.contains("ralph queue next-id"));
    assert!(!prompt.contains("ralph queue next` for each new task ID"));
    Ok(())
}

#[test]
fn default_scan_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_scan_prompt(dir.path())?;
    assert!(prompt.contains("ralph queue next-id"));
    assert!(!prompt.contains("ralph queue next` for each new task ID"));
    Ok(())
}

#[test]
fn default_worker_prompt_excludes_completion_checklist() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_prompt(dir.path())?;
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    assert!(!prompt.contains("END-OF-TURN CHECKLIST"));
    Ok(())
}

#[test]
fn load_completion_checklist_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let checklist = load_completion_checklist(dir.path())?;
    assert!(checklist.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    Ok(())
}

#[test]
fn load_completion_checklist_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("completion_checklist.md"), "override")?;
    let checklist = load_completion_checklist(dir.path())?;
    assert_eq!(checklist, "override");
    Ok(())
}

#[test]
fn load_worker_prompt_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("worker.md"), "override")?;
    let prompt = load_worker_prompt(dir.path())?;
    assert_eq!(prompt, "override");
    Ok(())
}

#[test]
fn render_worker_phase1_prompt_replaces_placeholders() -> Result<()> {
    let template =
        "ID={{TASK_ID}}\nPHASE={{TOTAL_PHASES}}\nPLAN={{PLAN_PATH}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let rendered = render_worker_phase1_prompt(
        template,
        "BASE",
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        true,
        &config,
    )?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PHASE=2"));
    assert!(rendered.contains("PLAN=.ralph/cache/plans/RQ-0001.md"));
    assert!(rendered.contains("BASE"));
    assert!(rendered.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(rendered.contains("PLANNING REQUIREMENT"));
    assert!(!rendered.contains("{{"));
    Ok(())
}

#[test]
fn render_worker_phase2_prompt_skips_repoprompt_when_not_required() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PLAN_TEXT}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let rendered = render_worker_phase2_prompt(
        template,
        "BASE",
        "PLAN",
        "CHECKLIST",
        "RQ-0001",
        2,
        false,
        &config,
    )?;
    assert!(rendered.contains("PHASE=2"));
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PLAN"));
    assert!(rendered.contains("CHECKLIST"));
    assert!(rendered.contains("BASE"));
    assert!(!rendered.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!rendered.contains("{{"));
    Ok(())
}

#[test]
fn render_worker_phase3_prompt_includes_review_and_base() -> Result<()> {
    let template = "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{CODE_REVIEW_BODY}}\n{{COMPLETION_CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let rendered = render_worker_phase3_prompt(
        template,
        "BASE\n\n## PROJECT TYPE: CODE\n\nBase Guidance\n",
        "REVIEW\n## PROJECT TYPE: CODE\n\nExtra\n\n## NEXT",
        "RQ-0001",
        "CHECKLIST",
        3,
        true,
        &config,
    )?;
    assert!(rendered.contains("PHASE=3"));
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("REVIEW"));
    assert!(rendered.contains("## NEXT"));
    assert_eq!(rendered.matches("## PROJECT TYPE: CODE").count(), 1);
    assert!(rendered.contains("CHECKLIST"));
    assert!(rendered.contains("BASE"));
    assert!(rendered.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!rendered.contains("{{"));
    Ok(())
}

#[test]
fn render_worker_single_phase_prompt_requires_task_id() -> Result<()> {
    let template = "{{TASK_ID}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let result =
        render_worker_single_phase_prompt(template, "BASE", "CHECKLIST", "", false, &config);
    assert!(result.is_err());
    Ok(())
}

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
    std::env::remove_var(var_name);
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
    std::env::set_var(var_name, "actual_value");
    let result = expand_variables(&template, &config)?;
    std::env::remove_var(var_name);
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
fn expand_variables_expands_config_ci_gate_command() -> Result<()> {
    let template = "CI: {{config.agent.ci_gate_command}}";
    let mut config = default_config();
    config.agent.ci_gate_command = Some("make ci".to_string());
    let result = expand_variables(template, &config)?;
    assert!(result.contains("CI: make ci"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_ci_gate_enabled() -> Result<()> {
    let template = "Enabled: {{config.agent.ci_gate_enabled}}";
    let mut config = default_config();
    config.agent.ci_gate_enabled = Some(false);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Enabled: false"));
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
fn render_code_review_prompt_replaces_placeholders() -> Result<()> {
    let template = "ID={{TASK_ID}}\n";
    let config = default_config();
    let rendered = render_code_review_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PROJECT TYPE: CODE"));
    Ok(())
}

#[test]
fn render_code_review_prompt_allows_placeholder_like_text() -> Result<()> {
    let template = "ID={{TASK_ID}}\nSome text with {{TASK_ID}} in it\n";
    let config = default_config();
    let rendered = render_code_review_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("Some text with RQ-0001 in it"));
    Ok(())
}

#[test]
fn render_code_review_prompt_fails_missing_task_id() -> Result<()> {
    let template = "{{TASK_ID}}\n";
    let config = default_config();
    let result = render_code_review_prompt(template, "", ProjectType::Code, &config);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("task id"));
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
