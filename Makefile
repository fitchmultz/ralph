# Purpose: Provide the stable top-level Ralph build, test, release, and developer command entrypoint.
# Responsibilities: Define global Make settings, shared variables, public phony targets, help text, and include focused target fragments.
# Scope: Compatibility shell only; target bodies live in mk/*.mk fragments.
# Usage: Run `make <target>` from the repository root.
# Invariants/Assumptions: GNU Make >= 4 parses this file before included fragments, and all public targets remain invokable from this root entrypoint.

RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph
CARGO_PACKAGE_NAME ?= ralph-agent-loop
CARGO_HTTP_MULTIPLEXING ?= false
XCODE_DERIVED_DATA_ROOT ?= target/tmp/xcode-deriveddata
# Pin destination arch to avoid xcodebuild's "first of multiple matching destinations" warning.
# Override if you intentionally want a different destination.
XCODE_DESTINATION ?= platform=macOS,arch=$(shell uname -m)
# Local CI validates the host architecture that XCTest can execute. Release artifact
# packaging remains responsible for any multi-architecture distribution builds.
XCODE_ARCHS ?= $(shell uname -m)
# UI tests: Set to 1 to include UI tests (headed, mouse-interactive), 0 to skip (default for CI)
RALPH_UI_TESTS ?= 0
# UI screenshots: opt-in evidence capture for headed macOS UI tests.
RALPH_UI_SCREENSHOTS ?= 0
# UI screenshot mode: off|checkpoints|timeline (empty lets tests decide from RALPH_UI_SCREENSHOTS).
RALPH_UI_SCREENSHOT_MODE ?=
# Optional focused UI test selector for retest loops.
RALPH_UI_ONLY_TESTING ?=
# Result bundle path override for UI evidence export workflows.
XCODE_RESULT_BUNDLE_PATH ?=
# Root directory for exported UI visual artifacts.
RALPH_UI_ARTIFACTS_ROOT ?= target/ui-artifacts
MACOS_APP_INSTALL_DIR ?= /Applications
XCODE_BUILD_LOCK_DIR ?= target/tmp/locks/xcodebuild.lock
# Default to tool-managed Rust/nextest parallelism for fastest local iteration.
# Set an explicit cap (for example `RALPH_CI_JOBS=4`) on shared workstations.
RALPH_CI_JOBS ?= 0
# Default to xcodebuild-managed parallelism for best local throughput.
# Set an explicit cap (for example `RALPH_XCODE_JOBS=4`) on shared workstations.
RALPH_XCODE_JOBS ?= 0
# Build stamp path to avoid duplicate release builds in a single make invocation.
RALPH_STAMP_DIR ?= target/tmp/stamps
RALPH_RELEASE_BUILD_STAMP := $(RALPH_STAMP_DIR)/ralph-release-build.stamp
# Inputs that affect the release CLI binary; when newer than the stamp, `make build` re-runs `ralph-cli-bundle.sh`.
RALPH_RELEASE_STAMP_INPUTS := Cargo.toml Cargo.lock VERSION rust-toolchain.toml scripts/ralph-cli-bundle.sh
RALPH_CRATE_SOURCE_FILES := $(shell find crates -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' \) 2>/dev/null | LC_ALL=C sort)
# Set to 1 to keep Xcode derived data between runs (faster local iteration; less pristine than default).
RALPH_XCODE_KEEP_DERIVED_DATA ?= 0
# Internal ship-gate toggle: reuse one derived-data tree across macos-build/test/contracts.
RALPH_XCODE_REUSE_SHIP_DERIVED_DATA ?= 0
# Prefer the rustup-managed pinned toolchain from rust-toolchain.toml when present.
RALPH_RUST_TOOLCHAIN_FILE := rust-toolchain.toml
RALPH_PINNED_RUST_TOOLCHAIN := $(shell sed -n 's/^[[:space:]]*channel = "\(.*\)"/\1/p' $(RALPH_RUST_TOOLCHAIN_FILE) 2>/dev/null | head -1)
RALPH_PINNED_RUSTC := $(shell if command -v rustup >/dev/null 2>&1 && [ -n "$(RALPH_PINNED_RUST_TOOLCHAIN)" ]; then rustup which rustc --toolchain "$(RALPH_PINNED_RUST_TOOLCHAIN)" 2>/dev/null; fi)
RALPH_PINNED_RUST_BIN_DIR := $(patsubst %/,%,$(dir $(RALPH_PINNED_RUSTC)))
# Command prefix placeholder for consistency across targets.
RALPH_ENV_RESET := :
ifneq ($(strip $(RALPH_PINNED_RUST_BIN_DIR)),)
RALPH_ENV_RESET := export PATH="$(RALPH_PINNED_RUST_BIN_DIR):$$PATH"; export RUSTC="$(RALPH_PINNED_RUSTC)"
endif

CARGO_JOBS_FLAG := $(if $(filter-out 0,$(RALPH_CI_JOBS)),--jobs $(RALPH_CI_JOBS),)
NEXTEST_JOBS_FLAG := $(if $(filter-out 0,$(RALPH_CI_JOBS)),--jobs $(RALPH_CI_JOBS),)
CARGO_TEST_THREADS_FLAG := $(if $(filter-out 0,$(RALPH_CI_JOBS)),--test-threads $(RALPH_CI_JOBS),)
XCODE_JOBS_FLAG := $(if $(filter-out 0,$(RALPH_XCODE_JOBS)),-jobs $(RALPH_XCODE_JOBS),)
XCODE_ACTIVE_ARCH_FLAGS := ARCHS=$(XCODE_ARCHS) ONLY_ACTIVE_ARCH=YES
RALPH_CLI_BUILD_JOBS_ARG := $(if $(filter-out 0,$(RALPH_CI_JOBS)),--jobs $(RALPH_CI_JOBS),)
XCODE_MACOS_BUILD_DERIVED_DATA_PATH := $(if $(filter 1,$(RALPH_XCODE_REUSE_SHIP_DERIVED_DATA)),$(XCODE_DERIVED_DATA_ROOT)/ship,$(XCODE_DERIVED_DATA_ROOT)/build)
XCODE_MACOS_TEST_DERIVED_DATA_PATH := $(if $(filter 1,$(RALPH_XCODE_REUSE_SHIP_DERIVED_DATA)),$(XCODE_DERIVED_DATA_ROOT)/ship,$(XCODE_DERIVED_DATA_ROOT)/test)
XCODE_MACOS_RELEASE_APP_BUNDLE := $(XCODE_MACOS_BUILD_DERIVED_DATA_PATH)/Build/Products/Release/RalphMac.app

.DELETE_ON_ERROR:
.ONESHELL:
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c

# Require GNU Make >= 4.x (Homebrew `make` provides `gmake`, plus a `make` shim under `.../gnubin`).
ifeq ($(filter 4.% 5.%,$(MAKE_VERSION)),)
$(error GNU Make >= 4 is required (found: $(MAKE_VERSION)). On macOS: `brew install make` then run `gmake <target>` or add Homebrew gnubin to PATH (Apple Silicon: /opt/homebrew/opt/make/libexec/gnubin, Intel: /usr/local/opt/make/libexec/gnubin).)
endif

MAKEFLAGS += --warn-undefined-variables
MAKEFLAGS += --no-builtin-rules

.PHONY: help install install-verify macos-install-app update lint lint-fix format format-check type-check clean clean-temp test generate docs build ci ci-fast ci-docs deps \
	changelog changelog-preview changelog-check version-check version-sync publish-check release release-dry-run release-verify release-artifacts pre-commit pre-public-check release-gate \
	profile-ship-gate profile-ship-gate-clean agent-ci check-env-safety check-backup-artifacts check-file-size-limits check-repo-safety macos-preflight macos-build macos-test macos-ci macos-test-ui \
	macos-ui-build-for-testing macos-ui-retest macos-test-ui-artifacts macos-ui-artifacts-clean \
	macos-test-window-shortcuts macos-test-contracts macos-test-settings-smoke macos-test-workspace-routing-contract coverage coverage-clean
help:
	@echo "Everyday commands:"
	@echo "  make agent-ci    # Required pre-commit gate: routes from the current local diff"
	@echo "  make release-gate # Heaviest final gate: macOS when available, otherwise Rust-only"
	@echo "  make pre-public-check # Publication audit + full local CI"
	@echo "  make install      # Install release CLI; on macOS also installs RalphMac.app"
	@echo ""
	@echo "Lower-level / power-user gates:"
	@echo "  make ci-docs     # Docs/community-only gate with markdown and path checks"
	@echo "  make ci-fast     # Fast deterministic Rust/CLI gate for day-to-day development"
	@echo "  make ci          # Full Rust release gate (ci-fast + build/generate/install verification)"
	@echo "  make macos-ci     # Rust gate + macOS app build+test + deterministic contract smoke (requires Xcode)"
	@echo "  make test         # Nextest workspace tests + cargo doc tests (auto-fallback if nextest missing)"
	@echo "  make coverage     # Generate code coverage report (requires cargo-llvm-cov)"
	@echo "  make coverage-clean  # Remove coverage artifacts"
	@echo "  make macos-test-window-shortcuts # Run focused multi-window shortcut UI regressions"
	@echo "  make macos-test-contracts # Run deterministic non-XCTest macOS contract checks"
	@echo "  make macos-test-settings-smoke # Run noninteractive Settings open-path contract coverage"
	@echo "  make macos-test-workspace-routing-contract # Run noninteractive workspace routing contract coverage"
	@echo "  make macos-ui-build-for-testing # Build/sign UI test bundles once for local iteration"
	@echo "  make macos-ui-retest         # Re-run UI tests without rebuilding bundles"
	@echo "  make macos-test-ui-artifacts # Run UI suite with xcresult capture + summary"
	@echo "  make macos-ui-artifacts-clean # Remove captured UI visual artifacts"
	@echo "  make profile-ship-gate # Capture canonical local ship-gate profiling bundle (requires Xcode)"
	@echo "  make profile-ship-gate-clean # Remove ship-gate profiling bundles"
	@echo "  make lint         # Clippy with -D warnings"
	@echo "  make generate     # Regenerate committed JSON schemas via release binary"
	@echo "  make update       # Update Rust deps to latest stable; use release-gate/macos-ci to verify the app toolchain"
	@echo "  make macos-install-app # Copy latest Release RalphMac.app into Applications"
	@echo "  make version-check # Verify VERSION, Cargo, and Xcode version metadata are synchronized"
	@echo "  make version-sync VERSION=x.y.z # Sync repo version metadata from one canonical semver"
	@echo "  make publish-check # Run cargo package review + crates.io dry-run for $(CARGO_PACKAGE_NAME)"
	@echo "  make release-verify VERSION=x.y.z # Prepare the exact local release snapshot that make release will publish"
	@echo "  make check-repo-safety # Fast required-files + env/runtime + secret checks"
	@echo "  make check-file-size-limits # Enforce warn-on-soft/fail-on-hard file-size guardrail"
	@echo ""
	@echo "Resource knobs (optional):"
	@echo "  RALPH_CI_JOBS=4     # Example cap for shared workstations (0 = tool default, fastest local iteration)"
	@echo "  RALPH_XCODE_JOBS=4  # Example cap for shared workstations (0 = xcodebuild default)"
	@echo "  XCODE_ARCHS=$$(uname -m) # Host-arch Xcode CI/test builds (override only for cross-arch validation)"
	@echo "  rust-toolchain.toml is respected automatically when rustup is available"
	@echo "  RALPH_UI_SCREENSHOT_MODE=timeline # off|checkpoints|timeline (for macos-ui-retest debugging)"
	@echo "  RALPH_UI_ONLY_TESTING=RalphMacUITests/RalphMacUILaunchAndTaskFlowTests/test_createNewTask_viaQuickCreate # Target macOS UI retests"
	@echo "  RALPH_UI_ARTIFACTS_ROOT=target/ui-artifacts # Export root for visual artifacts"
	@echo "  RALPH_XCODE_KEEP_DERIVED_DATA=1 # Keep Xcode incremental caches (default 0 = clean derived data per gate)"
	@echo "  RALPH_AGENT_CI_MIN_TIER=macos-ci|ci|ci-fast # Floor for agent-ci routing (optional)"
	@echo "  RALPH_AGENT_CI_FORCE_MACOS=1 # Always run macos-ci from agent-ci"

include mk/rust.mk
include mk/repo-safety.mk
include mk/ci.mk
include mk/macos.mk
include mk/coverage.mk
