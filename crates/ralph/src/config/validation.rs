//! Configuration validation functions for Ralph.
//!
//! Responsibilities:
//! - Validate config values (version, paths, numeric ranges, runner binaries).
//! - Validate queue config overrides (prefix, width, file paths).
//! - Validate git branch names for parallel execution.
//! - Validate agent config patches (for profiles).
//!
//! Not handled here:
//! - Config file loading/parsing (see `super::layer`).
//! - Config resolution from multiple sources (see `super::resolution`).
//! - Profile application logic (see `super::resolution`).
//!
//! Invariants/assumptions:
//! - Validation errors are returned as `anyhow::Error` with descriptive messages.
//! - Queue validation uses shared error messages for consistency.

use crate::constants::runner::{MAX_PHASES, MIN_ITERATIONS, MIN_PARALLEL_WORKERS, MIN_PHASES};
use crate::contracts::{AgentConfig, Config, QueueAgingThresholds, QueueConfig};
use anyhow::{Result, bail};
use std::path::{Component, Path};

/// Helper to format the aging threshold ordering error message.
fn format_aging_threshold_error(
    warning: Option<u32>,
    stale: Option<u32>,
    rotten: Option<u32>,
) -> String {
    format!(
        "Invalid queue.aging_thresholds ordering: require warning_days < stale_days < rotten_days (got warning_days={}, stale_days={}, rotten_days={}). Update .ralph/config.json.",
        warning
            .map(|w| w.to_string())
            .unwrap_or_else(|| "unset".to_string()),
        stale
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unset".to_string()),
        rotten
            .map(|r| r.to_string())
            .unwrap_or_else(|| "unset".to_string()),
    )
}

// Canonical error messages for queue config validation (single source of truth)
pub(crate) const ERR_EMPTY_QUEUE_ID_PREFIX: &str = "Empty queue.id_prefix: prefix is required if specified. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix.";
pub(crate) const ERR_INVALID_QUEUE_ID_WIDTH: &str = "Invalid queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width.";
pub(crate) const ERR_EMPTY_QUEUE_FILE: &str = "Empty queue.file: path is required if specified. Specify a valid path (e.g., '.ralph/queue.json') in .ralph/config.json or via --queue-file.";
pub(crate) const ERR_EMPTY_QUEUE_DONE_FILE: &str = "Empty queue.done_file: path is required if specified. Specify a valid path (e.g., '.ralph/done.json') in .ralph/config.json or via --done-file.";

/// Validate queue.id_prefix override (if specified, must be non-empty after trim).
pub fn validate_queue_id_prefix_override(id_prefix: Option<&str>) -> Result<()> {
    if let Some(prefix) = id_prefix
        && prefix.trim().is_empty()
    {
        bail!(ERR_EMPTY_QUEUE_ID_PREFIX);
    }
    Ok(())
}

/// Validate queue.id_width override (if specified, must be greater than 0).
pub fn validate_queue_id_width_override(id_width: Option<u8>) -> Result<()> {
    if let Some(width) = id_width
        && width == 0
    {
        bail!(ERR_INVALID_QUEUE_ID_WIDTH);
    }
    Ok(())
}

/// Validate queue.file override (if specified, must be non-empty).
pub fn validate_queue_file_override(file: Option<&Path>) -> Result<()> {
    if let Some(path) = file
        && path.as_os_str().is_empty()
    {
        bail!(ERR_EMPTY_QUEUE_FILE);
    }
    Ok(())
}

/// Validate queue.done_file override (if specified, must be non-empty).
pub fn validate_queue_done_file_override(done_file: Option<&Path>) -> Result<()> {
    if let Some(path) = done_file
        && path.as_os_str().is_empty()
    {
        bail!(ERR_EMPTY_QUEUE_DONE_FILE);
    }
    Ok(())
}

/// Validate all queue config overrides in a single call.
pub fn validate_queue_overrides(queue: &QueueConfig) -> Result<()> {
    validate_queue_id_prefix_override(queue.id_prefix.as_deref())?;
    validate_queue_id_width_override(queue.id_width)?;
    validate_queue_file_override(queue.file.as_deref())?;
    validate_queue_done_file_override(queue.done_file.as_deref())?;
    Ok(())
}

/// Validate queue.aging_thresholds ordering (if specified).
///
/// When any thresholds are specified, validates that:
/// - warning_days < stale_days (when both are set)
/// - stale_days < rotten_days (when both are set)
/// - warning_days < rotten_days (when both are set, transitive check)
pub fn validate_queue_aging_thresholds(thresholds: &Option<QueueAgingThresholds>) -> Result<()> {
    let Some(t) = thresholds else {
        return Ok(());
    };

    let warning = t.warning_days;
    let stale = t.stale_days;
    let rotten = t.rotten_days;

    // Check ordering when pairs are specified
    if let (Some(w), Some(s)) = (warning, stale)
        && w >= s
    {
        bail!(format_aging_threshold_error(Some(w), Some(s), rotten));
    }

    if let (Some(s), Some(r)) = (stale, rotten)
        && s >= r
    {
        bail!(format_aging_threshold_error(warning, Some(s), Some(r)));
    }

    // Transitive check for warning < rotten (catches cases where middle value is unset)
    if let (Some(w), Some(r)) = (warning, rotten)
        && w >= r
    {
        bail!(format_aging_threshold_error(Some(w), stale, Some(r)));
    }

    Ok(())
}

/// Validate that all configured binary paths are non-empty strings.
///
/// Checks each binary path field in AgentConfig - if specified, it must be
/// non-empty after trimming whitespace.
///
/// # Arguments
/// * `agent` - The agent config to validate
/// * `label` - Context label for error messages (e.g., "agent", "profiles.dev")
///
/// # Binary fields validated
/// - codex_bin, opencode_bin, gemini_bin, claude_bin, cursor_bin, kimi_bin, pi_bin
pub fn validate_agent_binary_paths(agent: &AgentConfig, label: &str) -> Result<()> {
    macro_rules! check_bin {
        ($field:ident) => {
            if let Some(bin) = &agent.$field
                && bin.trim().is_empty()
            {
                bail!(
                    "Empty {label}.{}: binary path is required if specified.",
                    stringify!($field)
                );
            }
        };
    }

    check_bin!(codex_bin);
    check_bin!(opencode_bin);
    check_bin!(gemini_bin);
    check_bin!(claude_bin);
    check_bin!(cursor_bin);
    check_bin!(kimi_bin);
    check_bin!(pi_bin);

    Ok(())
}

/// Validate the full configuration.
pub fn validate_config(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        bail!(
            "Unsupported config version: {}. Ralph requires version 1. Update the 'version' field in your config file.",
            cfg.version
        );
    }

    // Validate queue overrides using shared validators (single source of truth)
    validate_queue_overrides(&cfg.queue)?;
    validate_queue_aging_thresholds(&cfg.queue.aging_thresholds)?;

    if let Some(phases) = cfg.agent.phases
        && !(MIN_PHASES..=MAX_PHASES).contains(&phases)
    {
        bail!(
            "Invalid agent.phases: {}. Supported values are {}, {}, or {}. Update .ralph/config.json or CLI flags.",
            phases,
            MIN_PHASES,
            MIN_PHASES + 1,
            MAX_PHASES
        );
    }

    if let Some(iterations) = cfg.agent.iterations
        && iterations < MIN_ITERATIONS
    {
        bail!(
            "Invalid agent.iterations: {}. Iterations must be at least {}. Update .ralph/config.json.",
            iterations,
            MIN_ITERATIONS
        );
    }

    if let Some(workers) = cfg.parallel.workers
        && workers < MIN_PARALLEL_WORKERS
    {
        bail!(
            "Invalid parallel.workers: {}. Parallel workers must be >= {}. Update .ralph/config.json or CLI flags.",
            workers,
            MIN_PARALLEL_WORKERS
        );
    }

    // Validate workspace_root does not contain '..' components for security/predictability
    if let Some(root) = &cfg.parallel.workspace_root {
        if root.as_os_str().is_empty() {
            bail!(
                "Empty parallel.workspace_root: path is required if specified. Set a valid path or remove the field."
            );
        }
        if root.components().any(|c| matches!(c, Component::ParentDir)) {
            bail!(
                "Invalid parallel.workspace_root: path must not contain '..' components (got {}). Use a normalized path.",
                root.display()
            );
        }
    }

    if let Some(timeout) = cfg.agent.session_timeout_hours
        && timeout == 0
    {
        bail!(
            "Invalid agent.session_timeout_hours: {}. Session timeout must be greater than 0. Update .ralph/config.json.",
            timeout
        );
    }

    // Validate all agent binary paths using shared helper
    validate_agent_binary_paths(&cfg.agent, "agent")?;

    let ci_gate_enabled = cfg.agent.ci_gate_enabled.unwrap_or(true);
    if ci_gate_enabled
        && let Some(command) = &cfg.agent.ci_gate_command
        && command.trim().is_empty()
    {
        bail!(
            "Empty agent.ci_gate_command: CI gate command must be non-empty when enabled. Set a command (e.g., 'make ci') or disable the gate with agent.ci_gate_enabled=false."
        );
    }

    // Validate profile agent configs
    if let Some(profiles) = cfg.profiles.as_ref() {
        for (name, patch) in profiles {
            validate_agent_patch(patch, &format!("profiles.{name}"))?;
        }
    }

    Ok(())
}

/// Validate an AgentConfig patch (used for base agent and profile agents).
pub fn validate_agent_patch(agent: &AgentConfig, label: &str) -> Result<()> {
    if let Some(phases) = agent.phases
        && !(MIN_PHASES..=MAX_PHASES).contains(&phases)
    {
        bail!(
            "Invalid {label}.phases: {phases}. Supported values are {MIN_PHASES}, {}, or {MAX_PHASES}.",
            MIN_PHASES + 1
        );
    }

    if let Some(iterations) = agent.iterations
        && iterations < MIN_ITERATIONS
    {
        bail!(
            "Invalid {label}.iterations: {iterations}. Iterations must be at least {MIN_ITERATIONS}."
        );
    }

    if let Some(timeout) = agent.session_timeout_hours
        && timeout == 0
    {
        bail!(
            "Invalid {label}.session_timeout_hours: {timeout}. Session timeout must be greater than 0."
        );
    }

    // Validate all agent binary paths using shared helper
    validate_agent_binary_paths(agent, label)?;

    Ok(())
}

/// Validate parallel branch prefix by checking if it forms a valid git branch name.
pub fn validate_parallel_branch_prefix(prefix: &str) -> Result<()> {
    use crate::constants::git::SAMPLE_TASK_ID;

    // Validate the *constructed* branch name (prefix + typical task id),
    // since prefixes like "ralph/" are intended and only become valid with a suffix.
    let sample_branch = format!("{}{}", prefix, SAMPLE_TASK_ID);

    if let Some(reason) = git_ref_invalid_reason(&sample_branch) {
        bail!(
            "Invalid parallel.branch_prefix: {prefix:?}. When combined with a task id it must form a valid git branch name (e.g., {sample_branch:?}). {reason}. Update .ralph/config.json."
        );
    }

    Ok(())
}

/// Check if a string is a valid git branch name.
/// Returns None if valid, or Some(reason) if invalid.
/// Based on git's check-ref-format rules:
/// - Cannot contain spaces, tabs, or control characters
/// - Cannot contain .. (dotdot)
/// - Cannot contain @{ (at brace)
/// - Cannot start with . or end with .lock
/// - Cannot contain /./ or // or end with /
/// - Cannot be @ or contain @{ (reflog syntax)
pub fn git_ref_invalid_reason(branch: &str) -> Option<String> {
    // Empty check
    if branch.is_empty() {
        return Some("branch name cannot be empty".to_string());
    }

    // Check for spaces and control characters
    if branch.chars().any(|c| c.is_ascii_control() || c == ' ') {
        return Some("branch name cannot contain spaces or control characters".to_string());
    }

    // Check for double dots
    if branch.contains("..") {
        return Some("branch name cannot contain '..'".to_string());
    }

    // Check for @{ (reflog syntax)
    if branch.contains("@{") {
        return Some("branch name cannot contain '@{{'".to_string());
    }

    // Check for invalid dot patterns
    if branch.starts_with('.') {
        return Some("branch name cannot start with '.'".to_string());
    }

    if branch.ends_with(".lock") {
        return Some("branch name cannot end with '.lock'".to_string());
    }

    // Check for invalid slash patterns
    if branch.contains("//") || branch.contains("/.") || branch.ends_with('/') {
        return Some("branch name contains invalid slash/dot pattern".to_string());
    }

    // Check for @ as entire name or component
    if branch == "@" || branch.starts_with("@/") || branch.contains("/@/") || branch.ends_with("/@")
    {
        return Some("branch name cannot be '@' or contain '@' as a path component".to_string());
    }

    // Check for tilde expansion issues (~ is special in git)
    if branch.contains('~') {
        return Some("branch name cannot contain '~'".to_string());
    }

    // Check for caret (revision suffix)
    if branch.contains('^') {
        return Some("branch name cannot contain '^'".to_string());
    }

    // Check for colon (used for object names)
    if branch.contains(':') {
        return Some("branch name cannot contain ':'".to_string());
    }

    // Check for backslash
    if branch.contains('\\') {
        return Some("branch name cannot contain '\\'".to_string());
    }

    None
}
