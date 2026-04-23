//! Makefile routing and macOS gate contract tests.
//!
//! Responsibilities:
//! - Verify agent-ci routing and surface-classifier delegation.
//! - Verify macOS targets depend on preflight and isolate DerivedData state.
//! - Verify Makefile bootstraps the pinned rustup toolchain.
//!
//! Not handled here:
//! - Canonical CI step sequence equality.
//! - Inline parser fixtures or clean-target smoke tests.
//!
//! Invariants/assumptions:
//! - Routing contracts are asserted against the repo Makefile block text.
//! - macOS target assertions treat lock orchestration as part of the public contract.

use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::makefile_ci_contract_test_support::{
    extract_target_block, extract_target_dependencies, read_repo_makefile, repo_root,
};
use std::os::unix::fs::PermissionsExt;

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_bash_script(script: &str) -> Result<()> {
    let output = Command::new("bash")
        .arg("-lc")
        .arg(script)
        .output()
        .context("run bash script")?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
        "bash script failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |code| code.to_string()),
        stdout,
        stderr
    )
}

fn xcode_lock_helper_script() -> Result<String> {
    let helper_path = repo_root()?.join("scripts/lib/xcodebuild-lock.sh");
    Ok(shell_quote(&helper_path.display().to_string()))
}

#[test]
fn test_macos_targets_gate_with_preflight_and_isolate_derived_data() -> Result<()> {
    let makefile = read_repo_makefile()?;

    assert!(
        makefile.contains("macos-preflight:"),
        "Makefile should define macos-preflight target"
    );

    let macos_build_deps = extract_target_dependencies(&makefile, "macos-build")
        .context("extract macos-build deps")?;
    assert!(
        macos_build_deps.contains(&"macos-preflight".to_string()),
        "macos-build should depend on macos-preflight"
    );

    let macos_test_deps =
        extract_target_dependencies(&makefile, "macos-test").context("extract macos-test deps")?;
    assert!(
        macos_test_deps.contains(&"macos-preflight".to_string()),
        "macos-test should depend on macos-preflight"
    );

    assert!(
        makefile.contains("macos-ci: macos-preflight"),
        "macos-ci should depend on macos-preflight"
    );
    assert!(
        makefile.contains(
            "macos-test-contracts: macos-test-settings-smoke macos-test-workspace-routing-contract"
        ),
        "Makefile should define a deterministic macOS contract aggregate target"
    );
    assert!(
        makefile.contains(
            "XCODE_MACOS_BUILD_DERIVED_DATA_PATH := $(if $(filter 1,$(RALPH_XCODE_REUSE_SHIP_DERIVED_DATA)),$(XCODE_DERIVED_DATA_ROOT)/ship,$(XCODE_DERIVED_DATA_ROOT)/build)"
        ),
        "Makefile should define the macOS build DerivedData path selector"
    );
    assert!(
        makefile.contains(
            "XCODE_MACOS_TEST_DERIVED_DATA_PATH := $(if $(filter 1,$(RALPH_XCODE_REUSE_SHIP_DERIVED_DATA)),$(XCODE_DERIVED_DATA_ROOT)/ship,$(XCODE_DERIVED_DATA_ROOT)/test)"
        ),
        "Makefile should define the macOS test DerivedData path selector"
    );
    assert!(
        makefile.contains("rm -rf \"$$derived_data_path\""),
        "macOS targets should clear DerivedData before running xcodebuild"
    );
    assert!(
        makefile.contains("shared_derived_data_path=\"$(XCODE_DERIVED_DATA_ROOT)/ship\""),
        "macos-ci should reuse a shared ship-gate DerivedData path"
    );
    assert!(
        makefile.contains("RALPH_XCODE_REUSE_SHIP_DERIVED_DATA=1"),
        "macos-ci should pass the shared DerivedData toggle to its subtargets"
    );
    assert!(
        makefile.contains("XCODE_BUILD_LOCK_DIR ?= target/tmp/locks/xcodebuild.lock"),
        "Makefile should define a dedicated Xcode build lock path"
    );
    assert!(
        makefile.contains("RALPH_XCODE_JOBS ?= 0"),
        "Makefile should default to xcodebuild-managed parallelism unless operators set an explicit cap"
    );
    let xcode_lock_helper =
        std::fs::read_to_string(repo_root()?.join("scripts/lib/xcodebuild-lock.sh"))
            .context("read shared Xcode build lock helper")?;
    assert!(
        xcode_lock_helper.contains("Waiting for Xcode build lock"),
        "macOS Xcode targets should report lock contention"
    );
    assert!(
        xcode_lock_helper.contains("Removing stale Xcode build lock"),
        "macOS Xcode targets should recover stale project-owned build locks"
    );

    for target in [
        "macos-build",
        "macos-test",
        "macos-ui-build-for-testing",
        "macos-ui-retest",
        "macos-test-window-shortcuts",
    ] {
        let block = extract_target_block(&makefile, target)
            .with_context(|| format!("extract {target} block"))?;
        assert!(
            block.contains("lock_dir=\"$(XCODE_BUILD_LOCK_DIR)\""),
            "{target} should configure the shared Xcode build lock path"
        );
        assert!(
            block.contains("source scripts/lib/xcodebuild-lock.sh"),
            "{target} should source the shared Xcode build lock helper"
        );
        assert!(
            block.contains("ralph_acquire_xcode_build_lock"),
            "{target} should acquire the shared Xcode build lock through the helper"
        );
        assert!(
            block.contains("ralph_release_xcode_build_lock"),
            "{target} should release the shared Xcode build lock through the helper"
        );
    }

    let macos_build_block =
        extract_target_block(&makefile, "macos-build").context("extract macos-build block")?;
    assert!(
        macos_build_block.contains("derived_data_path=\"$(XCODE_MACOS_BUILD_DERIVED_DATA_PATH)\""),
        "macos-build should resolve its DerivedData path through the shared selector"
    );

    let macos_test_block =
        extract_target_block(&makefile, "macos-test").context("extract macos-test block")?;
    assert!(
        macos_test_block.contains("derived_data_path=\"$(XCODE_MACOS_TEST_DERIVED_DATA_PATH)\""),
        "macos-test should resolve its DerivedData path through the shared selector"
    );

    let ui_delegate_index = macos_test_block
        .find("if [ \"$$include_ui_tests\" = \"1\" ]")
        .context("locate macos-test UI delegation branch")?;
    let helper_index = macos_test_block
        .find("source scripts/lib/xcodebuild-lock.sh")
        .context("locate macos-test lock helper")?;
    assert!(
        ui_delegate_index < helper_index,
        "macos-test should delegate interactive UI coverage before acquiring the shared Xcode build lock"
    );

    for target in ["macos-ui-retest", "macos-test-window-shortcuts"] {
        let block = extract_target_block(&makefile, target)
            .with_context(|| format!("extract {target} cleanup block"))?;
        assert!(
            block.contains("app_binary="),
            "{target} should track the launched RalphMac test app binary for cleanup"
        );
        assert!(
            block.contains("runner_binary="),
            "{target} should track the launched UI test runner binary for cleanup"
        );
        assert!(
            block.contains("pkill -TERM -f \"$$runner_binary\""),
            "{target} should terminate any lingering UI test runner before exiting"
        );
        assert!(
            block.contains("pkill -TERM -f \"$$app_binary\""),
            "{target} should terminate any lingering RalphMac UI test app before exiting"
        );
        assert!(
            block.contains("left a lingering UI test app or runner process"),
            "{target} should fail loudly if UI test processes survive the run"
        );
    }

    let shortcuts_block = extract_target_block(&makefile, "macos-test-window-shortcuts")
        .context("extract macos-test-window-shortcuts selectors")?;
    assert!(
        shortcuts_block.contains("RalphMacUITests/RalphMacUIWindowRoutingTests/test_windowShortcuts_affectOnlyFocusedWindow"),
        "macos-test-window-shortcuts should target the focused-window routing suite"
    );
    assert!(
        shortcuts_block.contains("RalphMacUITests/RalphMacUIWindowRoutingTests/test_commandPaletteNewTab_affectsOnlyFocusedWindow"),
        "macos-test-window-shortcuts should target the focused-window command-palette routing suite"
    );

    Ok(())
}

#[test]
fn test_xcode_lock_helper_recovers_ownerless_and_dead_owner_locks() -> Result<()> {
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let ownerless_lock_dir = temp_dir.path().join("target/tmp/locks/ownerless.lock");
    let dead_owner_lock_dir = temp_dir.path().join("target/tmp/locks/dead-owner.lock");
    let helper_script = xcode_lock_helper_script()?;

    let script = format!(
        r#"set -euo pipefail
source {helper_script}

ownerless_lock_dir={ownerless_lock_dir}
mkdir -p "$ownerless_lock_dir"
touch -t 202001010000 "$ownerless_lock_dir"
ralph_acquire_xcode_build_lock "$ownerless_lock_dir" "ownerless-stale"
grep -q '^label: ownerless-stale$' "$(ralph_xcode_build_lock_owner_file "$ownerless_lock_dir")"
ralph_release_xcode_build_lock "$ownerless_lock_dir"
[ ! -d "$ownerless_lock_dir" ]

dead_owner_lock_dir={dead_owner_lock_dir}
mkdir -p "$dead_owner_lock_dir"
cat >"$(ralph_xcode_build_lock_owner_file "$dead_owner_lock_dir")" <<'EOF'
pid: 999999
started_at: 2026-03-28T00:00:00Z
command: make macos-build
label: stale-build
EOF
ralph_acquire_xcode_build_lock "$dead_owner_lock_dir" "dead-owner-stale"
grep -q '^label: dead-owner-stale$' "$(ralph_xcode_build_lock_owner_file "$dead_owner_lock_dir")"
ralph_release_xcode_build_lock "$dead_owner_lock_dir"
[ ! -d "$dead_owner_lock_dir" ]
"#,
        helper_script = helper_script,
        ownerless_lock_dir = shell_quote(&ownerless_lock_dir.display().to_string()),
        dead_owner_lock_dir = shell_quote(&dead_owner_lock_dir.display().to_string()),
    );

    run_bash_script(&script)
}

#[test]
fn test_xcode_lock_helper_leaves_live_owner_locks_in_place() -> Result<()> {
    let temp_dir = tempfile::tempdir().context("create temp dir")?;
    let lock_dir = temp_dir.path().join("target/tmp/locks/live-owner.lock");
    let ready_file = temp_dir.path().join("ready");
    let helper_script = xcode_lock_helper_script()?;

    let live_owner_script = format!(
        r#"set -euo pipefail
lock_dir={lock_dir}
ready_file={ready_file}
mkdir -p "$lock_dir"
cat >"$lock_dir/owner" <<EOF
pid: $$
started_at: 2026-03-28T00:00:00Z
command: make macos-build
label: live-build
EOF
touch "$ready_file"
sleep 30
"#,
        lock_dir = shell_quote(&lock_dir.display().to_string()),
        ready_file = shell_quote(&ready_file.display().to_string()),
    );

    let mut child = Command::new("bash")
        .arg("-lc")
        .arg(live_owner_script)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("spawn live owner shell")?;

    let wait_start = Instant::now();
    while !ready_file.exists() {
        if let Some(status) = child.try_wait().context("poll live owner shell")? {
            anyhow::bail!("live owner shell exited before writing owner metadata: {status}");
        }
        if wait_start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("timed out waiting for live owner metadata");
        }
        thread::sleep(Duration::from_millis(50));
    }

    let stale_check = format!(
        r#"set -euo pipefail
source {helper_script}
lock_dir={lock_dir}
if ralph_xcode_build_lock_is_stale "$lock_dir"; then
    echo "$RALPH_XCODE_LOCK_STALE_REASON"
    exit 1
fi
"#,
        helper_script = helper_script,
        lock_dir = shell_quote(&lock_dir.display().to_string()),
    );

    let check_result = run_bash_script(&stale_check);
    let _ = child.kill();
    let _ = child.wait();
    check_result
}

#[test]
fn test_macos_ui_artifact_target_preserves_result_bundle_and_summary() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let block = extract_target_block(&makefile, "macos-test-ui-artifacts")
        .context("extract macos-test-ui-artifacts block")?;

    assert!(
        block.contains("result_bundle_path=\"$$artifact_dir/RalphMacUITests.xcresult\""),
        "macos-test-ui-artifacts should preserve the xcresult bundle"
    );
    assert!(
        block.contains("targeted_test: $${RALPH_UI_ONLY_TESTING:-all}"),
        "macos-test-ui-artifacts summary should record whether the run was targeted"
    );
    assert!(
        !block.contains("xcresulttool export attachments"),
        "macos-test-ui-artifacts should not depend on empty xcresult attachment export"
    );
    assert!(
        !block.contains("screenshots_dir="),
        "macos-test-ui-artifacts should not carry dead screenshot-export plumbing"
    );

    Ok(())
}

#[test]
fn test_profile_ship_gate_targets_delegate_to_script() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let repo = repo_root()?;
    let profile_block = extract_target_block(&makefile, "profile-ship-gate")
        .context("extract profile-ship-gate block")?;
    let clean_block = extract_target_block(&makefile, "profile-ship-gate-clean")
        .context("extract profile-ship-gate-clean block")?;

    // Help text is the public contract — keep these strict.
    assert!(
        makefile.contains("make profile-ship-gate # Capture canonical local ship-gate profiling bundle (requires Xcode)"),
        "Makefile help should advertise the profiling entrypoint"
    );
    assert!(
        makefile.contains("make profile-ship-gate-clean # Remove ship-gate profiling bundles"),
        "Makefile help should advertise the profiling cleanup entrypoint"
    );

    // Targets delegate to the script — verify the script exists and is executable.
    let script_path = repo.join("scripts/profile-ship-gate.sh");
    assert!(
        script_path.exists(),
        "profile-ship-gate script should exist at scripts/profile-ship-gate.sh"
    );
    let script = std::fs::read_to_string(&script_path).context("read profile-ship-gate.sh")?;
    assert!(
        script.contains("ralph_activate_pinned_rust_toolchain"),
        "profile-ship-gate.sh should activate the pinned Rust toolchain via the shared helper"
    );
    assert!(
        !script.contains("RALPH_ENV_RESET"),
        "profile-ship-gate.sh should not depend on the removed env snippet"
    );
    let mode = std::fs::metadata(&script_path)
        .context("stat profile-ship-gate.sh")?
        .permissions()
        .mode();
    assert!(
        mode & 0o111 != 0,
        "profile-ship-gate.sh should be executable"
    );

    // Makefile targets delegate to the script.
    assert!(
        profile_block.contains("scripts/profile-ship-gate.sh run"),
        "profile-ship-gate target should delegate to script run subcommand"
    );
    assert!(
        clean_block.contains("scripts/profile-ship-gate.sh clean"),
        "profile-ship-gate-clean target should delegate to script clean subcommand"
    );

    // profile-ship-gate preserves the macos-preflight dependency at the Makefile level.
    let deps = extract_target_dependencies(&makefile, "profile-ship-gate")
        .context("extract profile-ship-gate dependencies")?;
    assert!(
        deps.contains(&"macos-preflight".to_string()),
        "profile-ship-gate should depend on macos-preflight"
    );

    Ok(())
}

#[test]
fn test_ci_docs_runs_focused_markdown_link_scan() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let ci_docs_block =
        extract_target_block(&makefile, "ci-docs").context("extract ci-docs block")?;
    let pre_public_check =
        std::fs::read_to_string(repo_root()?.join("scripts/pre-public-check.sh"))
            .context("read scripts/pre-public-check.sh")?;

    assert!(
        makefile
            .contains("make ci-docs     # Docs/community-only gate with markdown and path checks"),
        "Makefile help should advertise the docs-check behavior of ci-docs"
    );
    assert!(
        ci_docs_block.contains("bash ./scripts/lib/public_readiness_scan.sh links"),
        "ci-docs should run the focused markdown link scan entrypoint"
    );
    assert!(
        ci_docs_block.contains("bash ./scripts/lib/public_readiness_scan.sh session-paths"),
        "ci-docs should run the documented session-cache path guard"
    );
    assert!(
        !ci_docs_block.contains("pre-public-check.sh --skip-ci --skip-clean --skip-secrets"),
        "ci-docs should not rerun broader pre-public checks via skip-flag combinations"
    );
    assert!(
        makefile.contains(
            "scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean --allow-no-git"
        ),
        "check-env-safety should use source-snapshot-safe pre-public checks"
    );
    assert!(
        pre_public_check.contains("bash \"$SCRIPT_DIR/lib/public_readiness_scan.sh\" links"),
        "pre-public-check should reuse the focused markdown link scan entrypoint"
    );
    assert!(
        pre_public_check
            .contains("bash \"$SCRIPT_DIR/lib/public_readiness_scan.sh\" session-paths"),
        "pre-public-check should reuse the documented session-cache path guard"
    );
    assert!(
        pre_public_check.contains("bash \"$SCRIPT_DIR/lib/public_readiness_scan.sh\" secrets"),
        "pre-public-check should reuse the focused secret scan entrypoint"
    );

    Ok(())
}

#[test]
fn test_agent_ci_routes_between_docs_ci_fast_and_macos_ci() -> Result<()> {
    let makefile = read_repo_makefile()?;
    let agent_ci_block =
        extract_target_block(&makefile, "agent-ci").context("extract agent-ci block")?;

    assert!(
        agent_ci_block.contains("current local diff routing"),
        "agent-ci should explain that routing is based on the current working tree"
    );
    assert!(
        agent_ci_block.contains("full Rust release, macOS ship"),
        "agent-ci should advertise the four-way routing contract"
    );
    assert!(
        agent_ci_block.contains("scripts/agent-ci-surface.sh --emit-eval"),
        "agent-ci must route through the shared dependency-surface classifier"
    );
    assert!(
        agent_ci_block.contains("$(MAKE) --no-print-directory \"$$target_name\""),
        "agent-ci must dispatch to the classifier-selected gate target"
    );
    assert!(
        agent_ci_block.contains("RALPH_AGENT_CI_REASON"),
        "agent-ci should surface the classifier's routing reason"
    );
    assert!(
        agent_ci_block.contains("target_name\" = \"noop\""),
        "agent-ci should no-op when the working tree has no local changes"
    );
    assert!(
        agent_ci_block.contains("using platform-aware release gate fallback"),
        "agent-ci should explain the non-git source-snapshot fallback"
    );
    assert!(
        agent_ci_block.contains("$(MAKE) --no-print-directory release-gate"),
        "agent-ci should use the shared release gate when git metadata is unavailable"
    );
    assert!(
        !agent_ci_block.contains("running macOS gate for safety"),
        "agent-ci should not force the macOS-only gate outside git worktrees"
    );

    Ok(())
}

#[test]
fn test_makefile_auto_prefers_pinned_rustup_toolchain() -> Result<()> {
    let makefile = read_repo_makefile()?;

    assert!(
        makefile.contains("rustup which rustc --toolchain"),
        "Makefile should resolve the pinned rustup toolchain from rust-toolchain.toml"
    );
    assert!(
        makefile.contains("export PATH=\"$(RALPH_PINNED_RUST_BIN_DIR):$$PATH\"; export RUSTC=\"$(RALPH_PINNED_RUSTC)\""),
        "Makefile should inject the pinned Rust toolchain into PATH and RUSTC"
    );

    Ok(())
}
