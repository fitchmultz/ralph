RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph

.PHONY: install update lint type-check format clean test generate build build-release ci

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

clean:
	cargo clean
	find . -name '*.log' -type f -delete

test:
	cargo test --workspace --all-targets -- --include-ignored
	cargo test --workspace --doc -- --include-ignored
	cargo build --workspace --release

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
