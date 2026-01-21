//! Prompt template loading, rendering, and validation utilities.

use crate::contracts::{Config, ProjectType};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

const WORKER_PROMPT_REL_PATH: &str = ".ralph/prompts/worker.md";
const TASK_BUILDER_PROMPT_REL_PATH: &str = ".ralph/prompts/task_builder.md";
const SCAN_PROMPT_REL_PATH: &str = ".ralph/prompts/scan.md";
const COMPLETION_CHECKLIST_REL_PATH: &str = ".ralph/prompts/completion_checklist.md";
const CODE_REVIEW_PROMPT_REL_PATH: &str = ".ralph/prompts/code_review.md";
const PHASE2_HANDOFF_CHECKLIST_REL_PATH: &str = ".ralph/prompts/phase2_handoff_checklist.md";

const DEFAULT_WORKER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/worker.md"
));
const DEFAULT_TASK_BUILDER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/task_builder.md"
));
const DEFAULT_SCAN_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/scan.md"
));
const DEFAULT_COMPLETION_CHECKLIST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/completion_checklist.md"
));
const DEFAULT_CODE_REVIEW_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/code_review.md"
));
const DEFAULT_PHASE2_HANDOFF_CHECKLIST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/phase2_handoff_checklist.md"
));

/// Instructions for tooling requirements when RepoPrompt is required.
pub const REPOPROMPT_REQUIRED_INSTRUCTION: &str = r#"
## TOOLING REQUIREMENT: RepoPrompt
You are running in a RepoPrompt-enabled environment. You MUST use the available RepoPrompt tools (`read_file`, `search_file_content`, `run_shell_command`, etc.) to explore the codebase. Do not rely on internal knowledge or assumptions. Verify everything.
"#;

pub const REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION: &str = r#"
## PLANNING REQUIREMENT: Use context_builder and passthrough the plan verbatim
To generate the plan, you MUST use the `context_builder` tool.
1. Provide an extensively detailed `instructions` argument to `context_builder` that describes the task.
2. MANDATORY: set `response_type` to "plan". The context_builder MUST be executed with a plan requested as the response type.
3. VERBATIM OUTPUT: Once `context_builder` returns, you MUST output its plan EXACTLY AS-IS, with zero edits, summarization, or reformatting.
4. WRAP EXACTLY: Output MUST be wrapped in:
<<RALPH_PLAN_BEGIN>>
<verbatim plan from context_builder>

---

Proceed with the implementation of the plan above.
"#;

pub fn wrap_with_repoprompt_requirement(prompt: &str, required: bool) -> String {
    if !required {
        return prompt.to_string();
    }
    format!(
        "{}\n\n{}\n\n{}",
        REPOPROMPT_REQUIRED_INSTRUCTION.trim(),
        REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION.trim(),
        prompt
    )
}

/// Expand environment variables and config values in a template string.
///
/// Syntax:
/// - `${VAR}` - expand environment variable (error if missing)
/// - `${VAR:-default}` - expand environment variable with default value
/// - `{{config.section.key}}` - expand config value (supports nested paths)
/// - `$${VAR}` or `\${VAR}` - escaped, outputs literal `${VAR}`
///
/// The function processes escapes first, then env vars, then config values.
/// This order ensures that escaped sequences are preserved throughout.
pub fn expand_variables(template: &str, config: &Config) -> Result<String> {
    let mut result = template.to_string();

    // First pass: handle escaping
    // $${VAR} -> ${VAR}, \${VAR} -> ${VAR}
    let escape_regex = Regex::new(r"\$\$\{|\\\$\{").unwrap();
    result = escape_regex.replace_all(&result, "${").to_string();

    // Second pass: expand environment variables
    // ${VAR} or ${VAR:-default}
    let env_regex = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(:-([^}]*))?\}").unwrap();
    result = env_regex
        .replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default = caps.get(3).map(|m| m.as_str());
            match env::var(var_name) {
                Ok(value) => value,
                Err(_) => match default {
                    Some(d) => d.to_string(),
                    None => {
                        log::warn!(
                            "Environment variable '${}' not found in prompt template. Use ${{{var_name}:-default}} for a default value.",
                            var_name
                        );
                        format!("${{{var_name}}}")
                    }
                },
            }
        })
        .to_string();

    // Third pass: expand config values
    // {{config.section.key}} - only if it starts with "config."
    // We need to skip non-config placeholders like {{USER_REQUEST}}
    let config_regex = Regex::new(r"\{\{config\.([^}]+)\}\}").unwrap();
    result = config_regex
        .replace_all(&result, |caps: &regex::Captures| {
            let path = &caps[1];
            match get_config_value(config, path) {
                Ok(value) => value,
                Err(e) => {
                    log::warn!(
                        "Failed to expand config value 'config.{}' in prompt template: {}. Using literal placeholder.",
                        path, e
                    );
                    format!("{{{{config.{}}}}}", path)
                }
            }
        })
        .to_string();

    Ok(result)
}

/// Get a config value by dot-separated path (e.g., "agent.runner", "queue.id_prefix").
/// Returns a string representation of the value.
fn get_config_value(config: &Config, path: &str) -> Result<String> {
    let parts: Vec<&str> = path.split('.').collect();
    match parts.as_slice() {
        ["agent", "runner"] => config
            .agent
            .runner
            .map(|r| format!("{:?}", r))
            .or_else(|| config.agent.runner.map(|r| format!("{:?}", r)))
            .ok_or_else(|| anyhow::anyhow!("agent.runner not set")),
        ["agent", "model"] => config
            .agent
            .model
            .as_ref()
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow::anyhow!("agent.model not set")),
        ["agent", "reasoning_effort"] => config
            .agent
            .reasoning_effort
            .map(|e| format!("{:?}", e))
            .ok_or_else(|| anyhow::anyhow!("agent.reasoning_effort not set")),
        ["agent", "claude_permission_mode"] => config
            .agent
            .claude_permission_mode
            .map(|m| format!("{:?}", m))
            .ok_or_else(|| anyhow::anyhow!("agent.claude_permission_mode not set")),
        ["queue", "id_prefix"] => config
            .queue
            .id_prefix
            .clone()
            .ok_or_else(|| anyhow::anyhow!("queue.id_prefix not set")),
        ["queue", "id_width"] => config
            .queue
            .id_width
            .map(|w| w.to_string())
            .ok_or_else(|| anyhow::anyhow!("queue.id_width not set")),
        ["project_type"] => config
            .project_type
            .map(|p| format!("{:?}", p))
            .ok_or_else(|| anyhow::anyhow!("project_type not set")),
        ["version"] => Ok(config.version.to_string()),
        _ => bail!("unknown config path: '{}'", path),
    }
}

pub fn prompts_reference_readme(repo_root: &Path) -> Result<bool> {
    let worker = load_worker_prompt(repo_root)?;
    let task_builder = load_task_builder_prompt(repo_root)?;
    let scan = load_scan_prompt(repo_root)?;
    let completion_checklist = load_completion_checklist(repo_root)?;
    let code_review = load_code_review_prompt(repo_root)?;
    let phase2_handoff = load_phase2_handoff_checklist(repo_root)?;

    Ok(worker.contains(".ralph/README.md")
        || task_builder.contains(".ralph/README.md")
        || scan.contains(".ralph/README.md")
        || completion_checklist.contains(".ralph/README.md")
        || code_review.contains(".ralph/README.md")
        || phase2_handoff.contains(".ralph/README.md"))
}

pub fn load_worker_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        WORKER_PROMPT_REL_PATH,
        DEFAULT_WORKER_PROMPT,
        "worker",
    )
}

/// Load the completion checklist template (overridable).
pub fn load_completion_checklist(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        COMPLETION_CHECKLIST_REL_PATH,
        DEFAULT_COMPLETION_CHECKLIST,
        "completion checklist",
    )
}

pub fn load_code_review_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        CODE_REVIEW_PROMPT_REL_PATH,
        DEFAULT_CODE_REVIEW_PROMPT,
        "code review",
    )
}

pub fn load_phase2_handoff_checklist(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        PHASE2_HANDOFF_CHECKLIST_REL_PATH,
        DEFAULT_PHASE2_HANDOFF_CHECKLIST,
        "phase2 handoff checklist",
    )
}

fn project_type_guidance(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Code => {
            r#"
## PROJECT TYPE: CODE

This is a code repository. Prioritize:
- Implementation correctness and type safety
- Test coverage and regression prevention
- Performance and resource efficiency
- Clean, maintainable code structure
"#
        }
        ProjectType::Docs => {
            r#"
## PROJECT TYPE: DOCS

This is a documentation repository. Prioritize:
- Clear, accurate information
- Consistent formatting and structure
- Accessibility and readability
- Examples and practical guidance
"#
        }
    }
}

pub fn render_worker_prompt(
    template: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    // Expand environment variables and config values first
    let expanded = expand_variables(template, config)?;
    let guidance = project_type_guidance(project_type);
    let rendered = if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    };
    let rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    ensure_no_unresolved_placeholders(&rendered, "worker")?;
    Ok(rendered)
}

/// Render the completion checklist after expanding variables.
pub fn render_completion_checklist(template: &str, config: &Config) -> Result<String> {
    let expanded = expand_variables(template, config)?;
    ensure_no_unresolved_placeholders(&expanded, "completion checklist")?;
    Ok(expanded)
}

pub fn render_phase2_handoff_checklist(template: &str, config: &Config) -> Result<String> {
    let expanded = expand_variables(template, config)?;
    ensure_no_unresolved_placeholders(&expanded, "phase2 handoff checklist")?;
    Ok(expanded)
}

pub fn render_code_review_prompt(
    template: &str,
    task_id: &str,
    git_status: &str,
    git_diff: &str,
    git_diff_staged: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    if !template.contains("{{TASK_ID}}") {
        bail!("Template error: code review prompt template is missing the required '{{TASK_ID}}' placeholder.");
    }
    if !template.contains("{{GIT_STATUS}}") {
        bail!("Template error: code review prompt template is missing the required '{{GIT_STATUS}}' placeholder.");
    }
    if !template.contains("{{GIT_DIFF}}") {
        bail!("Template error: code review prompt template is missing the required '{{GIT_DIFF}}' placeholder.");
    }
    if !template.contains("{{GIT_DIFF_STAGED}}") {
        bail!("Template error: code review prompt template is missing the required '{{GIT_DIFF_STAGED}}' placeholder.");
    }

    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: code review prompt requires a non-empty task id.");
    }

    let expanded = expand_variables(template, config)?;
    let guidance = project_type_guidance(project_type);
    let mut rendered = if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    };

    rendered = rendered.replace("{{TASK_ID}}", id);
    rendered = rendered.replace("{{GIT_STATUS}}", git_status);
    rendered = rendered.replace("{{GIT_DIFF}}", git_diff);
    rendered = rendered.replace("{{GIT_DIFF_STAGED}}", git_diff_staged);

    ensure_no_unresolved_placeholders(&rendered, "code review")?;
    Ok(rendered)
}

pub fn load_task_builder_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        TASK_BUILDER_PROMPT_REL_PATH,
        DEFAULT_TASK_BUILDER_PROMPT,
        "task builder",
    )
}

pub fn render_task_builder_prompt(
    template: &str,
    user_request: &str,
    hint_tags: &str,
    hint_scope: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    if !template.contains("{{USER_REQUEST}}") {
        bail!("Template error: task builder prompt template is missing the required '{{USER_REQUEST}}' placeholder. Ensure the template in .ralph/prompts/task_builder.md includes this placeholder.");
    }
    if !template.contains("{{HINT_TAGS}}") {
        bail!("Template error: task builder prompt template is missing the required '{{HINT_TAGS}}' placeholder. Ensure the template includes this placeholder.");
    }
    if !template.contains("{{HINT_SCOPE}}") {
        bail!("Template error: task builder prompt template is missing the required '{{HINT_SCOPE}}' placeholder. Ensure the template includes this placeholder.");
    }

    let request = user_request.trim();
    if request.is_empty() {
        bail!("Missing request: user request must be non-empty. Provide a descriptive request for the task builder.");
    }

    // Expand environment variables and config values first
    let expanded = expand_variables(template, config)?;
    let guidance = project_type_guidance(project_type);
    let mut rendered = if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    };
    rendered = rendered.replace("{{USER_REQUEST}}", request);
    rendered = rendered.replace("{{HINT_TAGS}}", hint_tags.trim());
    rendered = rendered.replace("{{HINT_SCOPE}}", hint_scope.trim());
    rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    ensure_no_unresolved_placeholders(&rendered, "task builder")?;
    Ok(rendered)
}

pub fn load_scan_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(repo_root, SCAN_PROMPT_REL_PATH, DEFAULT_SCAN_PROMPT, "scan")
}

pub fn render_scan_prompt(
    template: &str,
    user_focus: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    if !template.contains("{{USER_FOCUS}}") {
        bail!("Template error: scan prompt template is missing the required '{{USER_FOCUS}}' placeholder. Ensure the template in .ralph/prompts/scan.md includes this placeholder.");
    }
    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };

    // Expand environment variables and config values first
    let expanded = expand_variables(template, config)?;
    let guidance = project_type_guidance(project_type);
    let rendered = if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    };
    let rendered = rendered.replace("{{USER_FOCUS}}", focus);
    ensure_no_unresolved_placeholders(&rendered, "scan")?;
    Ok(rendered)
}

fn unresolved_placeholders(rendered: &str) -> Vec<String> {
    let mut placeholders = Vec::new();
    let bytes = rendered.as_bytes();
    let mut i = 0;

    while i < bytes.len().saturating_sub(3) {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = bytes[i..].iter().position(|&b| b == b'}') {
                let end_idx = i + end;
                if end_idx < bytes.len().saturating_sub(1) && bytes[end_idx + 1] == b'}' {
                    let placeholder = &rendered[i..end_idx + 2];
                    let trimmed = placeholder.trim_matches(|c| c == '{' || c == '}');
                    if !trimmed.is_empty() {
                        placeholders.push(trimmed.to_uppercase());
                    }
                    i = end_idx + 2;
                    continue;
                }
            }
        }
        i += 1;
    }

    let mut unique: Vec<String> = placeholders
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    unique.sort();
    unique
}

fn ensure_no_unresolved_placeholders(rendered: &str, label: &str) -> Result<()> {
    let placeholders = unresolved_placeholders(rendered);
    if !placeholders.is_empty() {
        bail!(
            "Prompt validation failed for {}: unresolved placeholders remain after rendering: {}. Review the {} prompt template and ensure all placeholders are either removed or correctly formatted.",
            label,
            placeholders.join(", "),
            label
        );
    }
    Ok(())
}

fn load_prompt_with_fallback(
    repo_root: &Path,
    rel_path: &str,
    embedded_default: &'static str,
    label: &str,
) -> Result<String> {
    let path = repo_root.join(rel_path);
    match fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(embedded_default.to_string()),
        Err(err) => Err(err).with_context(|| format!("read {label} prompt {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Config;
    use std::fs;
    use tempfile::TempDir;

    fn default_config() -> Config {
        Config::default()
    }

    #[test]
    fn render_worker_prompt_replaces_interactive_instructions() -> Result<()> {
        let template = "Hello\n{{INTERACTIVE_INSTRUCTIONS}}\n";
        let config = default_config();
        let rendered = render_worker_prompt(template, ProjectType::Code, &config)?;
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
        // Use unique variable name to avoid test interference
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
        // Use unique variable name to avoid test interference
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
        // When env var is missing and no default, it leaves the literal
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
        let template = "ID={{TASK_ID}}\n{{GIT_STATUS}}\n{{GIT_DIFF}}\n{{GIT_DIFF_STAGED}}\n";
        let config = default_config();
        let rendered = render_code_review_prompt(
            template,
            "RQ-0001",
            "STATUS",
            "DIFF",
            "STAGED",
            ProjectType::Code,
            &config,
        )?;
        assert!(rendered.contains("ID=RQ-0001"));
        assert!(rendered.contains("STATUS"));
        assert!(rendered.contains("DIFF"));
        assert!(rendered.contains("STAGED"));
        Ok(())
    }

    #[test]
    fn render_code_review_prompt_fails_missing_task_id() -> Result<()> {
        let template = "{{TASK_ID}}\n{{GIT_STATUS}}\n{{GIT_DIFF}}\n{{GIT_DIFF_STAGED}}\n";
        let config = default_config();
        let result = render_code_review_prompt(
            template,
            "", // Empty task ID
            "STATUS",
            "DIFF",
            "STAGED",
            ProjectType::Code,
            &config,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("task id"));
        Ok(())
    }

    #[test]
    fn expand_variables_invalid_config_path_left_literal() -> Result<()> {
        let template = "Value: {{config.invalid.path}}";
        let config = default_config();
        let result = expand_variables(template, &config)?;
        // Invalid config paths are left as-is
        assert!(result.contains("{{config.invalid.path}}"));
        Ok(())
    }
}
