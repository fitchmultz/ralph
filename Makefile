RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph

.PHONY: install update lint type-check format clean test generate build ci

install:
	cargo build --workspace --release
	@bin_dir="$(BIN_DIR)"; \
	if [ ! -w "$$bin_dir" ]; then \
		bin_dir="$(CURDIR)/.local/bin"; \
		echo "install: $(BIN_DIR) not writable; using $$bin_dir"; \
	fi; \
	mkdir -p "$$bin_dir"; \
	install -m 0755 target/release/$(BIN_NAME) "$$bin_dir/$(BIN_NAME)"

update:
	cargo update

lint:
	cargo clippy --workspace --all-targets -- -D warnings

type-check:
	cargo check --workspace

format:
	cargo fmt --all

clean:
	cargo clean
	find . -name '*.log' -type f -delete

test:
	cargo test --workspace

generate:
	@echo "No API type generation configured."

build:
	cargo build --workspace

ci: generate format type-check lint build test install
