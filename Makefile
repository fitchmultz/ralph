RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph
CARGO_HTTP_MULTIPLEXING ?= false

.PHONY: install update lint format type-check clean clean-temp test generate build ci deps \
	check-env-safety check-backup-artifacts macos-build macos-test macos-ci

# Optional but cheap: fail fast if lockfile or network access is busted
deps:
	@echo "→ Fetching deps (locked)..."
	@cargo fetch --locked
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
	@cargo fmt --all
	@echo "  ✓ Formatting complete"

type-check:
	@echo "→ Type-checking..."
	@cargo check --workspace --all-targets
	@echo "  ✓ Type-checking complete"

lint:
	@echo "→ Clippy autofix (phase 1/2)..."
	@cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --locked -- -D warnings
	@echo "  ✓ Linting complete"

test:
	@echo "→ Running tests..."
	@bash -lc 'set -euo pipefail; \
		repo_root="$$(pwd -P)"; \
		system_tmp="$${TMPDIR:-/tmp}"; \
		system_tmp="$${system_tmp%/}"; \
		legacy_tmp_base="$$system_tmp/ralph-ci-tmp"; \
		if [ "$${RALPH_CI_KEEP_TMP:-0}" != "1" ]; then rm -rf "$$legacy_tmp_base" 2>/dev/null || true; fi; \
		tmp_base="$$repo_root/target/tmp/ralph-ci-tmp"; \
		if [ "$${RALPH_CI_KEEP_TMP:-0}" != "1" ]; then rm -rf "$$tmp_base" 2>/dev/null || true; fi; \
		mkdir -p "$$tmp_base"; \
		run_dir="$$(mktemp -d "$$tmp_base/ralph-ci.XXXXXX")"; \
		cleanup() { \
			if [ "$${RALPH_CI_KEEP_TMP:-0}" = "1" ]; then \
				echo "  ℹ Keeping CI temp dir: $$run_dir"; \
				return 0; \
			fi; \
			rm -rf "$$run_dir" 2>/dev/null || true; \
			rm -rf "$$tmp_base" 2>/dev/null || true; \
		}; \
		trap cleanup EXIT INT TERM; \
		export TMPDIR="$$run_dir"; \
		export TEMP="$$run_dir"; \
		export TMP="$$run_dir"; \
		unit_test_output=$$(cargo test --workspace --all-targets --locked -- --include-ignored 2>&1) || { \
			echo "  ✗ Unit tests failed!"; \
			echo ""; \
			echo "=== Full test output ==="; \
			echo "$$unit_test_output"; \
			exit 1; \
		}; \
		echo "$$unit_test_output" | grep -E "^(test result:|running|     Running)" || true; \
		doc_test_output=$$(cargo test --workspace --doc --locked -- --include-ignored 2>&1) || { \
			echo "  ✗ Doc tests failed!"; \
			echo ""; \
			echo "=== Full test output ==="; \
			echo "$$doc_test_output"; \
			exit 1; \
		}; \
		echo "$$doc_test_output" | grep -E "^(test result:|running|     Running)" || true; \
		echo "  ✓ Tests passed"'

# Required every time
build:
	@echo "→ Release build..."
	@cargo build --workspace --release --locked
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
	@bak_files=$$(find crates/ralph/src/ -name '*.bak' -type f 2>/dev/null); \
	if [ -n "$$bak_files" ]; then \
		echo "ERROR: Backup artifacts found in crates/ralph/src/:"; \
		echo "$$bak_files"; \
		echo "Remove these files before committing."; \
		exit 1; \
	fi

# Speed-first local CI that always builds release + installs
ci:
	@echo "→ Local CI (mutates code, always builds+installs release)..."
	@echo ""
	@set -e; \
	$(MAKE) check-env-safety        || { echo ""; echo "✗ CI failed at: check-env-safety"; exit 1; }; \
	$(MAKE) check-backup-artifacts  || { echo ""; echo "✗ CI failed at: check-backup-artifacts"; exit 1; }; \
	$(MAKE) deps                   || { echo ""; echo "✗ CI failed at: deps"; exit 1; }; \
	$(MAKE) format                 || { echo ""; echo "✗ CI failed at: format"; exit 1; }; \
	$(MAKE) type-check             || { echo ""; echo "✗ CI failed at: type-check"; exit 1; }; \
	$(MAKE) lint                   || { echo ""; echo "✗ CI failed at: lint"; exit 1; }; \
	$(MAKE) test                   || { echo ""; echo "✗ CI failed at: test"; exit 1; }; \
	$(MAKE) build                  || { echo ""; echo "✗ CI failed at: build"; exit 1; }; \
	$(MAKE) generate               || { echo ""; echo "✗ CI failed at: generate"; exit 1; }; \
	$(MAKE) install                || { echo ""; echo "✗ CI failed at: install"; exit 1; }
	@echo ""
	@echo "═══════════════════════════════════════════════════"
	@echo "  ✓ CI completed successfully"
	@echo "═══════════════════════════════════════════════════"

macos-build:
	@os="$$(uname -s)"; \
	if [ "$$os" != "Darwin" ]; then \
		echo "macos-build: macOS-only (uname: $$os)"; \
		exit 1; \
	fi; \
	echo "→ macOS build (Rust release + Xcode build)..."; \
	$(MAKE) build; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Release \
		-destination 'platform=macOS' \
		-derivedDataPath target/tmp/xcode-deriveddata \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		build

macos-test:
	@os="$$(uname -s)"; \
	if [ "$$os" != "Darwin" ]; then \
		echo "macos-test: macOS-only (uname: $$os)"; \
		exit 1; \
	fi; \
	echo "→ macOS tests (Xcode)..."; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination 'platform=macOS' \
		-derivedDataPath target/tmp/xcode-deriveddata \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		test

macos-ci:
	@os="$$(uname -s)"; \
	if [ "$$os" != "Darwin" ]; then \
		echo "macos-ci: macOS-only (uname: $$os)"; \
		exit 1; \
	fi; \
	echo "→ macOS ship gate (Rust CI + macOS app build+test)..."; \
	$(MAKE) ci; \
	$(MAKE) macos-build; \
	$(MAKE) macos-test
