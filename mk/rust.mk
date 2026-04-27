# Purpose: Define Ralph Rust, release, and general developer targets included by the root Makefile.
# Responsibilities: Own Rust dependency, formatting, linting, testing, build, schema generation, release, install, cleanup, and public-readiness recipes.
# Scope: Target bodies only; global variables, GNU Make settings, public help text, and phony aggregation stay in ../Makefile.
# Usage: Included by ../Makefile; invoke targets through the root Makefile rather than this fragment directly.
# Invariants/Assumptions: The including Makefile defines RALPH_ENV_RESET, release stamp variables, toolchain variables, shell flags, and shared resource knobs first.

$(RALPH_RELEASE_BUILD_STAMP): $(RALPH_RELEASE_STAMP_INPUTS) $(RALPH_CRATE_SOURCE_FILES)
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

install-verify: $(RALPH_RELEASE_BUILD_STAMP)
	@ralph_bin_path="$(CURDIR)/target/release/$(BIN_NAME)"; \
	if [ ! -x "$$ralph_bin_path" ]; then \
		echo "install-verify: missing release binary at $$ralph_bin_path (run make build first)" >&2; \
		exit 1; \
	fi; \
	bin_dir="$(BIN_DIR)"; \
	if [ ! -w "$$bin_dir" ]; then \
		bin_dir="$(HOME)/.local/bin"; \
		echo "install-verify: $(BIN_DIR) not writable; using $$bin_dir"; \
	fi; \
	mkdir -p "$$bin_dir"; \
	install -m 0755 "$$ralph_bin_path" "$$bin_dir/$(BIN_NAME)"; \
	"$$bin_dir/$(BIN_NAME)" --help >/dev/null

install: install-verify
	@if [ "$$(uname -s)" = "Darwin" ] && command -v xcodebuild >/dev/null 2>&1; then \
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
