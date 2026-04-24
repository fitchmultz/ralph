//! Runner binary detection utilities.
//!
//! Purpose:
//! - Runner binary detection utilities.
//!
//! Responsibilities:
//! - Check if runner binaries are installed and accessible.
//! - Extract version strings from runner binaries.
//!
//! Not handled here:
//! - Capability data (see capabilities.rs).
//! - CLI output formatting.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Context;

use crate::runutil::{ManagedCommand, TimeoutClass, execute_managed_command};
use std::process::Command;

/// Result of checking a runner binary.
#[derive(Debug, Clone)]
pub struct BinaryStatus {
    /// Whether the binary was found and executable.
    pub installed: bool,
    /// Version string if available.
    pub version: Option<String>,
    /// Error message if check failed.
    pub error: Option<String>,
}

/// Check if a runner binary is installed by trying common version/help flags.
///
/// Tries the following in order: --version, -V, --help, help
pub fn check_runner_binary(bin: &str) -> BinaryStatus {
    let fallbacks: &[&[&str]] = &[&["--version"], &["-V"], &["--help"], &["help"]];

    for args in fallbacks {
        match try_command(bin, args) {
            Ok(output) => {
                // Try to extract version from output
                let version = extract_version(&output);
                return BinaryStatus {
                    installed: true,
                    version,
                    error: None,
                };
            }
            Err(_) => continue,
        }
    }

    BinaryStatus {
        installed: false,
        version: None,
        error: Some(format!("binary '{}' not found or not executable", bin)),
    }
}

fn try_command(bin: &str, args: &[&str]) -> anyhow::Result<String> {
    let mut command = Command::new(bin);
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = execute_managed_command(ManagedCommand::new(
        command,
        format!("runner detection: {} {}", bin, args.join(" ")),
        TimeoutClass::Probe,
    ))
    .map(|output| output.into_output())
    .with_context(|| format!("failed to execute runner binary '{}'", bin))?;

    if output.status.success() {
        // Combine stdout and stderr for version parsing
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(format!("{}{}", stdout, stderr))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let cmd_display = format!("{} {}", bin, args.join(" "));
        anyhow::bail!(
            "runner binary check failed\n  command: {}\n  exit code: {}\n  stderr: {}",
            cmd_display.trim(),
            output.status,
            stderr.trim()
        )
    }
}

/// Extract version string from command output using common patterns.
fn extract_version(output: &str) -> Option<String> {
    // Look for common version patterns like "version 1.2.3" or "v1.2.3"
    for line in output.lines().take(5) {
        let lower = line.to_lowercase();
        if lower.contains("version") || lower.starts_with('v') {
            // Try to extract semver-like pattern
            if let Some(ver) = extract_semver(line) {
                return Some(ver);
            }
        }
    }
    // Fallback: return first non-empty line (often contains version)
    output.lines().next().map(|s| s.trim().to_string())
}

fn extract_semver(s: &str) -> Option<String> {
    // Simple heuristic: find digits and dots pattern
    let chars: Vec<char> = s.chars().collect();
    let mut start = None;
    let mut end = None;

    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_digit() && start.is_none() {
            start = Some(i);
        }
        if let Some(s) = start
            && !c.is_ascii_digit()
            && c != '.'
            && c != '-'
            && end.is_none()
            && i > s + 1
        {
            end = Some(i);
        }
    }

    match (start, end) {
        (Some(s), Some(e)) => Some(chars[s..e].iter().collect()),
        // Handle version at end of string (no terminator found)
        (Some(s), None) => Some(chars[s..].iter().collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_detection_handles_missing_binary() {
        let status = check_runner_binary("nonexistent_binary_12345");
        assert!(!status.installed);
        assert!(status.error.is_some());
    }

    #[test]
    fn extract_version_finds_semver() {
        let output = "codex version 1.2.3\nSome other info";
        let version = extract_version(output);
        // The function returns the first line containing "version" or starting with "v"
        assert!(version.as_ref().unwrap().contains("1.2.3"));
    }

    #[test]
    fn extract_version_handles_v_prefix() {
        let output = "v2.0.0-beta\nMore info";
        let version = extract_version(output);
        // The function returns the first line starting with "v" or containing "version"
        assert!(version.as_ref().unwrap().contains("2.0.0"));
    }

    #[test]
    fn extract_semver_handles_version_at_end() {
        // Version at end of string without terminator (bug fix verification)
        let result = extract_semver("version 1.2.3");
        assert_eq!(result, Some("1.2.3".to_string()));
    }

    #[test]
    fn extract_semver_handles_standalone_version() {
        // Just a version number with no other text (bug fix verification)
        let result = extract_semver("1.2.3");
        assert_eq!(result, Some("1.2.3".to_string()));
    }
}
