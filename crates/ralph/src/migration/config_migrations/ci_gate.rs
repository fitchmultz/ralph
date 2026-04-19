//! Purpose: Rewrite legacy CI gate config into the structured `agent.ci_gate` shape.
//!
//! Responsibilities:
//! - Detect and rewrite legacy `ci_gate_command` / `ci_gate_enabled` fields.
//! - Materialize the new argv-based `agent.ci_gate` payload.
//! - Persist migrated config files atomically.
//!
//! Scope:
//! - CI gate migration only; generic key rename/remove and legacy contract upgrade
//!   live in sibling modules.
//!
//! Usage:
//! - Used by `MigrationType::ConfigCiGateRewrite`.
//!
//! Invariants/Assumptions:
//! - Disabled legacy CI gate maps to `{ "enabled": false }`.
//! - Enabled legacy shell strings are migrated to argv-only execution via `shlex::split`
//!   only when the string has no shell control operators outside quotes (no lossy
//!   migration of `&&`, pipes, redirects, and so on).
//! - Missing/empty legacy commands default to `make ci`.

use anyhow::{Context, Result};
use serde_json::Value;
use std::{fs, path::Path};

use super::super::MigrationContext;

/// Rewrite legacy CI gate keys into structured `agent.ci_gate` config.
pub fn apply_ci_gate_rewrite(ctx: &MigrationContext) -> Result<()> {
    rewrite_ci_gate_in_file(&ctx.project_config_path)?;

    if let Some(global_path) = &ctx.global_config_path {
        rewrite_ci_gate_in_file(global_path)?;
    }

    Ok(())
}

pub(crate) fn rewrite_ci_gate_in_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;
    let mut value: Value = jsonc_parser::parse_to_serde_value::<Value>(&raw, &Default::default())?;

    let Some(root) = value.as_object_mut() else {
        return Ok(());
    };
    let Some(agent) = root.get_mut("agent").and_then(Value::as_object_mut) else {
        return Ok(());
    };

    let legacy_command = agent.remove("ci_gate_command");
    let legacy_enabled = agent.remove("ci_gate_enabled");
    if legacy_command.is_none() && legacy_enabled.is_none() {
        return Ok(());
    }

    let enabled = legacy_enabled
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let ci_gate = build_ci_gate_value(legacy_command.as_ref(), enabled)?;
    agent.insert("ci_gate".to_string(), ci_gate);

    let rendered = serde_json::to_string_pretty(&value).context("serialize migrated config")?;
    crate::fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write migrated config {}", path.display()))?;
    Ok(())
}

pub(crate) fn build_ci_gate_value(legacy_command: Option<&Value>, enabled: bool) -> Result<Value> {
    if !enabled {
        return Ok(serde_json::json!({ "enabled": false }));
    }

    let command = legacy_command
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("make ci");

    if let Some(reason) = legacy_ci_gate_command_rejects_lossless_migration(command) {
        anyhow::bail!(
            "cannot migrate legacy agent.ci_gate_command: {reason} \
             Replace it with a checked-in script (for example ./scripts/ci.sh) and set \
             agent.ci_gate.argv to an explicit argv array such as [\"./scripts/ci.sh\"]. \
             Original command: {command}"
        );
    }

    let argv = shlex::split(command).ok_or_else(|| {
        anyhow::anyhow!(
            "could not migrate legacy CI gate command to argv-only execution: {}",
            command
        )
    })?;
    if argv.is_empty() {
        return Ok(serde_json::json!({ "enabled": false }));
    }

    Ok(serde_json::json!({
        "enabled": true,
        "argv": argv,
    }))
}

/// Returns a short rejection reason when the legacy string cannot be migrated to argv
/// without a shell (compound commands, pipes, redirects, and so on).
fn legacy_ci_gate_command_rejects_lossless_migration(command: &str) -> Option<&'static str> {
    let bytes = command.as_bytes();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;

    while i < bytes.len() {
        let b = bytes[i];

        if in_single {
            if b == b'\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_double = false;
            }
            i += 1;
            continue;
        }

        match b {
            b'\'' => {
                in_single = true;
                i += 1;
            }
            b'"' => {
                in_double = true;
                i += 1;
            }
            b'&' => {
                if bytes.get(i..i + 2) == Some(b"&&") {
                    return Some("command contains `&&` outside quotes.");
                }
                return Some("command contains `&` outside quotes (background or shell syntax).");
            }
            b'|' => {
                if bytes.get(i..i + 2) == Some(b"||") {
                    return Some("command contains `||` outside quotes.");
                }
                return Some("command contains `|` (shell pipe) outside quotes.");
            }
            b';' => return Some("command contains `;` (shell command separator) outside quotes."),
            b'<' => return Some("command contains `<` (shell redirect) outside quotes."),
            b'>' => return Some("command contains `>` (shell redirect) outside quotes."),
            b'(' | b')' => {
                return Some("command contains `(` or `)` outside quotes (subshell/grouping).");
            }
            b'`' => {
                return Some("command contains backticks (command substitution) outside quotes.");
            }
            b'$' if bytes.get(i + 1).copied() == Some(b'(') => {
                return Some("command contains `$(` (command substitution) outside quotes.");
            }
            _ => i += 1,
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::build_ci_gate_value;
    use serde_json::json;

    #[test]
    fn build_ci_gate_migrates_simple_legacy_command() {
        let out = build_ci_gate_value(Some(&json!("make ci")), true).unwrap();
        assert_eq!(out["enabled"], true);
        assert_eq!(out["argv"], json!(["make", "ci"]));
    }

    #[test]
    fn build_ci_gate_disabled_ignores_command() {
        let out = build_ci_gate_value(Some(&json!("cargo test && cargo clippy")), false).unwrap();
        assert_eq!(out["enabled"], false);
        assert!(out.get("argv").is_none());
    }

    #[test]
    fn build_ci_gate_rejects_double_ampersand() {
        let err =
            build_ci_gate_value(Some(&json!("cargo test && cargo clippy")), true).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot migrate legacy agent.ci_gate_command"),
            "{err}"
        );
        assert!(err.to_string().contains("&&"), "{err}");
    }

    #[test]
    fn build_ci_gate_accepts_ampersand_inside_single_quotes() {
        let out = build_ci_gate_value(Some(&json!("echo 'a&&b'")), true).unwrap();
        assert_eq!(out["argv"], json!(["echo", "a&&b"]));
    }

    #[test]
    fn build_ci_gate_rejects_pipe() {
        let err = build_ci_gate_value(Some(&json!("cargo fmt | cargo clippy")), true).unwrap_err();
        assert!(err.to_string().contains("|"), "{err}");
    }
}
