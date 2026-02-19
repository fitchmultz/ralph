# Repository Guidelines (Ralph)

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.
This file is a fast path for contributors/agents; for deeper detail start at `docs/index.md` and `CONTRIBUTING.md`.

## TL;DR

- Run `make agent-ci` before claiming completion, committing, or merging.
- `make agent-ci` is the default contributor gate; it currently runs the Rust/CLI pipeline:
  `check-env-safety → check-backup-artifacts → deps → format → type-check → lint → test → build → generate → install`
- `make macos-ci` remains the ship gate for full macOS app build+test.
- Keep secrets out of git/logs; `.env` is for local use only and MUST remain untracked (CI enforces this).

## Non-Negotiables

- CI gate: `make agent-ci` MUST pass before claiming completion, committing, or merging.
- Source docs: every new/changed source file MUST start with module docs that state:
  - what the file is responsible for
  - what it explicitly does NOT handle
  - any invariants/assumptions callers must respect
  - (Rust: prefer `//!` module docs at the top of the file.)
- Tests: all new/changed behavior must be covered (success + failure modes). Prefer tests near the code.
- Feature parity: when changing a user-visible workflow, maintain parity between the CLI and the macOS app (or document/justify the divergence explicitly).
- CLI help: user-facing commands/flags MUST have `--help` text with examples (and keep `docs/cli.md` in sync).
- Secrets: never commit or print secrets; redact runner output before copying into `.ralph/queue.json` notes.

## Repository Map

- `apps/RalphMac/`: macOS SwiftUI app (thin client that shells out to the bundled `ralph` CLI)
- `crates/ralph/`: primary Rust CLI crate
  - `crates/ralph/src/`: CLI commands, runner integration, queue management
  - `crates/ralph/assets/prompts/`: embedded prompt templates (worker/task builder/scan)
- `docs/`: CLI + workflow + configuration docs (`docs/index.md` is the entry point)
- `schemas/`: generated JSON schemas (committed)
- `scripts/`: maintenance + release helper scripts
- `.ralph/`: repo-local runtime state (partially committed; queue.json is tracked)
  - `.ralph/queue.json`: active tasks (source of truth)
  - `.ralph/done.json`: archived tasks
  - `.ralph/config.json`: project config (overrides global)
  - `.ralph/prompts/*.md`: optional prompt overrides

## Build, Test, and CI

The Makefile is the contract; keep these targets working:

- `make agent-ci`: default local CI gate (Rust/CLI always; escalates to `make macos-ci` when `apps/RalphMac/` changes).
- `make macos-ci`: full ship gate (Rust CI + macOS app build+test).
- `make install`: install `ralph` to `~/.local/bin/ralph` (or a writable fallback) and sanity-check `ralph --help`.
- `make test`: runs `cargo nextest run` for workspace tests (with fallback to `cargo test` if nextest missing), then `cargo test --doc` for doctests.
- `make lint`: `cargo clippy --workspace --all-targets -- -D warnings`
- `make format`: `cargo fmt --all`
- `make type-check`: `cargo check --workspace --all-targets`
- `make generate`: regenerates JSON schemas into `schemas/`
- `make update`: updates Cargo dependencies (`cargo update`)
- `make clean`: removes build artifacts, logs, and most `.ralph/cache` entries

Useful iteration commands (not a substitute for `make agent-ci`):

- `cargo test -p ralph`
- `cargo run -p ralph -- <command>`
- `cargo run -p ralph -- queue validate`

## Rust Conventions (Project Defaults)

- Formatting/linting: `cargo fmt` + Clippy with `-D warnings` (CI treats warnings as errors).
- Visibility: keep APIs small; default to private, prefer `pub(crate)` over `pub`.
- Errors: prefer descriptive error types (`thiserror`) and `Result<T, E>` over panics.
- Cohesion: keep modules/files focused; split large files rather than growing grab-bags.

## Testing

- Unit tests: colocate with implementation via `#[cfg(test)]`.
- Integration tests: use `crates/ralph/tests/` when cross-module behavior is the subject.
- Temp dirs: CI tests run in `target/tmp/ralph-ci-tmp/` (set `RALPH_CI_KEEP_TMP=1` to keep).
- **Init tests**: when calling `ralph init` in integration tests, always use `--non-interactive` (e.g., `ralph init --force --non-interactive`). Without this flag, TTY detection may trigger the interactive wizard in test environments, breaking the CI gate.

## Queue, Prompts, and Workflow Contracts

- Queue is the source of truth: `.ralph/queue.json` (active) and `.ralph/done.json` (archive).
- Task ordering: queue file order is execution order (top runs first). Draft tasks are skipped unless `--include-draft`.
- Prompt composition: embedded defaults in `crates/ralph/assets/prompts/`, overridden by `.ralph/prompts/*.md`.
- Planning cache: Phase 1 plans are written to `.ralph/cache/plans/<TASK_ID>.md` (do not print inline).
- Supervision-aware completion: `ralph task done` writes `.ralph/cache/completions/<TASK_ID>.json` for the supervisor flow.

See `docs/workflow.md` and `docs/queue-and-tasks.md` for the full contract and schema details.

## Configuration

Config precedence (highest to lowest):

1. CLI flags
2. Project config: `.ralph/config.json`
3. Global config: `~/.config/ralph/config.json`
4. Schema defaults: `schemas/config.schema.json`

See `docs/configuration.md` for key fields (runner/model/phases/RepoPrompt toggles/CI gate settings).
Runner/model specifics live in `README.md` (supported runners and model constraints).

## Git Hygiene

- Commit message: `RQ-####: <short summary>` (task id + summary).
- Do not commit if `make agent-ci` is failing.
- This repo is local-CI-first; avoid adding remote CI (e.g., GitHub Actions) as a substitute for `make agent-ci`.

## PR / Review Expectations

- Include a short "what changed" + "how to verify" section (expected: `make agent-ci`).
- Call out any breaking behavior explicitly and update docs/help accordingly.
- When working from an issue/PR, prefer `gh` for context (`gh issue view ...`, `gh pr view ...`).

## Migrations

When making breaking changes to config keys or file formats, use the migration system:

- Migration registry: `crates/ralph/src/migration/registry.rs` - add new migrations here
- Migration types: `ConfigKeyRename`, `FileRename`, `ReadmeUpdate`
- History tracking: `.ralph/cache/migrations.json` (auto-generated)
- CLI command: `ralph migrate` (check/list/apply)

See `crates/ralph/src/migration/mod.rs` for invariants/assumptions (idempotency, JSONC comment preservation, backups).

## Documentation Maintenance

- Schema changes: update code, run `make generate`, and keep `schemas/*.schema.json` + `docs/configuration.md` aligned.
- CLI changes: update help text/examples and keep `docs/cli.md` aligned.
- Queue/task field changes: update `docs/queue-and-tasks.md`.
- Migration changes: update this section and the migration module docs.

## Troubleshooting

- CI failing: run `make agent-ci`; the first failing step is printed (common: formatting, Clippy warnings, tests).
- `.env tracked` error: run `git rm --cached .env` and ensure `.env` is in `.gitignore`.
- `Backup artifacts` error: remove any `*.bak` files under `crates/ralph/src/`.
- Queue lock: investigate `.ralph/lock`; use `--force` only when you understand why the lock is stale.
- Runner issues: verify the runner binary is on `PATH` (e.g., `codex --help`) and check runner/model settings in config.
