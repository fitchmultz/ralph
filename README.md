# Ralph

Ralph is a tool for managing AI agent loops with a structured YAML task queue.

## Current Status (Rust rewrite)

The canonical implementation is the Rust CLI in `crates/ralph/`.

- Queue (source of truth): `.ralph/queue.yaml`
- Done archive: `.ralph/done.yaml`
- Prompt templates: `.ralph/prompts/`

The legacy Go TUI/CLI lives in `ralph_tui/` and uses the Markdown pin workflow under `.ralph/pin/`. That Go implementation is frozen during the Rust rewrite and should only be modified when a queue task explicitly targets it.

## Quick Start (Rust)

- Install the `ralph` binary to `~/.local/bin`:
  - `make install`
- Run tests:
  - `cargo test --workspace`
- Validate queue:
  - `cargo run -p ralph -- queue validate`
- Add a task from a request:
  - `cargo run -p ralph -- task build "<request>"`
- Seed the backlog with a scan:
  - `cargo run -p ralph -- scan --focus "<focus>"`
- Execute the next task (first `todo` task in queue order):
  - `cargo run -p ralph -- run one`
- Archive completed tasks:
  - `cargo run -p ralph -- queue done`

## Go vs Rust Mapping

Rust is the canonical workflow for queue-driven execution. The Go TUI remains for legacy pin workflows only.

- Queue validation: Go `ralph pin validate` -> Rust `ralph queue validate`
- Backlog management: Go `.ralph/pin/*` -> Rust `.ralph/queue.yaml` + `.ralph/done.yaml`
- Task execution loop: Go TUI loop -> Rust `ralph run one` / `ralph run loop`

## Configuration

Ralph uses a two-layer YAML config:
- Global: `~/.config/ralph/config.yaml`
- Project: `.ralph/config.yaml` (overrides global)

## Project Types

Ralph supports a configurable `project_type` (`code` or `docs`) to tune prompts and workflows. This is read from config and primarily affects prompt defaults.

See `.ralph/README.md` for Rust runtime-file details.
