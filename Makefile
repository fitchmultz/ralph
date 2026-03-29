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
RALPH_CLI_BUILD_JOBS_ARG := $(if $(filter-out 0,$(RALPH_CI_JOBS)),--jobs $(RALPH_CI_JOBS),)

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

.PHONY: help install macos-install-app update lint lint-fix format format-check type-check clean clean-temp test generate docs build ci ci-fast ci-docs deps \
	changelog changelog-preview changelog-check version-check version-sync publish-check release release-dry-run release-verify release-artifacts pre-commit pre-public-check release-gate \
	profile-ship-gate profile-ship-gate-clean agent-ci check-env-safety check-backup-artifacts check-repo-safety macos-preflight macos-build macos-test macos-ci macos-test-ui \
	macos-ui-build-for-testing macos-ui-retest macos-test-ui-artifacts macos-ui-artifacts-clean \
	macos-test-window-shortcuts macos-test-contracts macos-test-settings-smoke macos-test-workspace-routing-contract coverage coverage-clean FORCE
help:
	@echo "Common targets:"
	@echo "  make ci-docs     # Docs/community-only safety gate used by make agent-ci"
	@echo "  make ci-fast     # Fast deterministic Rust/CLI gate for day-to-day development"
	@echo "  make ci          # Full Rust release gate (ci-fast + build/generate/install)"
	@echo "  make agent-ci    # Agent gate: dependency-surface routing between ci-docs, ci-fast, and macos-ci"
	@echo "  make macos-ci     # Rust gate + macOS app build+test + deterministic contract smoke (requires Xcode)"
	@echo "  make release-gate # Canonical ship gate: macOS when available, otherwise Rust-only"
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
	@echo "  make update       # Update Rust deps to latest stable; use macos-ci to verify the bundled Swift app toolchain"
	@echo "  make install      # Install release CLI; on macOS also installs RalphMac.app"
	@echo "  make macos-install-app # Copy latest Release RalphMac.app into Applications"
	@echo "  make version-check # Verify VERSION, Cargo, and Xcode version metadata are synchronized"
	@echo "  make version-sync VERSION=x.y.z # Sync repo version metadata from one canonical semver"
	@echo "  make publish-check # Run cargo package review + crates.io dry-run for $(CARGO_PACKAGE_NAME)"
	@echo "  make release-verify VERSION=x.y.z # Prepare the exact local release snapshot that make release will publish"
	@echo "  make check-repo-safety # Fast required-files + env/runtime + secret checks"
	@echo "  make pre-public-check # Publication audit + full local CI"
	@echo ""
	@echo "Resource knobs (optional):"
	@echo "  RALPH_CI_JOBS=4     # Example cap for shared workstations (0 = tool default, fastest local iteration)"
	@echo "  RALPH_XCODE_JOBS=4  # Example cap for shared workstations (0 = xcodebuild default)"
	@echo "  rust-toolchain.toml is respected automatically when rustup is available"
	@echo "  RALPH_UI_SCREENSHOT_MODE=timeline # off|checkpoints|timeline (for macos-ui-retest debugging)"
	@echo "  RALPH_UI_ONLY_TESTING=RalphMacUITests/RalphMacUILaunchAndTaskFlowTests/test_createNewTask_viaQuickCreate # Target macOS UI retests"
	@echo "  RALPH_UI_ARTIFACTS_ROOT=target/ui-artifacts # Export root for visual artifacts"

FORCE:

$(RALPH_RELEASE_BUILD_STAMP): FORCE
	@mkdir -p "$(RALPH_STAMP_DIR)"
	@echo "→ Release build..."
	@$(RALPH_ENV_RESET); scripts/ralph-cli-bundle.sh --configuration Release $(RALPH_CLI_BUILD_JOBS_ARG) --print-path >/dev/null
	@touch "$(RALPH_RELEASE_BUILD_STAMP)"
	@echo "  ✓ Release build complete"

# Optional but cheap: fail fast if lockfile or network access is busted
deps:
	@echo "→ Fetching deps (locked)..."
	@$(RALPH_ENV_RESET); cargo fetch --locked
	@./scripts/versioning.sh check
	@echo "  ✓ Deps fetched"

install: $(RALPH_RELEASE_BUILD_STAMP)
	@ralph_bin_path="$$(scripts/ralph-cli-bundle.sh --configuration Release $(RALPH_CLI_BUILD_JOBS_ARG) --print-path)"; \
	bin_dir="$(BIN_DIR)"; \
	if [ ! -w "$$bin_dir" ]; then \
		bin_dir="$(HOME)/.local/bin"; \
		echo "install: $(BIN_DIR) not writable; using $$bin_dir"; \
	fi; \
	mkdir -p "$$bin_dir"; \
	install -m 0755 "$$ralph_bin_path" "$$bin_dir/$(BIN_NAME)"; \
	"$$bin_dir/$(BIN_NAME)" --help >/dev/null; \
	if [ "$$(uname -s)" = "Darwin" ] && command -v xcodebuild >/dev/null 2>&1; then \
		$(MAKE) --no-print-directory macos-install-app; \
	fi

update:
	@echo "→ Updating direct dependencies to latest stable requirements..."
	@$(RALPH_ENV_RESET); cargo upgrade --incompatible
	@echo "→ Refreshing lockfile to latest compatible transitive versions..."
	@$(RALPH_ENV_RESET); CARGO_HTTP_MULTIPLEXING=$(CARGO_HTTP_MULTIPLEXING) cargo update
	@echo "  ℹ Swift/Xcode has no external package manifest here; use make macos-ci to verify the app against the current toolchain"
	@echo "  ✓ Dependency update complete"

format:
	@echo "→ Formatting code..."
	@$(RALPH_ENV_RESET); cargo fmt --all
	@echo "  ✓ Formatting complete"

format-check:
	@echo "→ Checking formatting..."
	@$(RALPH_ENV_RESET); cargo fmt --all --check
	@echo "  ✓ Formatting OK"

type-check:
	@echo "→ Type-checking..."
	@$(RALPH_ENV_RESET); cargo check --workspace --all-targets --all-features --locked $(CARGO_JOBS_FLAG)
	@echo "  ✓ Type-checking complete"

lint:
	@echo "→ Linting (clippy, non-mutating)..."
	@$(RALPH_ENV_RESET); cargo clippy --workspace --all-targets --all-features --locked $(CARGO_JOBS_FLAG) -- -D warnings
	@echo "  ✓ Linting complete"

lint-fix:
	@echo "→ Clippy autofix (optional)..."
	@$(RALPH_ENV_RESET); cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --locked $(CARGO_JOBS_FLAG) -- -D warnings
	@echo "  ✓ Lint autofix complete"

test:
	@echo "→ Running tests..."
	@system_tmp="$${TMPDIR:-/tmp}"; \
	system_tmp="$${system_tmp%/}"; \
	run_dir="$$(mktemp -d "$$system_tmp/ralph-ci.XXXXXX")"; \
	cleanup() { \
		if [ "$${RALPH_CI_KEEP_TMP:-0}" = "1" ]; then \
			echo "  ℹ Keeping CI temp dir: $$run_dir"; \
			return 0; \
		fi; \
		rm -rf "$$run_dir" 2>/dev/null || true; \
	}; \
	trap cleanup EXIT INT TERM; \
	export TMPDIR="$$run_dir"; \
	export TEMP="$$run_dir"; \
	export TMP="$$run_dir"; \
	$(RALPH_ENV_RESET); \
	unit_log="$$run_dir/unit-tests.log"; \
	doc_log="$$run_dir/doc-tests.log"; \
	unit_log_content=""; \
	doc_log_content=""; \
	exit_code=0; \
	if cargo nextest --version >/dev/null 2>&1; then \
		echo "  → Using cargo-nextest for non-doc tests"; \
		if cargo nextest run --workspace --all-targets --locked $(NEXTEST_JOBS_FLAG) -- --include-ignored >"$$unit_log" 2>&1; then \
			grep -E "^(test result:|running|     Running|Summary|PASS|FAIL)" "$$unit_log" | tail -5 || true; \
		else \
			unit_log_content="$$(cat "$$unit_log" 2>/dev/null || true)"; \
			echo "  ✗ Workspace tests failed!"; echo ""; echo "=== Full test output ==="; echo "$$unit_log_content"; \
			exit_code=1; \
		fi; \
	else \
		echo "  ⚠ cargo-nextest not found; falling back to cargo test --workspace --all-targets"; \
		echo "    Install with: cargo install cargo-nextest --locked"; \
		if cargo test --workspace --all-targets --locked $(CARGO_JOBS_FLAG) -- --include-ignored $(CARGO_TEST_THREADS_FLAG) >"$$unit_log" 2>&1; then \
			grep -E "^(test result:|running|     Running)" "$$unit_log" || true; \
		else \
			unit_log_content="$$(cat "$$unit_log" 2>/dev/null || true)"; \
			echo "  ✗ Workspace tests failed!"; echo ""; echo "=== Full test output ==="; echo "$$unit_log_content"; \
			exit_code=1; \
		fi; \
	fi; \
	if [ "$$exit_code" -eq 0 ]; then \
		if cargo test --workspace --doc --locked $(CARGO_JOBS_FLAG) -- --include-ignored $(CARGO_TEST_THREADS_FLAG) >"$$doc_log" 2>&1; then \
			grep -E "^(test result:|running|     Running)" "$$doc_log" || true; \
		else \
			doc_log_content="$$(cat "$$doc_log" 2>/dev/null || true)"; \
			echo "  ✗ Doc tests failed!"; echo ""; echo "=== Full test output ==="; echo "$$doc_log_content"; \
			exit_code=1; \
		fi; \
	fi; \
	if [ "$$exit_code" -eq 0 ]; then \
		echo "  ✓ Tests passed"; \
	fi; \
	exit "$$exit_code"

# Required every time (deduplicated via release-build stamp)
build: $(RALPH_RELEASE_BUILD_STAMP)
	@true

# Use the already-built release binary (no cargo run, no debug compile)
generate: $(RALPH_RELEASE_BUILD_STAMP)
	@echo "→ Generating schemas (via release binary)..."
	@mkdir -p schemas
	@./target/release/$(BIN_NAME) config schema > schemas/config.schema.json
	@./target/release/$(BIN_NAME) queue schema > schemas/queue.schema.json
	@./target/release/$(BIN_NAME) machine schema > schemas/machine.schema.json
	@echo "  ✓ Schemas generated"

docs:
	@echo "→ Generating rustdocs..."
	@$(RALPH_ENV_RESET); cargo doc --workspace --all-features --no-deps --locked $(CARGO_JOBS_FLAG)
	@echo "  ✓ Rustdocs generated in target/doc"

changelog:
	@scripts/generate-changelog.sh

changelog-preview:
	@scripts/generate-changelog.sh --dry-run

changelog-check:
	@scripts/generate-changelog.sh --check

version-check:
	@./scripts/versioning.sh check

version-sync:
	@if [ -n "$(VERSION)" ]; then \
		./scripts/versioning.sh sync --version "$(VERSION)"; \
	else \
		./scripts/versioning.sh sync; \
	fi

publish-check:
	@echo "→ Validating crates.io package ($(CARGO_PACKAGE_NAME))..."
	@$(RALPH_ENV_RESET); cargo package --list -p $(CARGO_PACKAGE_NAME) --allow-dirty
	@$(RALPH_ENV_RESET); cargo publish --dry-run -p $(CARGO_PACKAGE_NAME) --locked --allow-dirty
	@echo "  ✓ crates.io package dry-run passed"

release:
	@if [ -z "$(VERSION)" ]; then \
		echo "Usage: make release VERSION=x.y.z"; \
		exit 2; \
	fi
	@scripts/release.sh execute "$(VERSION)"

release-dry-run:
	@if [ -z "$(VERSION)" ]; then \
		echo "Usage: make release-dry-run VERSION=x.y.z"; \
		exit 2; \
	fi
	@scripts/release.sh verify "$(VERSION)"

release-verify:
	@if [ -z "$(VERSION)" ]; then \
		echo "Usage: make release-verify VERSION=x.y.z"; \
		exit 2; \
	fi
	@scripts/release.sh verify "$(VERSION)"
	@echo "  ✓ Release snapshot prepared for $(VERSION)"
	@echo "  ✓ Safe to run: make release VERSION=$(VERSION)"

release-artifacts:
	@if [ -n "$(VERSION)" ]; then \
		scripts/build-release-artifacts.sh "$(VERSION)"; \
	else \
		scripts/build-release-artifacts.sh; \
	fi

pre-public-check:
	@scripts/pre-public-check.sh

clean: clean-temp
	@cargo clean
	@find . -name '*.log' -type f -delete
	@rm -rf .ralph/lock .ralph/logs
	@if [ -d .ralph/cache ]; then \
		find .ralph/cache -mindepth 1 -maxdepth 1 ! -name completions -exec rm -rf {} +; \
	fi

clean-temp:
	@rm -rf target/tmp

check-env-safety:
	@scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean

check-backup-artifacts:
	@bak_files="$$(find crates/ralph/src/ -name '*.bak' -type f 2>/dev/null || true)"; \
	if [ -n "$$bak_files" ]; then \
		echo "ERROR: Backup artifacts found in crates/ralph/src/:"; \
		echo "$$bak_files"; \
		echo "Remove these files before committing."; \
		exit 1; \
	fi

check-repo-safety: check-env-safety
	@true

pre-commit: check-env-safety check-backup-artifacts format-check
	@echo "→ Pre-commit checks complete"
	@echo "  ✓ Pre-commit checks passed"

# Docs/community-only safety gate when no executable surface changed.
ci-docs: check-env-safety check-backup-artifacts
	@echo "→ Docs-only CI gate (no executable surface changed)..."
	@echo ""
	@echo "  ✓ Docs-only CI completed"

# Fast deterministic Rust/CLI gate for routine development and PR-equivalent checks.
ci-fast: check-env-safety check-backup-artifacts deps format type-check lint test
	@echo "→ Fast CI gate (format/type/lint/test)..."
	@echo ""
	@echo "  ✓ Fast CI completed"

# Full Rust release gate (includes release build/schema generation/install checks).
ci: ci-fast build generate install
	@echo "→ Full CI gate (ci-fast + release build/generate/install)..."
	@echo ""
	@echo "  ✓ CI completed"

release-gate:
	@if [ "$$(uname -s)" = "Darwin" ] && command -v xcodebuild >/dev/null 2>&1; then \
		echo "  → Running macOS release gate"; \
		$(MAKE) --no-print-directory macos-ci; \
	else \
		echo "  → Running Rust release gate"; \
		$(MAKE) --no-print-directory ci; \
	fi

profile-ship-gate: macos-preflight
	@timestamp="$$(date +%Y%m%d-%H%M%S)"; \
	profile_dir="target/profiling/$$timestamp-ship-gate"; \
	timings_path="$$profile_dir/timings.tsv"; \
	summary_path="$$profile_dir/summary.md"; \
	mkdir -p "$$profile_dir"; \
	printf 'label\tseconds\tstatus\n' > "$$timings_path"; \
	run_timed_shell() { \
		label="$$1"; \
		command="$$2"; \
		start="$$(date +%s)"; \
		set +e; \
		bash -c "$$command"; \
		status="$$?"; \
		set -e; \
		end="$$(date +%s)"; \
		duration="$$((end - start))"; \
		printf '%s\t%s\t%s\n' "$$label" "$$duration" "$$status" >> "$$timings_path"; \
		return "$$status"; \
	}; \
	write_summary() { \
		{ \
			echo '# Ship-gate profiling baseline'; \
			echo; \
			echo "- date: $$(date -u +%Y-%m-%dT%H:%M:%SZ)"; \
			echo "- profile_dir: $$profile_dir"; \
			echo '- retention: timestamped bundles are retained until explicit cleanup'; \
			echo '- cleanup: make profile-ship-gate-clean'; \
			echo; \
			echo '## Environment'; \
			echo; \
			echo "- uname: $$(uname -a)"; \
			echo "- xcodebuild: $$(xcodebuild -version | tr '\n' ' ' | sed 's/  */ /g')"; \
			echo "- RALPH_CI_JOBS: $(RALPH_CI_JOBS)"; \
			echo "- RALPH_XCODE_JOBS: $(RALPH_XCODE_JOBS)"; \
			echo; \
			echo '## Timings'; \
			echo; \
			awk 'NR == 1 { next } { printf "- %s: %ss (exit %s)\n", $$1, $$2, $$3 }' "$$timings_path"; \
			echo; \
			echo '## Slowest surfaces'; \
			echo; \
			tail -n +2 "$$timings_path" | sort -k2,2nr | head -3 | awk '{ printf "- %s: %ss\n", $$1, $$2 }'; \
		} > "$$summary_path"; \
	}; \
	echo "→ Capturing ship-gate profiling bundle under $$profile_dir..."; \
	run_timed_shell ci "$(MAKE) --no-print-directory ci" || { write_summary; exit 1; }; \
	run_timed_shell nextest_run_parallel_test "$(RALPH_ENV_RESET); NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run --workspace --locked --test run_parallel_test --show-progress none --status-level none --final-status-level none --message-format libtest-json-plus > '$$profile_dir/nextest.run_parallel_test.jsonl'" || { write_summary; exit 1; }; \
	run_timed_shell nextest_parallel_direct_push_test "$(RALPH_ENV_RESET); NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run --workspace --locked --test parallel_direct_push_test --show-progress none --status-level none --final-status-level none --message-format libtest-json-plus > '$$profile_dir/nextest.parallel_direct_push_test.jsonl'" || { write_summary; exit 1; }; \
	run_timed_shell macos_build "$(MAKE) --no-print-directory macos-build" || { write_summary; exit 1; }; \
	run_timed_shell macos_test "$(MAKE) --no-print-directory macos-test" || { write_summary; exit 1; }; \
	run_timed_shell macos_test_contracts "$(MAKE) --no-print-directory macos-test-contracts" || { write_summary; exit 1; }; \
	write_summary; \
	echo "  ✓ Profiling bundle: $$profile_dir"; \
	echo "  ✓ Summary: $$summary_path"; \
	echo "  ℹ Retained until: make profile-ship-gate-clean"

profile-ship-gate-clean:
	@echo "→ Removing ship-gate profiling bundles..."
	@rm -rf target/profiling
	@echo "  ✓ Ship-gate profiling bundles removed"

# Agent CI compromise: route to the smallest valid gate and escalate only when the dependency surface demands it.
# Set RALPH_AGENT_CI_FORCE_MACOS=1 to force the macOS app gate.
agent-ci:
	@echo "→ Agent CI gate (dependency-surface routing between docs, Rust, and macOS ship gates)..."
	@force_macos="$${RALPH_AGENT_CI_FORCE_MACOS:-0}"; \
	if [ "$$force_macos" = "1" ]; then \
		echo "  → RALPH_AGENT_CI_FORCE_MACOS=1; running macOS gate"; \
		$(MAKE) --no-print-directory macos-ci; \
		exit 0; \
	fi; \
	if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then \
		echo "  → Not in a git worktree; running macOS gate for safety"; \
		$(MAKE) --no-print-directory macos-ci; \
		exit 0; \
	fi; \
	target_name="$$(scripts/agent-ci-surface.sh --target)"; \
	target_reason="$$(scripts/agent-ci-surface.sh --reason)"; \
	echo "  → $$target_reason"; \
	$(MAKE) --no-print-directory "$$target_name"

macos-preflight:
	@os="$$(uname -s)"; \
	if [ "$$os" != "Darwin" ]; then \
		echo "macos-preflight: macOS-only (uname: $$os)"; \
		exit 1; \
	fi; \
	if ! command -v xcodebuild >/dev/null 2>&1; then \
		echo "macos-preflight: xcodebuild not found on PATH"; \
		exit 1; \
	fi

macos-build: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	cleanup() { if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; }; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-build"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/build"; \
	echo "→ macOS build (Xcode build)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Release \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		build

macos-install-app: macos-build
	@derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/build"; \
	app_bundle="$$derived_data_path/Build/Products/Release/RalphMac.app"; \
	install_dir="$(MACOS_APP_INSTALL_DIR)"; \
	if [ ! -w "$$install_dir" ]; then \
		install_dir="$(HOME)/Applications"; \
		echo "macos-install-app: $(MACOS_APP_INSTALL_DIR) not writable; using $$install_dir"; \
	fi; \
	mkdir -p "$$install_dir"; \
	dest_bundle="$$install_dir/RalphMac.app"; \
	echo "→ Installing RalphMac.app to $$dest_bundle"; \
	rm -rf "$$dest_bundle"; \
	ditto "$$app_bundle" "$$dest_bundle"; \
	/System/Library/Frameworks/CoreServices.framework/Versions/Current/Frameworks/LaunchServices.framework/Versions/Current/Support/lsregister -f "$$dest_bundle" >/dev/null 2>&1 || true; \
	echo "  ✓ RalphMac.app installed"

macos-test: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@include_ui_tests="$(RALPH_UI_TESTS)"; \
	result_bundle_path="$(XCODE_RESULT_BUNDLE_PATH)"; \
	if [ "$$include_ui_tests" = "1" ]; then \
		echo "→ macOS tests (Xcode, including UI tests - will take over mouse/keyboard)..."; \
		$(MAKE) --no-print-directory macos-ui-build-for-testing; \
		$(MAKE) --no-print-directory macos-ui-retest \
			RALPH_UI_SCREENSHOTS="$(RALPH_UI_SCREENSHOTS)" \
			RALPH_UI_SCREENSHOT_MODE="$(RALPH_UI_SCREENSHOT_MODE)" \
			XCODE_RESULT_BUNDLE_PATH="$$result_bundle_path"; \
	else \
		lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
		source scripts/lib/xcodebuild-lock.sh; \
		acquired=0; \
		cleanup() { if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; }; \
		trap cleanup EXIT INT TERM; \
		derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/test"; \
		ralph_acquire_xcode_build_lock "$$lock_dir" "macos-test"; \
		acquired=1; \
		echo "→ macOS tests (Xcode, skipping UI tests - use RALPH_UI_TESTS=1 to include)..."; \
		skipped_tests="-skip-testing RalphMacUITests"; \
		rm -rf "$$derived_data_path" 2>/dev/null || true; \
		xcodebuild \
			-project apps/RalphMac/RalphMac.xcodeproj \
			-scheme RalphMac \
			-configuration Debug \
			-destination '$(XCODE_DESTINATION)' \
			-derivedDataPath "$$derived_data_path" \
			$(XCODE_JOBS_FLAG) \
			CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
			SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
			$$skipped_tests \
			test; \
	fi; \
	true

# Build/sign macOS UI test bundles once for local iteration.
# Use macos-ui-retest repeatedly afterward to avoid fresh bundle preparation.
macos-ui-build-for-testing: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	cleanup() { if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; }; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-ui-build-for-testing"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui"; \
	echo "→ macOS UI build-for-testing (one-time prompt may appear for a rebuilt bundle)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		build-for-testing; \
	echo "→ Clearing quarantine metadata on UI test bundles..."; \
	xattr -dr com.apple.quarantine "$$derived_data_path/Build/Products/Debug/RalphMac.app" "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app" 2>/dev/null || true; \
	echo "→ Re-signing UI test bundles (ad-hoc) to avoid Gatekeeper runner failures..."; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMac.app"; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app"; \
	echo "  ✓ Prepared UI runner under $$derived_data_path"

# Re-run macOS UI tests without rebuilding the app/runner bundles.
# Optional: set RALPH_UI_ONLY_TESTING=<Target/Class/testMethod> to focus a single test.
macos-ui-retest:
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui"; \
	app_binary="$$derived_data_path/Build/Products/Debug/RalphMac.app/Contents/MacOS/RalphMac"; \
	runner_binary="$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app/Contents/MacOS/RalphMacUITests-Runner"; \
	cleanup() { \
		if pgrep -f "$$runner_binary" >/dev/null 2>&1; then pkill -TERM -f "$$runner_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$runner_binary" >/dev/null 2>&1 && pkill -KILL -f "$$runner_binary" >/dev/null 2>&1 || true; fi; \
		if pgrep -f "$$app_binary" >/dev/null 2>&1; then pkill -TERM -f "$$app_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$app_binary" >/dev/null 2>&1 && pkill -KILL -f "$$app_binary" >/dev/null 2>&1 || true; fi; \
		if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; \
	}; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-ui-retest"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui"; \
	result_bundle_path="$(XCODE_RESULT_BUNDLE_PATH)"; \
	only_testing="$(RALPH_UI_ONLY_TESTING)"; \
	app_bundle="$$derived_data_path/Build/Products/Debug/RalphMac.app"; \
	runner_bundle="$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app"; \
	if [ ! -d "$$app_bundle" ] || [ ! -d "$$runner_bundle" ]; then \
		echo "ERROR: UI test bundles are not prepared. Run 'make macos-ui-build-for-testing' first." >&2; \
		exit 2; \
	fi; \
	result_bundle_args=(); \
	if [ -n "$$result_bundle_path" ]; then \
		mkdir -p "$$(dirname "$$result_bundle_path")"; \
		result_bundle_args=(-resultBundlePath "$$result_bundle_path"); \
	fi; \
	test_scope_args=(); \
	if [ -n "$$only_testing" ]; then \
		test_scope_args=(-only-testing:"$$only_testing"); \
		echo "→ macOS UI retest (targeted: $$only_testing)..."; \
	else \
		echo "→ macOS UI retest (reusing prepared bundles; no rebuild)..."; \
	fi; \
	RALPH_UI_SCREENSHOTS="$(RALPH_UI_SCREENSHOTS)" \
	RALPH_UI_SCREENSHOT_MODE="$(RALPH_UI_SCREENSHOT_MODE)" \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		"$${result_bundle_args[@]}" \
		"$${test_scope_args[@]}" \
		test-without-building; \
	if pgrep -f "$$runner_binary" >/dev/null 2>&1 || pgrep -f "$$app_binary" >/dev/null 2>&1; then \
		echo "ERROR: macos-ui-retest left a lingering UI test app or runner process" >&2; \
		ps -axo pid=,command= | grep -E "$$runner_binary|$$app_binary" | grep -v grep >&2 || true; \
		exit 1; \
	fi

# Run macOS UI tests (interactive - will take over mouse/keyboard)
macos-test-ui:
	@$(MAKE) --no-print-directory macos-ui-build-for-testing
	@$(MAKE) --no-print-directory macos-ui-retest

# Run macOS UI tests with preserved xcresult output (interactive).
# Stores timestamped artifacts under $(RALPH_UI_ARTIFACTS_ROOT)/<timestamp>/.
macos-test-ui-artifacts: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@timestamp="$$(date +%Y%m%d-%H%M%S)"; \
	artifact_dir="$(RALPH_UI_ARTIFACTS_ROOT)/$$timestamp"; \
	result_bundle_path="$$artifact_dir/RalphMacUITests.xcresult"; \
	summary_path="$$artifact_dir/summary.txt"; \
	mkdir -p "$$artifact_dir"; \
	echo "→ macOS UI tests with xcresult capture..."; \
	set +e; \
	$(MAKE) --no-print-directory macos-ui-build-for-testing; \
	$(MAKE) --no-print-directory macos-ui-retest \
		XCODE_RESULT_BUNDLE_PATH="$$result_bundle_path"; \
	test_exit="$$?"; \
	set -e; \
	final_exit="$$test_exit"; \
	if [ -d "$$result_bundle_path" ]; then \
		{ \
			echo "Ralph macOS UI artifact summary"; \
			echo "timestamp: $$timestamp"; \
			echo "result_bundle: $$result_bundle_path"; \
			echo "targeted_test: $${RALPH_UI_ONLY_TESTING:-all}"; \
		} > "$$summary_path"; \
		echo "  ✓ Result bundle: $$result_bundle_path"; \
		echo "  ✓ Summary: $$summary_path"; \
	else \
		echo "  ⚠ No xcresult bundle found at $$result_bundle_path"; \
		if [ "$$test_exit" = "0" ]; then final_exit=1; fi; \
	fi; \
	echo "  ℹ Cleanup after review: make macos-ui-artifacts-clean"; \
	exit "$$final_exit"

# Remove captured UI visual artifacts after review.
macos-ui-artifacts-clean:
	@echo "→ Removing captured UI visual artifacts..."
	@rm -rf "$(RALPH_UI_ARTIFACTS_ROOT)"
	@echo "  ✓ UI visual artifacts removed"

# Run deterministic non-XCTest macOS contract checks against the built app.
macos-test-contracts: macos-test-settings-smoke macos-test-workspace-routing-contract
	@echo "→ macOS deterministic contract checks completed"

# Run targeted noninteractive Settings contract coverage for supported entry paths.
macos-test-settings-smoke: macos-build
	@echo "→ macOS Settings smoke contract coverage (keyboard, app menu, URL route; noninteractive)..."
	@./scripts/macos-settings-smoke.sh --app-bundle "$(XCODE_DERIVED_DATA_ROOT)/build/Build/Products/Release/RalphMac.app"

# Run targeted noninteractive workspace bootstrap/routing contract coverage.
macos-test-workspace-routing-contract: macos-build
	@echo "→ macOS workspace routing contract coverage (bootstrap, URL open, pending scene routes; noninteractive)..."
	@./scripts/macos-workspace-routing-contract.sh --app-bundle "$(XCODE_DERIVED_DATA_ROOT)/build/Build/Products/Release/RalphMac.app"

macos-test-window-shortcuts: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui-shortcuts"; \
	app_binary="$$derived_data_path/Build/Products/Debug/RalphMac.app/Contents/MacOS/RalphMac"; \
	runner_binary="$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app/Contents/MacOS/RalphMacUITests-Runner"; \
	cleanup() { \
		if pgrep -f "$$runner_binary" >/dev/null 2>&1; then pkill -TERM -f "$$runner_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$runner_binary" >/dev/null 2>&1 && pkill -KILL -f "$$runner_binary" >/dev/null 2>&1 || true; fi; \
		if pgrep -f "$$app_binary" >/dev/null 2>&1; then pkill -TERM -f "$$app_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$app_binary" >/dev/null 2>&1 && pkill -KILL -f "$$app_binary" >/dev/null 2>&1 || true; fi; \
		if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; \
	}; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-test-window-shortcuts"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui-shortcuts"; \
	echo "→ macOS UI shortcut regressions (focused window/tab behavior)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_windowShortcuts_affectOnlyFocusedWindow \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_commandPaletteNewTab_affectsOnlyFocusedWindow \
		build-for-testing; \
	echo "→ Clearing quarantine metadata on UI test bundles..."; \
	xattr -dr com.apple.quarantine "$$derived_data_path/Build/Products/Debug/RalphMac.app" "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app" 2>/dev/null || true; \
	echo "→ Re-signing UI test bundles (ad-hoc) to avoid Gatekeeper runner failures..."; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMac.app"; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app"; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_windowShortcuts_affectOnlyFocusedWindow \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_commandPaletteNewTab_affectsOnlyFocusedWindow \
		test-without-building; \
	if pgrep -f "$$runner_binary" >/dev/null 2>&1 || pgrep -f "$$app_binary" >/dev/null 2>&1; then \
		echo "ERROR: macos-test-window-shortcuts left a lingering UI test app or runner process" >&2; \
		ps -axo pid=,command= | grep -E "$$runner_binary|$$app_binary" | grep -v grep >&2 || true; \
		exit 1; \
	fi

macos-ci: macos-preflight ci macos-build macos-test macos-test-contracts
	@echo "→ macOS ship gate (Rust CI + macOS app build+test + deterministic contract smoke)..."
	@echo "  ℹ Interactive XCTest UI automation remains excluded from macos-ci (use make macos-test-ui or make macos-test-window-shortcuts when idle)."
	@echo "  ✓ macOS CI completed"

# Coverage output directory
COVERAGE_DIR ?= target/coverage

# Coverage: Generate HTML and summary reports (requires cargo-llvm-cov)
# Generates: HTML report, text summary with per-crate breakdown, and JSON data
coverage:
	@echo "→ Running coverage analysis..."
	@if ! cargo llvm-cov --version >/dev/null 2>&1; then \
		echo "ERROR: cargo-llvm-cov not found."; \
		echo ""; \
		echo "Install with:"; \
		echo "  cargo install cargo-llvm-cov"; \
		echo ""; \
		echo "On macOS, you may also need:"; \
		echo "  rustup component add llvm-tools-preview"; \
		exit 1; \
	fi
	@mkdir -p $(COVERAGE_DIR)
	@echo "  → Running tests with coverage instrumentation..."
	@cargo llvm-cov --workspace --all-targets --all-features --locked \
		--html --output-dir $(COVERAGE_DIR)/html \
		--json --output-path $(COVERAGE_DIR)/coverage.json
	@echo ""
	@echo "  ✓ Coverage report generated:"
	@echo "    HTML:  $(COVERAGE_DIR)/html/index.html"
	@echo "    JSON:  $(COVERAGE_DIR)/coverage.json"
	@echo ""
	@echo "  → Coverage summary:"
	@echo ""
	@echo "    Total Coverage:"
	@jq -r '[.data[0].totals.lines.percent // 0, .data[0].totals.functions.percent // 0, .data[0].totals.regions.percent // 0] | "      Lines: \(.[0])%, Functions: \(.[1])%, Regions: \(.[2])%"' $(COVERAGE_DIR)/coverage.json 2>/dev/null || echo "      (install jq for formatted output)"
	@echo ""
	@echo "    Per-Crate Breakdown:"
	@jq -r '.data[0].summaries // [] | sort_by(.crate_name) | .[] | "      \(.crate_name): Lines \(.summary.lines.percent // 0)%, Functions \(.summary.functions.percent // 0)%"' $(COVERAGE_DIR)/coverage.json 2>/dev/null || echo "      (see $(COVERAGE_DIR)/coverage.json for raw data)"
	@echo ""
	@echo "  → Opening HTML report..."
	@open $(COVERAGE_DIR)/html/index.html 2>/dev/null || echo "    (open $(COVERAGE_DIR)/html/index.html manually)"

# Coverage clean: Remove coverage artifacts
coverage-clean:
	@echo "→ Cleaning coverage artifacts..."
	@rm -rf $(COVERAGE_DIR)
	@find . -name '*.profraw' -type f -delete 2>/dev/null || true
	@find . -name '*.profdata' -type f -delete 2>/dev/null || true
	@echo "  ✓ Coverage artifacts removed"
