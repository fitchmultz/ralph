# CRITICAL: DATE AWARENESS - CURRENT DATE IS FEBRUARY 2026 (NOT 2024)

> ⚠️ **TIMEFRAME CRITICAL**: The current date is **February 2026**. All documentation referencing "2024" is OUTDATED.

## Date Verification Protocol
- **ALWAYS** check the current system date at the start of each session
- Use 2026 as the reference timeframe for ALL decisions, searches, and recommendations
- **NEVER** use 2024 data as a benchmark or current reference

## Up-to-Date Information Requirement
- You have access to web search tools - **USE THEM** to find current information as of 2026
- When researching libraries, frameworks, APIs, or best practices, search for "2026" or "latest" versions
- Do not assume 2024 documentation is still current; verify with web search
- Default to the most recent stable versions available in 2026

---

# Repository Guidelines (Ralph)

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.

For deeper architectural detail, see `docs/index.md` and `CONTRIBUTING.md`.

---

## Quick Start

```bash
# Before committing or merging, always run:
make macos-ci
```

The CI gate runs: `check-env-safety → check-backup-artifacts → deps → format → type-check → lint → test → build → generate → install`

---

## Project Structure

```
apps/
  RalphMac/           # macOS SwiftUI app (thin client that shells out to the bundled ralph CLI)
crates/
  ralph/              # Primary Rust CLI crate
    src/              # CLI commands, runner integration, queue management
    assets/prompts/   # Embedded prompt templates (worker/task builder/scan)
    tests/            # Integration tests
docs/                 # CLI + workflow + configuration docs
schemas/              # Generated JSON schemas (committed)
scripts/              # Maintenance + release helper scripts
.ralph/               # Repo-local runtime state
  queue.json          # Active tasks (source of truth)
  done.json           # Archived tasks
  config.json         # Project config (overrides global)
  prompts/*.md        # Optional prompt overrides
```

---

## Development Commands

| Command | Purpose |
|---------|---------|
| `make macos-ci` | **Local CI gate — must pass before committing** (Rust + Xcode; warnings are errors) |
| `make install` | Install `ralph` to `~/.local/bin/ralph` (or writable fallback) |
| `make test` | Run workspace unit + doc tests in isolated temp dirs |
| `make lint` | Run Clippy with `-D warnings` (warnings are errors) |
| `make format` | Run `cargo fmt --all` |
| `make type-check` | Run `cargo check --workspace --all-targets` |
| `make generate` | Regenerate JSON schemas into `schemas/` |
| `make update` | Update Cargo dependencies (`cargo update`) |
| `make clean` | Remove build artifacts, logs, and most cache entries |

### Running Tests

```bash
# All tests (CI)
make test

# Single crate tests
cargo test -p ralph

# Run a specific test
cargo test -p ralph -- test_name_pattern

# Run tests matching a pattern
cargo test -p ralph queue::operations::edit

# Keep temp directories for debugging
RALPH_CI_KEEP_TMP=1 make test
```

### Quick Development Cycle

```bash
# Quick test cycle (not a substitute for `make macos-ci`)
cargo test -p ralph
cargo run -p ralph -- <command>
cargo run -p ralph -- queue validate
```

---

## Coding Standards

### Rust Conventions

- **Formatting**: `cargo fmt` + Clippy with `-D warnings`; Xcode with `SWIFT_TREAT_WARNINGS_AS_ERRORS=YES` (CI treats warnings as errors for both Rust and Swift)
- **Visibility**: Keep APIs small; default to private, prefer `pub(crate)` over `pub`
- **Cohesion**: Keep modules/files focused; split large files rather than growing grab-bags

### Error Handling

Ralph uses a two-tier strategy: `anyhow` for general propagation, `thiserror` for domain-specific errors.

| Scenario | Pattern | Example |
|----------|---------|---------|
| Propagating errors | `anyhow::Result<T>` | `fn foo() -> Result<T>` |
| Quick error return | `bail!` | `bail!("invalid input")` |
| Adding context | `.context()` | `.context("read config")` |
| Matchable domain errors | `thiserror` | `RunnerError`, `GitError` |
| CLI value parsers | `anyhow::Result` | `parse_phase()` |

### Module Documentation

Every new/changed source file MUST start with module docs (`//!`) stating:

- What the file is responsible for
- What it explicitly does NOT handle
- Any invariants/assumptions callers must respect

Example:

```rust
//! Ralph CLI entrypoint and command routing.
//!
//! Responsibilities:
//! - Load environment defaults, parse CLI args, and dispatch to command handlers.
//! - Initialize logging/redaction and apply CLI-level behavior toggles.
//!
//! Not handled here:
//! - CLI flag definitions (see `crate::cli`).
//! - Queue persistence, prompt rendering, or runner execution.
//!
//! Invariants/assumptions:
//! - CLI arguments are normalized before Clap parsing.
//! - Command handlers enforce their own safety checks and validation.
```

### File Boundaries

- **Target**: Keep individual source files under ~500 LOC
- **Soft limit**: Files exceeding ~800 LOC require explicit justification
- **Hard limit**: Files exceeding ~1,000 LOC are presumed mis-scoped and MUST be split
- Prefer internal module splits over public API expansion

### Naming Conventions

- Functions: `snake_case`
- Types: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`
- CLI commands: `kebab-case` (e.g., `run one`, `queue list`)

### Dead Code Management

- Prefer explicit, minimal usage patterns over `#[allow(dead_code)]` when preserving public APIs
- Remove truly dead code rather than suppressing warnings
- Three occurrences of the same pattern = must abstract into shared helper

---

## Testing Guidelines

- **Unit tests**: Colocate with implementation via `#[cfg(test)]`
- **Integration tests**: Use `crates/ralph/tests/` when cross-module behavior is the subject
- **Temp dirs**: CI tests run in `target/tmp/ralph-ci-tmp/` (set `RALPH_CI_KEEP_TMP=1` to keep)
- **Init tests**: When calling `ralph init` in tests, always use `--non-interactive`:

  ```rust
  ralph init --force --non-interactive
  ```

---

## Git Hygiene

- **Commit messages**: `RQ-####: <short summary>` (task id + summary)
- **Do not commit** if `make macos-ci` is failing
- **This repo is local-CI-first**; avoid adding remote CI (e.g., GitHub Actions) as a substitute for `make macos-ci`
- **Keep secrets out of git/logs**: `.env` is for local use only and MUST remain untracked

---

## Configuration

Config precedence (highest to lowest):

1. CLI flags
2. Project config: `.ralph/config.json`
3. Global config: `~/.config/ralph/config.json`
4. Schema defaults: `schemas/config.schema.json`

See `docs/configuration.md` for key fields (runner/model/phases/RepoPrompt toggles/CI gate settings).

---

## Workflow Contracts

### Queue and Prompts

- **Queue is the source of truth**: `.ralph/queue.json` (active) and `.ralph/done.json` (archive)
- **Task ordering**: Queue file order is execution order (top runs first). Draft tasks are skipped unless `--include-draft`
- **Queue backups are bounded**: `.ralph/cache/queue.json.backup*` files must be auto-pruned with explicit retention limits (no unbounded cache growth)
- **Prompt composition**: Embedded defaults in `crates/ralph/assets/prompts/`, overridden by `.ralph/prompts/*.md`
- **Planning cache**: Phase 1 plans are written to `.ralph/cache/plans/<TASK_ID>.md`
- **Supervision-aware completion**: `ralph task done` writes `.ralph/cache/completions/<TASK_ID>.json`

### Runner Session Handling

Ralph manages runner sessions explicitly for reliable crash recovery:

**Session ID Format**: `{task_id}-p{phase}-{timestamp}`  
**Example**: `RQ-0001-p2-1704153600`

Note: `timestamp` is Unix epoch seconds. No `ralph-` prefix; no pid suffix.

**Key Behaviors**:
- Each phase (1, 2, 3) generates its own unique session ID at phase start
- Session IDs are passed to runners via `--session` flag (not `--continue`)
- The same session ID is reused for all continue/resume operations within a phase

**Implementation**: `crates/ralph/src/commands/run/phases/mod.rs` (`generate_phase_session_id`)

---

## Migrations

When making breaking changes to config keys or file formats, use the migration system:

- **Migration registry**: `crates/ralph/src/migration/registry.rs`
- **Migration types**: `ConfigKeyRename`, `FileRename`, `ReadmeUpdate`
- **History tracking**: `.ralph/cache/migrations.json` (auto-generated)
- **CLI command**: `ralph migrate` (check/list/apply)

---

## Documentation Maintenance

When making changes, keep docs in sync:

| Change Type | Files to Update |
|-------------|-----------------|
| Schema changes | `schemas/*.schema.json`, `docs/configuration.md` |
| CLI changes | Help text/examples, `docs/cli.md` |
| Queue/task fields | `docs/queue-and-tasks.md` |
| Migrations | This file + migration module docs |

---

## Non-Negotiables

- **CI gate**: `make macos-ci` MUST pass before claiming completion, committing, or merging
- **Source docs**: Every new/changed source file MUST have module docs (see [Coding Standards](#coding-standards))
- **Test coverage**: All new/changed behavior must be covered (success + failure modes)
- **File size**: Individual source files SHOULD remain under ~500 LOC; files over ~1,000 LOC MUST be split
- **Feature parity**: When changing user-visible workflows, maintain parity between the CLI and the macOS app (or document divergence)
- **CLI help**: User-facing commands/flags MUST have `--help` text with examples (keep `docs/cli.md` in sync)
- **Secrets**: Never commit or print secrets; redact runner output before copying into `.ralph/queue.json` notes

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| CI failing | Run `make macos-ci`; first failing step is printed (common: formatting, Clippy warnings, tests) |
| `.env tracked` error | Run `git rm --cached .env` and ensure `.env` is in `.gitignore` |
| `Backup artifacts` error | Remove any `*.bak` files under `crates/ralph/src/` |
| Queue lock | Investigate `.ralph/lock`; use `--force` only when you understand why the lock is stale |
| Runner issues | Verify runner binary is on `PATH` (e.g., `codex --help`) and check runner/model settings in config |
