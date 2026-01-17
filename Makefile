RUST_WORKSPACE := .
PREFIX ?= $(HOME)/.local
BIN_DIR ?= $(PREFIX)/bin
BIN_NAME ?= ralph
BIN_PATH ?= $(BIN_DIR)/$(BIN_NAME)

.PHONY: install update lint type-check format clean test generate build ci

install:
	cargo build --workspace --release
	mkdir -p $(BIN_DIR)
	install -m 0755 target/release/ralph $(BIN_PATH)

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
