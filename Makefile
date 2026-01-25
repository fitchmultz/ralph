RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph

.PHONY: install update lint type-check format clean clean-temp test generate build build-release ci

install: build-release
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
	cargo clippy --workspace --all-targets -- -D warnings

type-check:
	cargo check --workspace --all-targets

format:
	cargo fmt --all

clean: clean-temp
	cargo clean
	find . -name '*.log' -type f -delete
	rm -rf .ralph/cache .ralph/lock .ralph/logs

clean-temp:
	rm -rf target/tmp

test:
	@bash -lc 'set -euo pipefail; \
		repo_root="$$(pwd -P)"; \
		\
		# Legacy (previous behavior): CI temp base lived under system temp. \
		# Keep cleaning it so old runs do not continue to consume /private/var/folders on macOS. \
		system_tmp="$${TMPDIR:-/tmp}"; \
		system_tmp="$${system_tmp%/}"; \
		legacy_tmp_base="$$system_tmp/ralph-ci-tmp"; \
		if [ "$${RALPH_CI_KEEP_TMP:-0}" != "1" ]; then \
			rm -rf "$$legacy_tmp_base" || true; \
		fi; \
		\
		# New behavior: force CI temp space under repo-local target/tmp (disposable + easy to clean). \
		tmp_base="$$repo_root/target/tmp/ralph-ci-tmp"; \
		if [ "$${RALPH_CI_KEEP_TMP:-0}" != "1" ]; then \
			rm -rf "$$tmp_base" || true; \
		fi; \
		mkdir -p "$$tmp_base"; \
		\
		run_dir="$$(mktemp -d "$$tmp_base/ralph-ci.XXXXXX")"; \
		cleanup() { \
			if [ "$${RALPH_CI_KEEP_TMP:-0}" = "1" ]; then \
				echo "Keeping CI temp dir: $$run_dir"; \
				return 0; \
			fi; \
			rm -rf "$$run_dir" || true; \
			rm -rf "$$tmp_base" || true; \
		}; \
		trap cleanup EXIT INT TERM; \
		\
		# Ensure Rust + tempfile + any child processes use the repo-local temp area. \
		export TMPDIR="$$run_dir"; \
		export TEMP="$$run_dir"; \
		export TMP="$$run_dir"; \
		\
		echo "CI temp dir: $$run_dir"; \
		cargo test --workspace --all-targets -- --include-ignored; \
		cargo test --workspace --doc -- --include-ignored; \
		cargo build --workspace --release; \
	'

stress:
	@echo "Running burn-in stress tests..."
	RALPH_STRESS_BURN_IN=1 cargo test -p ralph --test stress_queue_contract_test --release -- --ignored --nocapture

generate:
	@mkdir -p schemas
	@cargo run -q --bin ralph -- config schema > schemas/config.schema.json
	@cargo run -q --bin ralph -- queue schema > schemas/queue.schema.json
	@echo "Schemas generated in schemas/"

build:
	cargo build --workspace

build-release:
	cargo build --workspace --release

ci: generate format type-check lint build test install
