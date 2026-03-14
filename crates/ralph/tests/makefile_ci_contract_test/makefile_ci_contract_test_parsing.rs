//! Makefile parser and extraction contract tests.
//!
//! Responsibilities:
//! - Cover suite-local Makefile parsing helpers on inline fixtures.
//! - Verify CI alias expansion and multiline dependency parsing.
//! - Keep parser expectations isolated from repo-wide contract assertions.
//!
//! Not handled here:
//! - Real-repo CI sequence parity.
//! - Routing or release-help assertions.
//!
//! Invariants/assumptions:
//! - Inline fixtures intentionally represent supported Makefile shapes.
//! - Parser helpers are defined in the adjacent support module only.

use anyhow::Result;

use super::makefile_ci_contract_test_support::{
    REQUIRED_CI_STEPS, REQUIRED_MACOS_CI_DEPS, REQUIRED_MACOS_TEST_CONTRACT_DEPS,
    extract_make_ci_steps, extract_target_dependencies,
};

#[test]
fn test_extract_make_ci_steps_prefers_ci_target_over_macos_ci() -> Result<()> {
    let makefile = r#"
macos-ci: macos-preflight ci macos-build macos-test macos-test-contracts
macos-test-contracts: macos-test-settings-smoke

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
fn test_extract_make_ci_steps_expands_ci_fast_dependencies() -> Result<()> {
    let makefile = r#"
ci-fast: check-env-safety check-backup-artifacts deps format type-check lint test
ci: ci-fast build generate install
"#;

    let actual = extract_make_ci_steps(makefile)?;
    let expected: Vec<String> = REQUIRED_CI_STEPS
        .iter()
        .map(|step| (*step).to_string())
        .collect();
    assert_eq!(
        actual, expected,
        "extractor should expand ci-fast alias to semantic ci steps"
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
	macos-test \
	macos-test-contracts
macos-test-contracts: macos-test-settings-smoke
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
fn test_extract_target_dependencies_parses_macos_test_contracts_target() -> Result<()> {
    let makefile = r#"
macos-test-contracts: macos-test-settings-smoke
"#;

    let actual = extract_target_dependencies(makefile, "macos-test-contracts")?;
    let expected: Vec<String> = REQUIRED_MACOS_TEST_CONTRACT_DEPS
        .iter()
        .map(|step| (*step).to_string())
        .collect();
    assert_eq!(
        actual, expected,
        "target deps extractor should parse macos contract dependency headers"
    );

    Ok(())
}
