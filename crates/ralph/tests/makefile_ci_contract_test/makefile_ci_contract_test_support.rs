//! Shared helpers for Makefile CI contract tests.
//!
//! Responsibilities:
//! - Define the canonical CI and macOS gate step sequences.
//! - Parse Makefile target headers, recipes, and semantic CI expansions.
//! - Provide repo-root and Makefile loading helpers for contract modules.
//!
//! Not handled here:
//! - Individual contract assertions.
//! - Executing the full CI pipeline.
//!
//! Invariants/assumptions:
//! - The repo root is two directories above `CARGO_MANIFEST_DIR`.
//! - GNU Make must be available for clean-target smoke coverage.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub(super) const REQUIRED_CI_DOCS_STEPS: &[&str] = &["check-env-safety", "check-backup-artifacts"];

pub(super) const REQUIRED_CI_STEPS: &[&str] = &[
    "check-env-safety",
    "check-backup-artifacts",
    "deps",
    "format-check",
    "type-check",
    "lint",
    "test",
    "build",
    "generate",
    "install-verify",
];

pub(super) const REQUIRED_CI_FAST_STEPS: &[&str] = &[
    "check-env-safety",
    "check-backup-artifacts",
    "deps",
    "format-check",
    "type-check",
    "lint",
    "test",
];

pub(super) const REQUIRED_MACOS_TEST_CONTRACT_DEPS: &[&str] = &[
    "macos-test-settings-smoke",
    "macos-test-workspace-routing-contract",
];

pub(super) const REQUIRED_MACOS_CI_DEPS: &[&str] = &[
    "macos-preflight",
    "ci",
    "macos-build",
    "macos-test",
    "macos-test-contracts",
];

pub(super) fn required_ci_pipeline_text() -> String {
    REQUIRED_CI_STEPS.join(" → ")
}

pub(super) fn repo_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(PathBuf::from)
        .context("resolve repo root")
}

pub(super) fn read_repo_makefile() -> Result<String> {
    let repo_root = repo_root()?;
    std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")
}

pub(super) fn parse_target_declaration(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(".PHONY") {
        return None;
    }
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }

    let colon_index = trimmed.find(':')?;
    if trimmed.as_bytes().get(colon_index + 1) == Some(&b'=') {
        return None;
    }

    let target = trimmed[..colon_index].trim();
    let deps = trimmed[colon_index + 1..].trim();
    if target.is_empty() {
        return None;
    }

    Some((target, deps))
}

pub(super) fn parse_dependency_tokens(fragment: &str) -> Vec<String> {
    fragment
        .split('#')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|token| {
            let cleaned = token.trim_end_matches('\\').trim();
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned.to_string())
            }
        })
        .collect()
}

pub(super) fn collect_target_dependencies(
    lines: &[&str],
    header_index: usize,
    header_dependencies: &str,
) -> (Vec<String>, usize) {
    let mut dependencies = parse_dependency_tokens(header_dependencies);
    let mut index = header_index;

    while lines[index].trim_end().ends_with('\\') {
        index += 1;
        if index >= lines.len() {
            break;
        }
        dependencies.extend(parse_dependency_tokens(lines[index]));
    }

    (dependencies, index)
}

pub(super) fn parse_make_invocation_target(line: &str) -> Option<String> {
    let make_index = line.find("$(MAKE)")?;
    let invocation = &line[make_index + "$(MAKE)".len()..];

    invocation
        .split_whitespace()
        .find(|token| !token.starts_with('-') && !token.contains('='))
        .map(|token| token.trim_end_matches('\\').to_string())
}

pub(super) fn expand_ci_alias_steps(makefile: &str, steps: Vec<String>) -> Result<Vec<String>> {
    let mut expanded = Vec::new();

    for step in steps {
        if step == "ci-fast" {
            expanded.extend(
                extract_target_dependencies(makefile, "ci-fast")
                    .context("extract ci-fast deps for ci expansion")?,
            );
        } else {
            expanded.push(step);
        }
    }

    Ok(expanded)
}

pub(super) fn extract_make_ci_steps(makefile: &str) -> Result<Vec<String>> {
    let lines: Vec<&str> = makefile.lines().collect();
    let mut ci_header = None;

    for (index, line) in lines.iter().enumerate() {
        if let Some((target, deps)) = parse_target_declaration(line)
            && target == "ci"
        {
            ci_header = Some((index, deps));
            break;
        }
    }

    let (header_index, header_dependencies) = ci_header.context("failed to find `ci` target")?;
    let (mut steps, mut current_index) =
        collect_target_dependencies(&lines, header_index, header_dependencies);

    current_index += 1;
    while current_index < lines.len() {
        let line = lines[current_index];

        if let Some((target, _)) = parse_target_declaration(line)
            && target != "ci"
        {
            break;
        }

        if let Some(target) = parse_make_invocation_target(line) {
            steps.push(target);
        }

        current_index += 1;
    }

    let expanded_steps = expand_ci_alias_steps(makefile, steps)?;
    anyhow::ensure!(
        !expanded_steps.is_empty(),
        "failed to extract any ci steps from Makefile"
    );
    Ok(expanded_steps)
}

pub(super) fn extract_target_block(makefile: &str, target: &str) -> Result<String> {
    let header = format!("{target}:");
    let mut block_lines = Vec::new();
    let mut in_target = false;

    for line in makefile.lines() {
        if !in_target {
            if line.starts_with(&header) {
                in_target = true;
                block_lines.push(line.to_string());
            }
            continue;
        }

        let is_top_level = !line.starts_with('\t') && !line.starts_with(' ');
        if is_top_level {
            let trimmed = line.trim();
            if let Some(colon_index) = trimmed.find(':') {
                let is_assignment = trimmed.as_bytes().get(colon_index + 1) == Some(&b'=');
                if !is_assignment && !trimmed.starts_with('#') && !trimmed.starts_with(".PHONY") {
                    break;
                }
            }
        }

        block_lines.push(line.to_string());
    }

    anyhow::ensure!(
        !block_lines.is_empty(),
        "failed to extract target block for `{target}`"
    );
    Ok(block_lines.join("\n"))
}

pub(super) fn extract_target_dependencies(makefile: &str, target: &str) -> Result<Vec<String>> {
    let lines: Vec<&str> = makefile.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if let Some((line_target, deps)) = parse_target_declaration(line)
            && line_target == target
        {
            let (dependencies, _) = collect_target_dependencies(&lines, index, deps);
            return Ok(dependencies);
        }
    }

    anyhow::bail!("failed to find `{target}` target in Makefile")
}

pub(super) fn resolve_make_command() -> Result<String> {
    fn is_gnu_make_at_least_4(cmd: &str) -> bool {
        let output = Command::new(cmd).arg("--version").output();
        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        if parts.next() != Some("GNU") || parts.next() != Some("Make") {
            return false;
        }
        let Some(version) = parts.next() else {
            return false;
        };
        let Some(major_str) = version.split('.').next() else {
            return false;
        };
        let Ok(major) = major_str.parse::<u32>() else {
            return false;
        };
        major >= 4
    }

    if is_gnu_make_at_least_4("make") {
        return Ok("make".to_string());
    }
    if is_gnu_make_at_least_4("gmake") {
        return Ok("gmake".to_string());
    }

    anyhow::bail!(
        "GNU Make >= 4 is required for this repo. Install a newer GNU Make (on macOS: `brew install make`) and ensure `make` or `gmake` resolves to it."
    )
}
