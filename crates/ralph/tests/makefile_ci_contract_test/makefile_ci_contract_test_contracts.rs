//! Repository-wide CI contract assertions.
//!
//! Responsibilities:
//! - Verify canonical CI, fast CI, and macOS dependency sequences in the repo Makefile.
//! - Verify contributor docs and release help stay synchronized with canonical contracts.
//! - Verify target blocks preserve non-mutating lint and expected test orchestration.
//!
//! Not handled here:
//! - Inline parser fixture coverage.
//! - Clean-target smoke behavior.
//!
//! Invariants/assumptions:
//! - Assertions read the repo's tracked Makefile and docs from disk.
//! - Canonical sequences come from the suite support module.

use anyhow::{Context, Result};

use super::makefile_ci_contract_test_support::{
    REQUIRED_CI_FAST_STEPS, REQUIRED_CI_STEPS, REQUIRED_MACOS_CI_DEPS,
    REQUIRED_MACOS_TEST_CONTRACT_DEPS, extract_make_ci_steps, extract_target_block,
    extract_target_dependencies, read_repo_makefile, repo_root, required_ci_pipeline_text,
};

#[test]
fn test_makefile_ci_matches_required_sequence_exactly() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let actual = extract_make_ci_steps(&makefile).context("extract Makefile ci steps")?;
    let expected: Vec<String> = REQUIRED_CI_STEPS
        .iter()
        .map(|step| step.to_string())
        .collect();

    assert_eq!(
        actual, expected,
        "Makefile `ci` must exactly match required CI gate sequence.\nExpected: {:?}\nActual:   {:?}",
        expected, actual
    );

    Ok(())
}

#[test]
fn test_makefile_ci_fast_matches_required_subset() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let actual =
        extract_target_dependencies(&makefile, "ci-fast").context("extract ci-fast deps")?;
    let expected: Vec<String> = REQUIRED_CI_FAST_STEPS
        .iter()
        .map(|step| step.to_string())
        .collect();

    assert_eq!(
        actual, expected,
        "`ci-fast` must exactly match required fast CI subset.\nExpected: {:?}\nActual:   {:?}",
        expected, actual
    );

    Ok(())
}

#[test]
fn test_makefile_ci_contains_each_required_step_once() -> Result<()> {
    let makefile = read_repo_makefile()?;
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
    let makefile = read_repo_makefile()?;
    let actual =
        extract_target_dependencies(&makefile, "macos-ci").context("extract macos-ci deps")?;
    let expected: Vec<String> = REQUIRED_MACOS_CI_DEPS
        .iter()
        .map(|step| step.to_string())
        .collect();

    assert_eq!(
        actual, expected,
        "`macos-ci` must exactly match required dependency sequence.\nExpected: {:?}\nActual:   {:?}",
        expected, actual
    );

    Ok(())
}

#[test]
fn test_macos_test_contracts_matches_required_dependency_sequence() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let actual = extract_target_dependencies(&makefile, "macos-test-contracts")
        .context("extract macos-test-contracts deps")?;
    let expected: Vec<String> = REQUIRED_MACOS_TEST_CONTRACT_DEPS
        .iter()
        .map(|step| step.to_string())
        .collect();

    assert_eq!(
        actual, expected,
        "`macos-test-contracts` must exactly match required dependency sequence.\nExpected: {:?}\nActual:   {:?}",
        expected, actual
    );

    Ok(())
}

#[test]
fn test_contributing_ci_step_list_matches_canonical_pipeline() -> Result<()> {
    let repo_root = repo_root()?;
    let contributing = std::fs::read_to_string(repo_root.join("CONTRIBUTING.md"))
        .context("read CONTRIBUTING.md")?;
    let pipeline = required_ci_pipeline_text();

    assert!(
        contributing.contains(&pipeline),
        "CONTRIBUTING.md CI pipeline must match canonical sequence.\nExpected to find: {}\n",
        pipeline
    );

    Ok(())
}

#[test]
fn test_lint_is_non_mutating_and_lint_fix_is_opt_in() -> Result<()> {
    let makefile = read_repo_makefile()?;

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
    let makefile = read_repo_makefile()?;
    let test_block = extract_target_block(&makefile, "test").context("extract test block")?;

    assert!(
        test_block.contains("cargo nextest run --workspace --all-targets --locked"),
        "test target should run cargo-nextest for non-doc tests"
    );
    assert!(
        test_block.contains("cargo test --workspace --doc --locked"),
        "test target should keep explicit doc test coverage"
    );
    assert!(
        test_block.contains("cargo nextest --version >/dev/null 2>&1"),
        "test target should check for nextest availability"
    );
    assert!(
        test_block.contains("cargo test --workspace --all-targets --locked"),
        "test target should keep cargo test fallback coverage when nextest is unavailable"
    );
    assert!(
        test_block.contains("-- --include-ignored"),
        "test target should keep include-ignored coverage for workspace and doc tests"
    );
    assert!(
        test_block.contains("cargo install cargo-nextest --locked"),
        "test target should provide install guidance when nextest is missing"
    );

    Ok(())
}

#[test]
fn test_release_verify_target_orchestrates_release_preflight() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let release_verify_block = extract_target_block(&makefile, "release-verify")
        .context("extract release-verify block")?;

    assert!(
        makefile.contains("release-verify"),
        "Makefile should define a release-verify target"
    );
    assert!(
        release_verify_block.contains("Usage: make release-verify VERSION=x.y.z"),
        "release-verify should require VERSION explicitly"
    );
    assert!(
        release_verify_block.contains("scripts/release.sh verify \"$(VERSION)\""),
        "release-verify should delegate to the transactional verify command"
    );
    assert!(
        !release_verify_block.contains("./scripts/versioning.sh sync --version \"$(VERSION)\"")
            && !release_verify_block.contains("./scripts/versioning.sh check")
            && !release_verify_block
                .contains("scripts/pre-public-check.sh --skip-ci --release-context")
            && !release_verify_block.contains("$(MAKE) --no-print-directory release-gate"),
        "release-verify should not duplicate the heavy preflight steps that the transactional verify command now owns"
    );
    assert!(
        makefile.contains("make release-verify VERSION=x.y.z"),
        "Makefile help output should advertise release-verify"
    );

    Ok(())
}
