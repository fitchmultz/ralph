//! Shared prompt utilities (template expansion, validation, and loading).
//!
//! This module centralizes common behavior used by multiple prompt categories.

use crate::contracts::{Config, ProjectType};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

/// Instructions for tooling requirements when RepoPrompt is required.
pub const REPOPROMPT_REQUIRED_INSTRUCTION: &str = r#"
## TOOLING REQUIREMENT: RepoPrompt
You are running in a RepoPrompt-enabled environment. You MUST use the available RepoPrompt tools (`read_file`, `search_file_content`, `run_shell_command`, etc.) to explore the codebase. Do not rely on internal knowledge or assumptions. Verify everything.
"#;

pub const REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION: &str = r#"
## PLANNING REQUIREMENT: Use context_builder and write the plan to the cache
To generate the plan, you MUST use the `context_builder` tool.
1. Provide an extensively detailed `instructions` argument to `context_builder` that describes the CURRENT TASK (use the task context provided in the prompt; do not pick another task).
2. MANDATORY: set `response_type` to "plan". The `context_builder` MUST be executed with a plan requested as the response type.
3. VERBATIM FILE WRITE: Once `context_builder` returns, you MUST write its plan content EXACTLY AS-IS (with zero edits, summarization, or reformatting) to the plan cache file specified in the prompt.

## OUTPUT FORMAT (MANDATORY)
- Do NOT print the plan in your reply.
- Write the plan verbatim to the plan cache file specified in the prompt.
- Use the available tooling to write the plan file directly.
- After writing the file, respond only with a short confirmation (no plan text).

Do NOT add any other text beyond the brief confirmation.
Do NOT start implementation in Phase 1.
"#;

pub fn wrap_with_repoprompt_requirement(prompt: &str, required: bool) -> String {
    if !required {
        return prompt.to_string();
    }

    format!("{}\n\n{}", REPOPROMPT_REQUIRED_INSTRUCTION.trim(), prompt)
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

    let escape_regex = Regex::new(r"\$\$\{|\\\$\{").unwrap();
    result = escape_regex.replace_all(&result, "${").to_string();

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
        ["agent", "ci_gate_command"] => config
            .agent
            .ci_gate_command
            .clone()
            .ok_or_else(|| anyhow::anyhow!("agent.ci_gate_command not set")),
        ["agent", "ci_gate_enabled"] => config
            .agent
            .ci_gate_enabled
            .map(|v| v.to_string())
            .ok_or_else(|| anyhow::anyhow!("agent.ci_gate_enabled not set")),
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

pub fn unresolved_placeholders(rendered: &str) -> Vec<String> {
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

pub fn ensure_no_unresolved_placeholders(rendered: &str, label: &str) -> Result<()> {
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

pub fn load_prompt_with_fallback(
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

pub fn project_type_guidance(project_type: ProjectType) -> &'static str {
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
