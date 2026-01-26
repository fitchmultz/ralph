//! Shared prompt utilities (template expansion, validation, and loading).
//!
//! Responsibilities: expand environment/config variables, validate placeholders, and load prompt
//! overrides with fallbacks.
//! Not handled: prompt-specific placeholder replacement rules or registry metadata.
//! Invariants/assumptions: templates are UTF-8 strings using `{{...}}` placeholders and required
//! placeholders include braces (e.g., `{{TASK_ID}}`).

use crate::contracts::{Config, ProjectType};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RequiredPlaceholder {
    pub token: &'static str,
    pub error_message: &'static str,
}

/// Instructions for tooling requirements when RepoPrompt tooling reminders are enabled.
pub const REPOPROMPT_REQUIRED_INSTRUCTION: &str = r#"
## TOOLING REQUIREMENT: RepoPrompt
You are running in a RepoPrompt-enabled environment. You MUST use the available RepoPrompt tools (`list_windows`, `select_window`, `apply_edits`, `read_file`, `file_search`, etc.) to explore the codebase. Do not rely on internal knowledge or assumptions. Verify everything.
"#;

pub const REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION: &str = r#"
## PLANNING REQUIREMENT: Use context_builder and write the plan to the cache
To generate the plan, you MUST use the `context_builder` tool.
1. BEFORE invoking `context_builder`, do a quick repo reality check using the available tools:
   - Validate key assumptions in the task/evidence/plan/notes (e.g., referenced commands, entrypoints, files, features).
   - Identify the most relevant files and any capability gaps or parity needs across user-facing entrypoints (CLI/API/UI/scripts).
   - Feed these findings and file paths into the `context_builder` instructions along with the task context.
2. Provide an extensively detailed `instructions` argument to `context_builder` that describes the CURRENT TASK (use the task provided in the prompt; do not pick another task). Within the `instructions` string you pass to `context_builder`, include guidance directing it to be generous with file selection: instruct `context_builder` that if there is any doubt whether a file might be needed for planning, it should be included. Direct `context_builder` to utilize its token budget generously and always include files that could possibly be needed by the planning agent.
3. MANDATORY: set `response_type` to "plan". The `context_builder` MUST be executed with a plan requested as the response type.
4. RepoPrompt produces a plan, but you own its correctness. If the plan contradicts repo reality, you must correct it before writing the plan to the mandated file path. Do this by adjusting the plan based on your findings and/or asking follow-ups via the provided chat ID.
5. If you need a follow-up: first review the current Repo Prompt file selection, append (add) missing files do NOT replace selection), then ask the follow-up in the same chat context using the chat ID returned via the context_builder tool. Use the Repo Prompt manage selection add/append tool in your harness (for example, a `manage_selection` tool with `op=add`), not a tool that replaces the selection.
6. Parity rule: if the repo exposes multiple user-facing entrypoints (CLI/API/UI/scripts), prefer parity over downgrading requirements or docs. Adjust the plan to implement the missing capability rather than changing the plan to fit a gap. If only one entrypoint exists, ignore this rule.
7. FINAL PLAN WRITE: Write the final plan to the mandated file path using verbatim content from the RepoPrompt plan response, except for refinements you deem necessary and/or changes established via follow-up messages with the RepoPrompt chat planner.

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
        ["agent", "iterations"] => config
            .agent
            .iterations
            .map(|value| value.to_string())
            .ok_or_else(|| anyhow::anyhow!("agent.iterations not set")),
        ["agent", "followup_reasoning_effort"] => config
            .agent
            .followup_reasoning_effort
            .map(|e| format!("{:?}", e))
            .ok_or_else(|| anyhow::anyhow!("agent.followup_reasoning_effort not set")),
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
        ["agent", "git_commit_push_enabled"] => config
            .agent
            .git_commit_push_enabled
            .map(|v| v.to_string())
            .ok_or_else(|| anyhow::anyhow!("agent.git_commit_push_enabled not set")),
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

/// Escape placeholder-like tokens in user-provided prompt sections for validation.
///
/// This keeps literal `{{...}}` sequences in user text from triggering unresolved
/// placeholder errors while still allowing templates to enforce required tokens.
pub fn escape_placeholder_like_text(text: &str) -> String {
    if !text.contains("{{") && !text.contains("}}") {
        return text.to_string();
    }
    text.replace("{{", "{ {").replace("}}", "} }")
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

pub(crate) fn ensure_required_placeholders(
    template: &str,
    required: &[RequiredPlaceholder],
) -> Result<()> {
    for placeholder in required {
        if !template.contains(placeholder.token) {
            bail!(placeholder.error_message);
        }
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

pub fn apply_project_type_guidance(expanded: &str, project_type: ProjectType) -> String {
    let guidance = project_type_guidance(project_type);
    if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    }
}

pub fn apply_project_type_guidance_if_needed(
    expanded: &str,
    project_type: ProjectType,
    enabled: bool,
) -> String {
    if enabled {
        apply_project_type_guidance(expanded, project_type)
    } else {
        expanded.to_string()
    }
}
