//! Purpose: Shared prompt utilities for template expansion, placeholder validation, and prompt loading.
//!
//! Responsibilities:
//! - Expand environment and config variables in templates.
//! - Validate required and unresolved placeholders.
//! - Load prompt defaults and apply project-type guidance.
//!
//! Scope:
//! - Template rendering helpers and prompt loading only.
//! - Does not handle instruction-file I/O or prompt registry metadata.
//!
//! Usage:
//! - Used by prompt renderers and config resolution.
//! - Imported through `crate::prompts_internal::util` for internal helper access.
//!
//! Invariants/Assumptions:
//! - Templates are UTF-8 strings using `{{...}}` placeholders.
//! - Required placeholders include braces (e.g., `{{TASK_ID}}`).

use crate::constants::queue::{DEFAULT_DONE_FILE, DEFAULT_QUEUE_FILE};
use crate::contracts::{Config, ProjectType};
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::LazyLock;

/// Regex for matching escaped variable sequences (`$${` or `\${`).
/// Used to replace escaped sequences with literal `${`.
static ESCAPE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\$\{|\\\$\{").unwrap());

/// Regex for matching environment variable references (`${VAR}` or `${VAR:-default}`).
/// Captures the variable name and optional default value.
static ENV_VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(:-([^}]*))?\}").unwrap());

/// Regex for matching config value references (`{{config.section.key}}`).
/// Captures the config path for lookup.
static CONFIG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{config\.([^}]+)\}\}").unwrap());

#[derive(Clone, Copy, Debug)]
pub(crate) struct RequiredPlaceholder {
    pub token: &'static str,
    pub error_message: &'static str,
}

/// Instructions for tooling requirements when RepoPrompt tooling reminders are enabled.
pub(crate) const REPOPROMPT_REQUIRED_INSTRUCTION: &str = r#"
## REPOPROMPT TOOLING (WHEN CONNECTED)
Prefer RepoPrompt tools when they are the best way to gather repo context, inspect structure, or apply focused edits. If RepoPrompt is unavailable or incomplete, use the best available tools in the current harness.

Useful RepoPrompt tool groups:
- Targeting: `list_windows` + `select_window` (or pass `_windowID`), `manage_workspaces`, `list_tabs` / `select_tab` (or pass `_tabID`)
- Discovery/context: `manage_selection`, `get_file_tree`, `file_search`, `read_file`, `get_code_structure`, `workspace_context`, `prompt`
- Edits: `apply_edits`, `file_actions`
- Read-only git: `git` (`status`, `diff`, `log`, `show`, `blame`)
- Planning/review: `context_builder`, `list_models`, `chat_send`, `chats`

Tool budget: start with the smallest context that can answer the task, then expand only when a required file, symbol, validation signal, or decision input is missing.

## CLI FALLBACK
If MCP tools are unavailable, prefer the RepoPrompt CLI when installed:
- Start with `rp-cli --help`; use `rp -h` only if the wrapper exists.
- `rp-cli` commonly supports `-e`, for example `rp-cli -e 'tree'`.
- `rp` is a convenience wrapper, for example `rp 'tree'`.
"#;

pub(crate) const REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION: &str = r#"
## REPOPROMPT PLANNING FLOW
When `context_builder` is available, use it to produce planning evidence. You still own the final plan artifact.

Outcome:
- validate task assumptions against repo reality
- identify relevant files and affected entrypoints
- capture parity needs across CLI/API/UI/scripts when applicable
- write a standalone plan to the provided plan cache path

Suggested flow:
1. Do a quick repo reality check before asking for a plan.
2. Give `context_builder` the current task plus generous file-selection guidance.
3. Use `response_type: "plan"` when supported.
4. Correct the drafted plan if repo evidence disagrees.
5. If a required file is missing from context, append it and continue in the same chat.

Output:
- write the final plan to the plan cache path provided in the prompt
- reply with a brief confirmation instead of repeating the full plan unless required
- do not start implementation in Phase 1
"#;

pub(crate) fn wrap_with_repoprompt_requirement(prompt: &str, required: bool) -> String {
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
pub(crate) fn expand_variables(template: &str, config: &Config) -> Result<String> {
    let mut result = template.to_string();

    result = ESCAPE_REGEX.replace_all(&result, "${").to_string();

    result = ENV_VAR_REGEX
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

    result = CONFIG_REGEX
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
            .as_ref()
            .map(|r| format!("{:?}", r))
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
        ["agent", "ci_gate_enabled"] => Ok(config.agent.ci_gate_enabled().to_string()),
        ["agent", "ci_gate_display"] | ["agent", "ci_gate", "display"] => {
            Ok(config.agent.ci_gate_display_string())
        }
        ["agent", "ci_gate", "enabled"] => Ok(config.agent.ci_gate_enabled().to_string()),
        ["agent", "git_publish_mode"] => config
            .agent
            .effective_git_publish_mode()
            .map(|mode| mode.as_str().to_string())
            .ok_or_else(|| anyhow::anyhow!("agent.git_publish_mode not set")),
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
        ["queue", "file"] => Ok(config
            .queue
            .file
            .as_deref()
            .unwrap_or_else(|| Path::new(DEFAULT_QUEUE_FILE))
            .to_string_lossy()
            .to_string()),
        ["queue", "done_file"] => Ok(config
            .queue
            .done_file
            .as_deref()
            .unwrap_or_else(|| Path::new(DEFAULT_DONE_FILE))
            .to_string_lossy()
            .to_string()),
        ["project_type"] => config
            .project_type
            .map(|p| format!("{:?}", p))
            .ok_or_else(|| anyhow::anyhow!("project_type not set")),
        ["version"] => Ok(config.version.to_string()),
        _ => bail!("unknown config path: '{}'", path),
    }
}

pub(crate) fn unresolved_placeholders(rendered: &str) -> Vec<String> {
    let mut placeholders = Vec::new();
    let bytes = rendered.as_bytes();
    let mut i = 0;

    while i < bytes.len().saturating_sub(3) {
        if bytes[i] == b'{'
            && bytes[i + 1] == b'{'
            && let Some(end) = bytes[i..].iter().position(|&b| b == b'}')
        {
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
pub(crate) fn escape_placeholder_like_text(text: &str) -> String {
    if !text.contains("{{") && !text.contains("}}") {
        return text.to_string();
    }
    text.replace("{{", "{ {").replace("}}", "} }")
}

pub(crate) fn ensure_no_unresolved_placeholders(rendered: &str, label: &str) -> Result<()> {
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

pub(crate) fn load_prompt_with_fallback(
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

pub(crate) fn project_type_guidance(project_type: ProjectType) -> &'static str {
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

pub(crate) fn apply_project_type_guidance(expanded: &str, project_type: ProjectType) -> String {
    let guidance = project_type_guidance(project_type);
    if expanded.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        expanded.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", expanded, guidance)
    }
}

pub(crate) fn apply_project_type_guidance_if_needed(
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
