RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph
CARGO_HTTP_MULTIPLEXING ?= false
XCODE_DERIVED_DATA_ROOT ?= target/tmp/xcode-deriveddata
# Pin destination arch to avoid xcodebuild's "first of multiple matching destinations" warning.
# Override if you intentionally want a different destination.
XCODE_DESTINATION ?= platform=macOS,arch=$(shell uname -m)
# UI tests: Set to 1 to include UI tests (headed, mouse-interactive), 0 to skip (default for CI)
RALPH_UI_TESTS ?= 0
# Command prefix placeholder for consistency across targets.
RALPH_ENV_RESET := :

.DELETE_ON_ERROR:
.ONESHELL:
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c

# Require GNU Make >= 4.x (Homebrew `make` provides `gmake`, plus a `make` shim under `.../gnubin`).
ifeq ($(filter 4.% 5.%,$(MAKE_VERSION)),)
$(error GNU Make >= 4 is required (found: $(MAKE_VERSION)). On macOS: `brew install make` then run `gmake <target>` or add `$(HOME)/.zshrc`: export PATH="/opt/homebrew/opt/make/libexec/gnubin:$$PATH")
endif

MAKEFLAGS += --warn-undefined-variables
MAKEFLAGS += --no-builtin-rules

.PHONY: help install update lint lint-fix format type-check clean clean-temp test generate build ci deps \
	agent-ci check-env-safety check-backup-artifacts macos-preflight macos-build macos-test macos-ci macos-test-ui \
	macos-test-window-shortcuts coverage coverage-clean

help:
	@echo "Common targets:"
	@echo "  make ci          # Rust-only local CI gate (formats code, builds+installs release)"
	@echo "  make agent-ci    # Agent gate: Rust/CLI always; macOS app gate only on apps/RalphMac changes"
	@echo "  make macos-ci     # Rust gate + macOS app build+test (requires Xcode)"
	@echo "  make test         # Nextest workspace tests + cargo doc tests (auto-fallback if nextest missing)"
	@echo "  make coverage     # Generate code coverage report (requires cargo-llvm-cov)"
	@echo "  make coverage-clean  # Remove coverage artifacts"
	@echo "  make macos-test-window-shortcuts # Run focused multi-window shortcut UI regressions"
	@echo "  make lint         # Clippy with -D warnings"
	@echo "  make generate     # Regenerate committed JSON schemas via release binary"
	@echo "  make install      # Install release binary to BIN_DIR"

# Optional but cheap: fail fast if lockfile or network access is busted
deps:
	@echo "→ Fetching deps (locked)..."
	@$(RALPH_ENV_RESET); cargo fetch --locked
	@echo "  ✓ Deps fetched"

install: build
	@bin_dir="$(BIN_DIR)"; \
	if [ ! -w "$$bin_dir" ]; then \
		bin_dir="$(HOME)/.local/bin"; \
		echo "install: $(BIN_DIR) not writable; using $$bin_dir"; \
	fi; \
	mkdir -p "$$bin_dir"; \
	install -m 0755 target/release/$(BIN_NAME) "$$bin_dir/$(BIN_NAME)"; \
	"$$bin_dir/$(BIN_NAME)" --help >/dev/null

update:
	@CARGO_HTTP_MULTIPLEXING=$(CARGO_HTTP_MULTIPLEXING) cargo update

format:
	@echo "→ Formatting code..."
	@$(RALPH_ENV_RESET); cargo fmt --all
	@echo "  ✓ Formatting complete"

type-check:
	@echo "→ Type-checking..."
	@$(RALPH_ENV_RESET); cargo check --workspace --all-targets --all-features --locked
	@echo "  ✓ Type-checking complete"

lint:
	@echo "→ Linting (clippy, non-mutating)..."
	@$(RALPH_ENV_RESET); cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
	@echo "  ✓ Linting complete"

lint-fix:
	@echo "→ Clippy autofix (optional)..."
	@$(RALPH_ENV_RESET); cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --locked -- -D warnings
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
	if cargo nextest --version >/dev/null 2>&1; then \
		echo "  → Using cargo-nextest for non-doc tests"; \
		if cargo nextest run --workspace --all-targets --locked -- --include-ignored >"$$unit_log" 2>&1; then \
			grep -E "^(test result:|running|     Running|Summary|PASS|FAIL)" "$$unit_log" | tail -5 || true; \
		else \
			echo "  ✗ Workspace tests failed!"; echo ""; echo "=== Full test output ==="; cat "$$unit_log"; exit 1; \
		fi; \
	else \
		echo "  ⚠ cargo-nextest not found; falling back to cargo test --workspace --all-targets"; \
		echo "    Install with: cargo install cargo-nextest --locked"; \
		if cargo test --workspace --all-targets --locked -- --include-ignored >"$$unit_log" 2>&1; then \
			grep -E "^(test result:|running|     Running)" "$$unit_log" || true; \
		else \
			echo "  ✗ Workspace tests failed!"; echo ""; echo "=== Full test output ==="; cat "$$unit_log"; exit 1; \
		fi; \
	fi; \
	if cargo test --workspace --doc --locked -- --include-ignored >"$$doc_log" 2>&1; then \
		grep -E "^(test result:|running|     Running)" "$$doc_log" || true; \
	else \
		echo "  ✗ Doc tests failed!"; echo ""; echo "=== Full test output ==="; cat "$$doc_log"; exit 1; \
	fi; \
	echo "  ✓ Tests passed"

# Required every time
build:
	@echo "→ Release build..."
	@$(RALPH_ENV_RESET); cargo build --workspace --release --locked
	@echo "  ✓ Release build complete"

# Use the already-built release binary (no cargo run, no debug compile)
generate: build
	@echo "→ Generating schemas (via release binary)..."
	@mkdir -p schemas
	@./target/release/$(BIN_NAME) config schema > schemas/config.schema.json
	@./target/release/$(BIN_NAME) queue schema > schemas/queue.schema.json
	@echo "  ✓ Schemas generated"

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
	@if git ls-files .env | grep -q .env; then \
		echo "ERROR: .env is tracked in git. Remove it with '\''git rm --cached .env'\'' and ensure .env is in .gitignore."; \
		exit 1; \
	fi

check-backup-artifacts:
	@bak_files="$$(find crates/ralph/src/ -name '*.bak' -type f 2>/dev/null || true)"; \
	if [ -n "$$bak_files" ]; then \
		echo "ERROR: Backup artifacts found in crates/ralph/src/:"; \
		echo "$$bak_files"; \
		echo "Remove these files before committing."; \
		exit 1; \
	fi

# Speed-first local CI that always builds release + installs
ci: check-env-safety check-backup-artifacts deps format type-check lint test build generate install
	@echo "→ Local CI (formats code, always builds+installs release)..."
	@echo ""
	@echo "  ✓ CI completed"

# Agent CI compromise: always run Rust/CLI gate; run macOS app gate only when app paths change.
# Set RALPH_AGENT_CI_FORCE_MACOS=1 to force the macOS app gate.
agent-ci:
	@echo "→ Agent CI gate (Rust/CLI always; macOS app gate on app changes)..."
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
	changed_paths="$$( \
		{ \
			git diff --name-only --relative; \
			git diff --cached --name-only --relative; \
			git ls-files --others --exclude-standard; \
		} | sed '/^$$/d' | sort -u \
	)"; \
	if printf '%s\n' "$$changed_paths" | grep -qE '^apps/RalphMac/'; then \
		echo "  → app changes detected under apps/RalphMac/; running macOS gate"; \
		$(MAKE) --no-print-directory macos-ci; \
	else \
		echo "  → no app changes detected; running Rust/CLI gate"; \
		$(MAKE) --no-print-directory ci; \
	fi

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

macos-build:
	@$(MAKE) --no-print-directory macos-preflight
	@$(MAKE) --no-print-directory build
	@derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/build"; \
	echo "→ macOS build (Xcode build)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Release \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		build

macos-test:
	@$(MAKE) --no-print-directory macos-preflight
	@$(MAKE) --no-print-directory build
	@derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/test"; \
	ralph_bin_path="$$(pwd)/target/release/ralph"; \
	include_ui_tests="$(RALPH_UI_TESTS)"; \
	if [ "$$include_ui_tests" = "1" ]; then \
		echo "→ macOS tests (Xcode, including UI tests - will take over mouse/keyboard)..."; \
		skipped_tests=""; \
	else \
		echo "→ macOS tests (Xcode, skipping UI tests - use RALPH_UI_TESTS=1 to include)..."; \
		skipped_tests="-skip-testing RalphMacUITests"; \
	fi; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	RALPH_BIN_PATH="$$ralph_bin_path" xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		$$skipped_tests \
		test

# Run macOS UI tests (interactive - will take over mouse/keyboard)
macos-test-ui:
	@$(MAKE) --no-print-directory macos-test RALPH_UI_TESTS=1

# Run targeted UI regressions for window/tab shortcut scoping.
macos-test-window-shortcuts:
	@$(MAKE) --no-print-directory macos-preflight
	@$(MAKE) --no-print-directory build
	@derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui-shortcuts"; \
	ralph_bin_path="$$(pwd)/target/release/ralph"; \
	echo "→ macOS UI shortcut regressions (focused window/tab behavior)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	RALPH_BIN_PATH="$$ralph_bin_path" xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		-only-testing:RalphMacUITests/RalphMacUITests/test_windowShortcuts_affectOnlyFocusedWindow \
		-only-testing:RalphMacUITests/RalphMacUITests/test_commandPaletteNewTab_affectsOnlyFocusedWindow \
		test

macos-ci: macos-preflight ci macos-build macos-test
	@echo "→ macOS ship gate (Rust CI + macOS app build+test)..."
	@echo "  ℹ UI automation is intentionally excluded from macos-ci (use make macos-test-ui or make macos-test-window-shortcuts when idle)."
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
