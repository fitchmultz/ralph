//! Integration tests for Makefile CI contract.
//!
//! Responsibilities:
//! - Verify the Makefile `ci` and `macos-ci` targets define the exact required
//!   dependency sequence (no missing, reordered, or duplicated steps).
//! - Ensure documentation (CONTRIBUTING.md, GEMINI.md) stays synchronized with
//!   the canonical CI pipeline definition.
//! - Validate clean target preserves user data while removing temp artifacts.
//!
//! Not handled here:
//! - Execution of CI targets (see Makefile and actual CI runs).
//! - Linting/formatting/type-checking correctness (see respective tooling).
//!
//! Invariants/assumptions:
//! - The `ci` target in the Makefile must exactly match `REQUIRED_CI_STEPS`.
//! - The `macos-ci` target must exactly match `REQUIRED_MACOS_CI_DEPS`.
//! - Docs parity is anchored to the canonical constant, not dynamically parsed
//!   Makefile output, to prevent lockstep drift.

use anyhow::{Context, Result};
use std::process::Command;
use tempfile::TempDir;

/// Canonical required CI gate steps in exact order (single source of truth).
const REQUIRED_CI_STEPS: &[&str] = &[
    "check-env-safety",
    "check-backup-artifacts",
    "deps",
    "format",
    "type-check",
    "lint",
    "test",
    "build",
    "generate",
    "install",
];

/// Canonical required macos-ci dependencies in exact order (single source of truth).
const REQUIRED_MACOS_CI_DEPS: &[&str] = &["macos-preflight", "ci", "macos-build", "macos-test"];

/// Generate the human-readable CI pipeline text from canonical constant.
fn required_ci_pipeline_text() -> String {
    REQUIRED_CI_STEPS.join(" → ")
}

/// Parse a top-level Makefile target declaration (`target: deps...`).
fn parse_target_declaration(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(".PHONY") {
        return None;
    }

    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }

    let colon_idx = trimmed.find(':')?;
    // Skip variable assignments like `FOO := bar`.
    if trimmed.as_bytes().get(colon_idx + 1) == Some(&b'=') {
        return None;
    }

    let target = trimmed[..colon_idx].trim();
    let deps = trimmed[colon_idx + 1..].trim();

    if target.is_empty() {
        return None;
    }

    Some((target, deps))
}

/// Parse dependency tokens from a Makefile dependency fragment.
fn parse_dependency_tokens(fragment: &str) -> Vec<String> {
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

/// Collect target dependencies from a target header and any backslash-continued lines.
fn collect_target_dependencies(
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

/// Extract `<target>` from a `$(MAKE)` command invocation in a recipe line.
fn parse_make_invocation_target(line: &str) -> Option<String> {
    let make_index = line.find("$(MAKE)")?;
    let invocation = &line[make_index + "$(MAKE)".len()..];

    invocation
        .split_whitespace()
        .find(|token| !token.starts_with('-') && !token.contains('='))
        .map(|token| token.trim_end_matches('\\').to_string())
}

fn resolve_make_command() -> Result<String> {
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
        // Typical: "GNU Make 4.4.1"
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

#[test]
fn test_make_clean_removes_temp_artifacts() -> Result<()> {
    let temp_dir = TempDir::new().context("create temp dir")?;
    let repo_root = temp_dir.path();

    // Create minimal Cargo workspace structure
    std::fs::write(
        repo_root.join("Cargo.toml"),
        r#"[workspace]
members = []
"#,
    )
    .context("write Cargo.toml")?;

    std::fs::write(
        repo_root.join("Cargo.lock"),
        "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 3\n",
    )
    .context("write Cargo.lock")?;

    // Copy Makefile to temp directory
    let makefile_source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?
        .join("Makefile");
    std::fs::copy(&makefile_source, repo_root.join("Makefile")).context("copy Makefile")?;

    // Create Ralph structure
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;

    // Create temp directories with sample files
    std::fs::create_dir_all(ralph_dir.join("cache/plans"))?;
    std::fs::write(ralph_dir.join("cache/test_plan.md"), "test").context("write test plan")?;

    std::fs::create_dir_all(ralph_dir.join("cache/completions"))?;
    std::fs::write(ralph_dir.join("cache/completions/test.json"), "{}")
        .context("write completion signal")?;

    std::fs::create_dir_all(ralph_dir.join("lock"))?;
    std::fs::write(ralph_dir.join("lock/owner"), "test").context("write lock file")?;

    std::fs::create_dir_all(ralph_dir.join("logs"))?;
    std::fs::write(ralph_dir.join("logs/test.log"), "test log").context("write log file")?;

    // Create user data
    std::fs::write(ralph_dir.join("queue.json"), "[]").context("write queue.json")?;
    std::fs::write(ralph_dir.join("done.json"), "[]").context("write done.json")?;
    std::fs::write(ralph_dir.join("config.json"), "{}").context("write config.json")?;

    // Create prompts override
    std::fs::create_dir_all(ralph_dir.join("prompts")).context("create prompts dir")?;
    std::fs::write(ralph_dir.join("prompts/worker.md"), "# override")
        .context("write prompt override")?;

    // Create README
    std::fs::write(ralph_dir.join("README.md"), "# Ralph").context("write README")?;

    // Run make clean
    let make_cmd = resolve_make_command().context("resolve make command")?;
    let status = Command::new(&make_cmd)
        .arg("clean")
        .current_dir(repo_root)
        .status()
        .with_context(|| format!("run {make_cmd} clean"))?;
    assert!(status.success(), "{make_cmd} clean should succeed");

    // Verify temp directories removed (except completion signals)
    assert!(
        !ralph_dir.join("cache/plans").exists(),
        "cache plans directory should be removed"
    );
    assert!(
        !ralph_dir.join("cache/test_plan.md").exists(),
        "cache test_plan.md should be removed"
    );
    assert!(
        ralph_dir.join("cache/completions").exists(),
        "completion signals directory should be preserved"
    );
    assert!(
        ralph_dir.join("cache/completions/test.json").exists(),
        "completion signal should be preserved"
    );
    assert!(
        !ralph_dir.join("lock").exists(),
        "lock directory should be removed"
    );
    assert!(
        !ralph_dir.join("logs").exists(),
        "logs directory should be removed"
    );

    // Verify user data preserved
    assert!(
        ralph_dir.join("queue.json").exists(),
        "queue.json should be preserved"
    );
    assert!(
        ralph_dir.join("done.json").exists(),
        "done.json should be preserved"
    );
    assert!(
        ralph_dir.join("config.json").exists(),
        "config.json should be preserved"
    );
    assert!(
        ralph_dir.join("prompts").exists(),
        "prompts directory should be preserved"
    );
    assert!(
        ralph_dir.join("prompts/worker.md").exists(),
        "prompt override file should be preserved"
    );
    assert!(
        ralph_dir.join("README.md").exists(),
        "README.md should be preserved"
    );

    Ok(())
}

/// Extract the ordered list of targets from the `ci:` recipe in a Makefile.
/// Handles both modern format (dependencies on the same line) and legacy format
/// (separate $(MAKE) invocations within the target body).
fn extract_make_ci_steps(makefile: &str) -> Result<Vec<String>> {
    let lines: Vec<&str> = makefile.lines().collect();
    let mut ci_header: Option<(usize, &str)> = None;

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

        // If another top-level target begins, stop scanning the ci recipe.
        if let Some((target, _)) = parse_target_declaration(line)
            && target != "ci"
        {
            break;
        }

        // Legacy format: look for $(MAKE) <target> invocations.
        if let Some(target) = parse_make_invocation_target(line) {
            steps.push(target);
        }

        current_index += 1;
    }

    anyhow::ensure!(
        !steps.is_empty(),
        "failed to extract any ci steps from Makefile"
    );
    Ok(steps)
}

/// Extract one target block (`target: ...` plus recipe lines) from a Makefile.
fn extract_target_block(makefile: &str, target: &str) -> Result<String> {
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
            if let Some(colon_idx) = trimmed.find(':') {
                // Skip variable assignments like `FOO := bar`.
                let is_assignment = trimmed.as_bytes().get(colon_idx + 1) == Some(&b'=');
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

/// Extract dependency list from a target header line (e.g., "ci: dep1 dep2").
fn extract_target_dependencies(makefile: &str, target: &str) -> Result<Vec<String>> {
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

#[test]
fn test_extract_make_ci_steps_prefers_ci_target_over_macos_ci() -> Result<()> {
    let makefile = r#"
macos-ci: macos-preflight ci macos-build macos-test

ci: check-env-safety check-backup-artifacts deps format type-check lint test build generate install
	@echo "done"
"#;

    let actual = extract_make_ci_steps(makefile)?;
    let expected: Vec<String> = REQUIRED_CI_STEPS
        .iter()
        .map(|step| (*step).to_string())
        .collect();
    assert_eq!(
        actual, expected,
        "extractor should parse only the `ci` target"
    );

    Ok(())
}

#[test]
fn test_extract_make_ci_steps_supports_multiline_header_dependencies() -> Result<()> {
    let makefile = r#"
ci: check-env-safety \
	check-backup-artifacts \
	deps format \
	type-check lint test build generate install
	@echo "done"
"#;

    let actual = extract_make_ci_steps(makefile)?;
    let expected: Vec<String> = REQUIRED_CI_STEPS
        .iter()
        .map(|step| (*step).to_string())
        .collect();
    assert_eq!(
        actual, expected,
        "extractor should parse multiline ci dependencies"
    );

    Ok(())
}

#[test]
fn test_extract_make_ci_steps_skips_make_flags_in_legacy_recipe() -> Result<()> {
    let makefile = r#"
ci:
	@$(MAKE) --no-print-directory check-env-safety
	@$(MAKE) --no-print-directory check-backup-artifacts
	@$(MAKE) --no-print-directory deps
	@$(MAKE) --no-print-directory format
	@$(MAKE) --no-print-directory type-check
	@$(MAKE) --no-print-directory lint
	@$(MAKE) --no-print-directory test
	@$(MAKE) --no-print-directory build
	@$(MAKE) --no-print-directory generate
	@$(MAKE) --no-print-directory install
"#;

    let actual = extract_make_ci_steps(makefile)?;
    let expected: Vec<String> = REQUIRED_CI_STEPS
        .iter()
        .map(|step| (*step).to_string())
        .collect();
    assert_eq!(
        actual, expected,
        "legacy extractor should parse make target names"
    );

    Ok(())
}

#[test]
fn test_extract_target_dependencies_supports_multiline_header() -> Result<()> {
    let makefile = r#"
macos-ci: macos-preflight \
	ci \
	macos-build \
	macos-test
"#;

    let actual = extract_target_dependencies(makefile, "macos-ci")?;
    let expected: Vec<String> = REQUIRED_MACOS_CI_DEPS
        .iter()
        .map(|step| (*step).to_string())
        .collect();
    assert_eq!(
        actual, expected,
        "target deps extractor should parse multiline headers"
    );

    Ok(())
}

#[test]
fn test_makefile_ci_matches_required_sequence_exactly() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;
    let makefile = std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")?;

    let actual = extract_make_ci_steps(&makefile).context("extract Makefile ci steps")?;
    let expected: Vec<String> = REQUIRED_CI_STEPS.iter().map(|s| s.to_string()).collect();

    assert_eq!(
        actual, expected,
        "Makefile `ci` must exactly match required CI gate sequence.\n\
         Expected: {:?}\n\
         Actual:   {:?}",
        expected, actual
    );

    Ok(())
}

#[test]
fn test_makefile_ci_contains_each_required_step_once() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;
    let makefile = std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")?;

    let actual = extract_make_ci_steps(&makefile).context("extract Makefile ci steps")?;

    for required in REQUIRED_CI_STEPS {
        let count = actual
            .iter()
            .filter(|step| step.as_str() == *required)
            .count();
        assert_eq!(
            count, 1,
            "required ci step `{}` must appear exactly once (found {} times)",
            required, count
        );
    }

    Ok(())
}

#[test]
fn test_macos_ci_matches_required_dependency_sequence() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;
    let makefile = std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")?;

    let actual =
        extract_target_dependencies(&makefile, "macos-ci").context("extract macos-ci deps")?;
    let expected: Vec<String> = REQUIRED_MACOS_CI_DEPS
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        actual, expected,
        "`macos-ci` must exactly match required dependency sequence.\n\
         Expected: {:?}\n\
         Actual:   {:?}",
        expected, actual
    );

    Ok(())
}

#[test]
fn test_contributing_ci_step_list_matches_canonical_pipeline() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;

    let contributing = std::fs::read_to_string(repo_root.join("CONTRIBUTING.md"))
        .context("read CONTRIBUTING.md")?;

    let pipeline = required_ci_pipeline_text();

    assert!(
        contributing.contains(&pipeline),
        "CONTRIBUTING.md CI pipeline must match canonical sequence.\n\
         Expected to find: {}\n",
        pipeline
    );

    Ok(())
}

#[test]
fn test_gemini_ci_step_list_matches_canonical_pipeline() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;

    let gemini = std::fs::read_to_string(repo_root.join("GEMINI.md")).context("read GEMINI.md")?;

    let pipeline = required_ci_pipeline_text();

    assert!(
        gemini.contains(&pipeline),
        "GEMINI.md CI pipeline must match canonical sequence.\n\
         Expected to find: {}\n",
        pipeline
    );

    Ok(())
}

#[test]
fn test_lint_is_non_mutating_and_lint_fix_is_opt_in() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;
    let makefile = std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")?;

    let lint_block = extract_target_block(&makefile, "lint").context("extract lint block")?;
    assert!(
        lint_block.contains("cargo clippy"),
        "lint target should run cargo clippy"
    );
    assert!(
        !lint_block.contains("--fix"),
        "lint target must be non-mutating"
    );

    let lint_fix_block =
        extract_target_block(&makefile, "lint-fix").context("extract lint-fix block")?;
    assert!(
        lint_fix_block.contains("cargo clippy"),
        "lint-fix target should run cargo clippy"
    );
    assert!(
        lint_fix_block.contains("--fix"),
        "lint-fix target should include --fix"
    );

    Ok(())
}

#[test]
fn test_makefile_test_target_uses_nextest_and_keeps_doc_tests() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;
    let makefile = std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")?;

    let test_block = extract_target_block(&makefile, "test").context("extract test block")?;
    assert!(
        test_block
            .contains("cargo nextest run --workspace --all-targets --locked -- --include-ignored"),
        "test target should run cargo-nextest for non-doc tests"
    );
    assert!(
        test_block.contains("cargo test --workspace --doc --locked -- --include-ignored"),
        "test target should keep explicit doc test coverage"
    );
    assert!(
        test_block.contains("cargo nextest --version >/dev/null 2>&1"),
        "test target should check for nextest availability"
    );
    assert!(
        test_block.contains("cargo test --workspace --all-targets --locked -- --include-ignored"),
        "test target should keep cargo test fallback coverage when nextest is unavailable"
    );
    assert!(
        test_block.contains("cargo install cargo-nextest --locked"),
        "test target should provide install guidance when nextest is missing"
    );

    Ok(())
}

#[test]
fn test_macos_targets_gate_with_preflight_and_isolate_derived_data() -> Result<()> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")?;
    let makefile = std::fs::read_to_string(repo_root.join("Makefile")).context("read Makefile")?;

    assert!(
        makefile.contains("macos-preflight:"),
        "Makefile should define macos-preflight target"
    );
    assert!(
        makefile.contains("macos-build:\n\t@$(MAKE) --no-print-directory macos-preflight"),
        "macos-build should run macos-preflight first"
    );
    assert!(
        makefile.contains("macos-test:\n\t@$(MAKE) --no-print-directory macos-preflight"),
        "macos-test should run macos-preflight first"
    );
    assert!(
        makefile.contains("macos-ci: macos-preflight"),
        "macos-ci should depend on macos-preflight"
    );
    assert!(
        makefile.contains("derived_data_path=\"$(XCODE_DERIVED_DATA_ROOT)/build\""),
        "macos-build should use an isolated build DerivedData path"
    );
    assert!(
        makefile.contains("derived_data_path=\"$(XCODE_DERIVED_DATA_ROOT)/test\""),
        "macos-test should use an isolated test DerivedData path"
    );
    assert!(
        makefile.contains("rm -rf \"$$derived_data_path\""),
        "macOS targets should clear DerivedData before running xcodebuild"
    );

    Ok(())
}
