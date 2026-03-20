//! Shared prompt utilities (template expansion, validation, and loading).
//!
//! Responsibilities: expand environment/config variables, validate placeholders, and load prompt
//! overrides with fallbacks.
//! Not handled: prompt-specific placeholder replacement rules or registry metadata.
//! Invariants/assumptions: templates are UTF-8 strings using `{{...}}` placeholders and required
//! placeholders include braces (e.g., `{{TASK_ID}}`).

use crate::constants::buffers::MAX_INSTRUCTION_BYTES;
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
You are running in a RepoPrompt-enabled environment. Prefer RepoPrompt tools when they are available in this harness.

When the RepoPrompt MCP server is connected, these tool groups are typically available:
- Targeting: `list_windows` + `select_window` (or pass `_windowID`), `manage_workspaces`, `list_tabs` / `select_tab` (or pass `_tabID`)
- Discovery/context: `manage_selection`, `get_file_tree`, `file_search`, `read_file`, `get_code_structure`, `workspace_context`, `prompt`
- Edits: `apply_edits`, `file_actions`
- Read-only git: `git` (`status`, `diff`, `log`, `show`, `blame`)
- Planning/review: `context_builder`, `list_models`, `chat_send`, `chats`

If RepoPrompt is unavailable or incomplete, use the best available repository tools in the current harness.

## CLI FALLBACK (WHEN MCP TOOLS ARE UNAVAILABLE)
If RepoPrompt MCP tools are unavailable, prefer the RepoPrompt CLI when it exists:
- Start with `rp-cli --help`; optionally use `rp -h` if the wrapper is installed.
- `rp-cli` commonly uses `-e` to execute an expression (example: `rp-cli -e 'tree'`).
- `rp` is a convenience wrapper with simpler syntax (example: `rp 'tree'`).
- If `rp` is missing, fall back to `rp-cli`.
"#;

pub(crate) const REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION: &str = r#"
## REPOPROMPT PLANNING FLOW
When `context_builder` is available, use it as the standard planning path. If it is unavailable, use the best available repo-inspection tools and still produce the required plan artifact.
1. PREFERRED: do a quick repo reality check before planning:
   - validate key assumptions from the task
   - identify relevant files
   - note capability gaps or parity needs across user-facing entrypoints
2. PREFERRED: provide detailed `instructions` to `context_builder`, including the current task and generous file-selection guidance.
3. Standard approach: set `response_type` to "plan" so the result is shaped for planning.
4. RepoPrompt can draft the plan, but you own the final artifact. Correct it if repo reality disagrees.
5. If follow-up is needed, append missing files to the existing selection and continue in the same chat context.
6. PREFERRED: favor parity across CLI/API/UI/scripts when multiple entrypoints exist.
7. REQUIRED: write the final plan to the plan cache path provided in the prompt. Phase 2 reads that file.

## OUTPUT
- REQUIRED: write the plan to the plan cache path provided in the prompt.
- PREFERRED: reply with a brief confirmation instead of repeating the full plan text.
- REQUIRED: do not start implementation in Phase 1.
"#;

pub(crate) fn wrap_with_repoprompt_requirement(prompt: &str, required: bool) -> String {
    if !required {
        return prompt.to_string();
    }

    format!("{}\n\n{}", REPOPROMPT_REQUIRED_INSTRUCTION.trim(), prompt)
}

pub(crate) fn wrap_with_instruction_files(
    repo_root: &Path,
    prompt: &str,
    config: &Config,
) -> Result<String> {
    let mut sources: Vec<(String, String)> = Vec::new();

    // Instruction files from configuration (user-specified, not auto-injected).
    if let Some(paths) = config.agent.instruction_files.as_ref() {
        for raw in paths {
            let resolved = resolve_instruction_path(repo_root, raw);
            let content = read_instruction_file(&resolved, MAX_INSTRUCTION_BYTES)
                .with_context(|| format!("read instruction file at {}", resolved.display()))?;
            sources.push((resolved.display().to_string(), content));
        }
    }

    if sources.is_empty() {
        return Ok(prompt.to_string());
    }

    let mut preamble = String::new();
    preamble.push_str(
        r#"## AGENTS / GLOBAL INSTRUCTIONS (AUTHORITATIVE)
The following instruction files are authoritative for this run. Follow them exactly.

"#,
    );

    for (idx, (label, content)) in sources.into_iter().enumerate() {
        if idx > 0 {
            preamble.push_str("\n---\n\n");
        }
        preamble.push_str(&format!("### Source: {label}\n\n"));
        preamble.push_str(content.trim());
        preamble.push('\n');
    }

    Ok(format!("{}\n\n---\n\n{}", preamble.trim(), prompt))
}

pub(crate) fn instruction_file_warnings(repo_root: &Path, config: &Config) -> Vec<String> {
    let mut warnings = Vec::new();

    // Only check configured instruction files (no auto-injection).
    if let Some(paths) = config.agent.instruction_files.as_ref() {
        for raw in paths {
            let resolved = resolve_instruction_path(repo_root, raw);
            if let Err(err) = read_instruction_file(&resolved, MAX_INSTRUCTION_BYTES) {
                warnings.push(format!(
                    "instruction_files entry '{}' (resolved: {}) is invalid: {}",
                    raw.display(),
                    resolved.display(),
                    err
                ));
            }
        }
    }

    warnings
}

/// Validates all instruction files in config and returns first error encountered.
/// Used for early config validation (fails fast) during config resolution.
pub(crate) fn validate_instruction_file_paths(repo_root: &Path, config: &Config) -> Result<()> {
    if let Some(paths) = config.agent.instruction_files.as_ref() {
        for raw in paths {
            let resolved = resolve_instruction_path(repo_root, raw);
            // read_instruction_file returns Err if file doesn't exist, isn't UTF-8, or is empty
            if let Err(err) = read_instruction_file(&resolved, MAX_INSTRUCTION_BYTES) {
                bail!(
                    "Invalid instruction_files entry '{}': {}. \
                     Ensure the file exists, is readable, and contains valid UTF-8 content.",
                    raw.display(),
                    err
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn resolve_instruction_path(repo_root: &Path, raw: &Path) -> std::path::PathBuf {
    let expanded = crate::fsutil::expand_tilde(raw);

    if expanded.is_absolute() {
        expanded
    } else {
        repo_root.join(expanded)
    }
}

pub(crate) fn read_instruction_file(path: &Path, max_bytes: usize) -> Result<String> {
    let data = fs::read(path).with_context(|| format!("read bytes from {}", path.display()))?;
    if data.len() > max_bytes {
        bail!(
            "instruction file {} is too large ({} bytes > {} bytes max)",
            path.display(),
            data.len(),
            max_bytes
        );
    }
    let text = String::from_utf8(data).map_err(|e| {
        anyhow::anyhow!(
            "instruction file {} is not valid UTF-8: {}",
            path.display(),
            e
        )
    })?;
    if text.trim().is_empty() {
        bail!("instruction file {} is empty", path.display());
    }
    Ok(text)
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

#[cfg(test)]
mod tests {
    use super::{instruction_file_warnings, resolve_instruction_path, wrap_with_instruction_files};
    use crate::contracts::Config;
    use serial_test::serial;
    use std::env;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Global lock for environment variable tests
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn wrap_with_instruction_files_is_noop_when_none_configured() {
        let dir = TempDir::new().expect("tempdir");
        // Even if AGENTS.md exists, it should NOT be injected without explicit configuration
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        let cfg = Config::default();
        let out = wrap_with_instruction_files(dir.path(), "hello", &cfg).expect("wrap");
        assert_eq!(out, "hello");
    }

    #[test]
    fn wrap_with_instruction_files_includes_agents_md_when_explicitly_configured() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        let mut cfg = Config::default();
        // Explicitly configure AGENTS.md for injection
        cfg.agent.instruction_files = Some(vec![Path::new("AGENTS.md").to_path_buf()]);

        let out = wrap_with_instruction_files(dir.path(), "hello", &cfg).expect("wrap");
        assert!(out.contains("AGENTS / GLOBAL INSTRUCTIONS"));
        assert!(out.contains("Repo instructions"));
        assert!(out.ends_with("\n\n---\n\nhello"));
    }

    #[test]
    fn wrap_with_instruction_files_does_not_include_repo_agents_md_when_not_configured() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        // Config with no instruction_files - AGENTS.md should NOT be auto-injected
        let cfg = Config::default();

        let out = wrap_with_instruction_files(dir.path(), "hello", &cfg).expect("wrap");
        // Should be exactly the original prompt with no preamble
        assert_eq!(out, "hello");
        assert!(!out.contains("AGENTS / GLOBAL INSTRUCTIONS"));
        assert!(!out.contains("Repo instructions"));
    }

    #[test]
    fn wrap_with_instruction_files_errors_on_missing_configured_file() {
        let dir = TempDir::new().expect("tempdir");
        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![Path::new("missing.md").to_path_buf()]);

        let err = wrap_with_instruction_files(dir.path(), "hello", &cfg).unwrap_err();
        assert!(err.to_string().contains("missing.md"));
    }

    #[test]
    fn instruction_file_warnings_reports_missing_configured_file() {
        let dir = TempDir::new().expect("tempdir");
        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![Path::new("missing.md").to_path_buf()]);

        let warnings = instruction_file_warnings(dir.path(), &cfg);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("instruction_files"));
        assert!(warnings[0].contains("missing.md"));
    }

    #[test]
    fn instruction_file_warnings_does_not_warn_about_unconfigured_repo_agents_md() {
        let dir = TempDir::new().expect("tempdir");
        // Create AGENTS.md but do NOT configure it
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        let cfg = Config::default();

        let warnings = instruction_file_warnings(dir.path(), &cfg);
        // Should have no warnings since AGENTS.md is not configured
        assert!(
            warnings.is_empty(),
            "Expected no warnings for unconfigured AGENTS.md"
        );
    }

    #[test]
    #[serial]
    fn resolve_instruction_path_expands_tilde_to_home() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let original_home = env::var("HOME").ok();

        unsafe { env::set_var("HOME", "/custom/home") };

        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("~/instructions.md"));
        assert_eq!(resolved, Path::new("/custom/home/instructions.md"));

        // Restore HOME
        match original_home {
            Some(v) => unsafe { env::set_var("HOME", v) },
            None => unsafe { env::remove_var("HOME") },
        }
    }

    #[test]
    #[serial]
    fn resolve_instruction_path_expands_tilde_alone_to_home() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let original_home = env::var("HOME").ok();

        unsafe { env::set_var("HOME", "/custom/home") };

        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("~"));
        assert_eq!(resolved, Path::new("/custom/home"));

        // Restore HOME
        match original_home {
            Some(v) => unsafe { env::set_var("HOME", v) },
            None => unsafe { env::remove_var("HOME") },
        }
    }

    #[test]
    #[serial]
    fn resolve_instruction_path_relative_when_home_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let original_home = env::var("HOME").ok();

        // Remove HOME - tilde should not expand
        unsafe { env::remove_var("HOME") };

        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("~/instructions.md"));
        // When HOME is unset, ~/instructions.md is treated as relative to repo_root
        assert_eq!(resolved, Path::new("/repo/root/~/instructions.md"));

        // Restore HOME
        if let Some(v) = original_home {
            unsafe { env::set_var("HOME", v) }
        }
    }

    #[test]
    fn resolve_instruction_path_absolute_unchanged() {
        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("/absolute/path/file.md"));
        assert_eq!(resolved, Path::new("/absolute/path/file.md"));
    }

    #[test]
    fn resolve_instruction_path_relative_unchanged() {
        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("relative/path/file.md"));
        assert_eq!(resolved, Path::new("/repo/root/relative/path/file.md"));
    }
}
