RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph

.PHONY: install update lint type-check format clean clean-temp test generate build ci runners-help release release-dry-run release-artifacts release-artifacts-all release-legacy changelog changelog-preview changelog-check

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
	@cargo update

lint:
	@echo "→ Running linter..."
	@cargo clippy --fix --allow-dirty --workspace --all-targets -- -D warnings 
	@echo "  ✓ Linting passed"

type-check:
	@echo "→ Type-checking workspace..."
	@cargo check --workspace --all-targets
	@echo "  ✓ Type-check passed"

format:
	@echo "→ Formatting code..."
	@cargo fmt --all
	@echo "  ✓ Formatting complete"

clean: clean-temp
	cargo clean
	find . -name '*.log' -type f -delete
	rm -rf .ralph/lock .ralph/logs
	@if [ -d .ralph/cache ]; then \
		find .ralph/cache -mindepth 1 -maxdepth 1 ! -name completions -exec rm -rf {} +; \
	fi

clean-temp:
	rm -rf target/tmp

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
		unit_test_output=$$(cargo test --workspace --all-targets -- --include-ignored 2>&1) || { \
			echo "  ✗ Unit tests failed!"; \
			echo ""; \
			echo "=== Full test output ==="; \
			echo "$$unit_test_output"; \
			exit 1; \
		}; \
		echo "$$unit_test_output" | grep -E "^(test result:|running|     Running)" || true; \
		doc_test_output=$$(cargo test --workspace --doc -- --include-ignored 2>&1) || { \
			echo "  ✗ Doc tests failed!"; \
			echo ""; \
			echo "=== Full test output ==="; \
			echo "$$doc_test_output"; \
			exit 1; \
		}; \
		echo "$$doc_test_output" | grep -E "^(test result:|running|     Running)" || true; \
		echo "  ✓ Tests passed"'

generate:
	@echo "→ Generating schemas..."
	@mkdir -p schemas
	@cargo run -q --bin ralph -- config schema > schemas/config.schema.json
	@cargo run -q --bin ralph -- queue schema > schemas/queue.schema.json
	@echo "  ✓ Schemas generated"

build:
	@echo "→ Building workspace..."
	@cargo build --workspace --release
	@echo "  ✓ Build complete"

check-env-safety:
	@if git ls-files .env | grep -q .env; then \
		echo "ERROR: .env is tracked in git. Remove it with 'git rm --cached .env' and ensure .env is in .gitignore."; \
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

ci:
	@echo "→ Starting CI pipeline..."
	@echo ""
	@$(MAKE) check-env-safety || { echo ""; echo "✗ CI failed at: check-env-safety"; exit 1; }
	@$(MAKE) check-backup-artifacts || { echo ""; echo "✗ CI failed at: check-backup-artifacts"; exit 1; }
	@$(MAKE) build || { echo ""; echo "✗ CI failed at: build"; exit 1; }
	@$(MAKE) generate || { echo ""; echo "✗ CI failed at: generate"; exit 1; }
	@$(MAKE) format || { echo ""; echo "✗ CI failed at: format"; exit 1; }
	@$(MAKE) type-check || { echo ""; echo "✗ CI failed at: type-check"; exit 1; }
	@$(MAKE) lint || { echo ""; echo "✗ CI failed at: lint"; exit 1; }
	@$(MAKE) test || { echo ""; echo "✗ CI failed at: test"; exit 1; }
	@$(MAKE) install || { echo ""; echo "✗ CI failed at: install"; exit 1; }
	@echo ""
	@echo "═══════════════════════════════════════════════════"
	@echo "  ✓ CI completed successfully"
	@echo "═══════════════════════════════════════════════════"

runners-help:
	@scripts/runner_cli_inventory.sh --out target/tmp/runner_cli_inventory
	@echo ""
	@echo "Runner CLI help captured under: target/tmp/runner_cli_inventory"
	@echo "Next: update docs/runner_cli_inventory.md with findings for approval."

# Release process: bump version, update changelog, build artifacts, create GitHub release
# Usage: make release VERSION=0.2.0
# For dry run: make release-dry-run VERSION=0.2.0
release:
	@scripts/release.sh $(VERSION)

# Release with dry-run mode (no side effects)
# Usage: make release-dry-run VERSION=0.2.0
release-dry-run:
	@RELEASE_DRY_RUN=1 scripts/release.sh $(VERSION)

# Build release artifacts for current platform
# Usage: make release-artifacts [VERSION=0.2.0]
release-artifacts:
	@scripts/build-release-artifacts.sh $(VERSION)

# Build release artifacts for all platforms (requires cross-compilation targets)
# Usage: make release-artifacts-all [VERSION=0.2.0]
release-artifacts-all:
	@scripts/build-release-artifacts.sh --all $(VERSION)

# Generate changelog entries from commits since last tag
# Usage: make changelog
changelog:
	@scripts/generate-changelog.sh

# Preview changelog changes without modifying files
# Usage: make changelog-preview
changelog-preview:
	@scripts/generate-changelog.sh --dry-run

# Check if changelog is up to date (CI)
# Usage: make changelog-check
changelog-check:
	@scripts/generate-changelog.sh --check
