# Repository Guidelines (Ralph)

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.

This file provides fast-path guidance for contributors and agents. For deeper architectural detail, start with `docs/index.md` and `CONTRIBUTING.md`.

---

## Project Overview

**Ralph** is a Rust-based CLI tool that manages AI agent workflows through a structured JSON task queue. It orchestrates AI runners (Claude, Codex, OpenCode, Gemini, Kimi, Cursor, Pi) to execute tasks in phases (Plan → Implement → Review) with support for dependency management, parallel execution, and session recovery.

### Key Features

- **Task Queue Management**: JSON-based queue with task lifecycle (todo → doing → done)
- **Multi-Phase Execution**: Configurable 1/2/3-phase workflows
- **Multi-Runner Support**: Codex, OpenCode, Gemini, Claude, Cursor, Kimi, Pi
- **Parallel Execution**: Run multiple tasks concurrently with direct-push integration
- **Session Management**: Crash recovery and resumption
- **Plugin System**: Extensible architecture for custom processors
- **Webhooks**: HTTP event notifications for task events
- **macOS App**: SwiftUI companion app (`apps/RalphMac/`)

---

## Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (Edition 2024) |
| CLI Framework | clap 4.x with derive macros |
| Serialization | serde, serde_json, serde_yaml |
| Error Handling | anyhow + thiserror |
| Logging | env_logger + log |
| Build Tool | Cargo + Makefile |
| Testing | Built-in test framework + insta (snapshots) + serial_test |
| JSON Schema | schemars |
| Time Handling | chrono, time |
| Process Management | ctrlc, notify |

---

## Project Structure

```
apps/
  RalphMac/           # macOS SwiftUI app (thin client that shells out to the bundled ralph CLI)
crates/
  ralph/              # Primary Rust CLI crate
    src/              # CLI commands, runner integration, queue management
      cli/            # CLI argument parsing and command handlers
      commands/       # Command implementations
      contracts/      # Data types (Queue, Task, Config, etc.)
      runner/         # Runner execution and session management
      plugins/        # Plugin system
      migration/      # Config/data migration system
      testsupport/    # Test utilities
      assets/         # Embedded assets
    assets/prompts/   # Embedded prompt templates (worker/task builder/scan)
    tests/            # Integration tests
docs/                 # CLI + workflow + configuration docs
schemas/              # Generated JSON schemas (committed)
scripts/              # Maintenance + release helper scripts
.ralph/               # Repo-local runtime state
  queue.json          # Active tasks (source of truth)
  done.json           # Archived tasks
  config.jsonc        # Project config (overrides global; .json fallback still supported)
  prompts/*.md        # Optional prompt overrides
```

### Module Organization

**Core Modules** (`src/`):
- `cli/` - CLI argument definitions and command routing
- `commands/` - Command implementations
- `contracts/` - Data structures (Queue, Task, Config, Session, etc.)
- `runner/` - Runner execution, session management, and phase orchestration
- `plugins/` - Plugin discovery, registry, and execution
- `migration/` - Config and data migration system

**Utility Modules**:
- `config.rs` - Configuration loading and resolution
- `queue.rs` - Queue file operations
- `lock.rs` - Queue file locking
- `redaction.rs` - Sensitive data redaction
- `git/` - Git operations
- `fsutil.rs` - Filesystem utilities
- `timeutil.rs` - Time handling
- `template/` - Prompt template loading
- `prompts_internal/` - Internal prompt composition

---

## Quick Start

```bash
# Before committing or merging, always run:
make ci
```

The CI gate runs: `check-env-safety → check-backup-artifacts → deps → format → type-check → lint → test → build → generate → install`

---

## Development Workflow

### Essential Commands

| Command | Purpose |
|---------|---------|
| `make ci` | Local CI gate — **must pass before committing** |
| `make macos-ci` | macOS-only ship gate (Rust CI + Xcode build + Xcode tests) |
| `make install` | Install `ralph` to `~/.local/bin/ralph` (or writable fallback) |
| `make test` | Nextest workspace tests + cargo doc tests (auto-fallback if nextest missing) |
| `make lint` | Run Clippy with `-D warnings` (warnings are errors) |
| `make format` | Run `cargo fmt --all` |
| `make type-check` | Run `cargo check --workspace --all-targets` |
| `make generate` | Regenerate JSON schemas into `schemas/` |
| `make update` | Upgrade direct Cargo requirements (`cargo upgrade --incompatible`) and refresh the lockfile (`cargo update`); use `make macos-ci` to verify the Swift/Xcode app because it has no external package manifest |
| `make clean` | Remove build artifacts, logs, and most cache entries |

### Development Iteration

```bash
# Quick test cycle (not a substitute for `make ci`)
cargo test -p ralph-agent-loop
cargo run -p ralph-agent-loop -- <command>
cargo run -p ralph-agent-loop -- queue validate
```

---

## Build and Test Commands

### Build

```bash
# Release build (used by install)
make build

# Debug build
cargo build -p ralph-agent-loop

# Check only (fast)
make type-check
```

### Test

```bash
# Full test suite (nextest workspace tests with fallback + cargo doc tests in isolated temp dirs)
make test

# Quick unit tests only
cargo test -p ralph-agent-loop

# Include ignored tests
cargo test -p ralph-agent-loop -- --include-ignored

# Update snapshots after intentional changes
INSTA_UPDATE=always cargo test -p ralph-agent-loop

# Keep temp directories for debugging
RALPH_CI_KEEP_TMP=1 make test
```

### Lint and Format

```bash
# Format code
make format

# Run Clippy (warnings are errors)
make lint

# Type check
make type-check
```

---

## Coding Standards

### Rust Conventions

- **Formatting**: `cargo fmt` + Clippy with `-D warnings` (CI treats warnings as errors)
- **Visibility**: Keep APIs small; default to private, prefer `pub(crate)` over `pub`
- **Cohesion**: Keep modules/files focused; split large files rather than growing grab-bags
- **Documentation**: Every module/file MUST have module docs (`//!`) stating:
  - What the file is responsible for
  - What it explicitly does NOT handle
  - Any invariants/assumptions callers must respect

### Error Handling

Ralph uses a two-tier strategy: `anyhow` for general propagation, `thiserror` for domain-specific errors.

| Scenario | Pattern | Example |
|----------|---------|---------|
| Propagating errors | `anyhow::Result<T>` | `fn foo() -> Result<T>` |
| Quick error return | `bail!` | `bail!("invalid input")` |
| Adding context | `.context()` | `.context("read config")` |
| Matchable domain errors | `thiserror` | `RunnerError`, `GitError` |
| CLI value parsers | `anyhow::Result` | `parse_phase()` |

See `docs/error-handling.md` for full guidelines.

### Security-Conscious Patterns

- **Redaction**: Use `RedactedString` for runner output; apply `redact_text()` before logging
- **Debug Logs**: Never commit debug logs (`.ralph/logs/debug.log`); they contain raw unredacted output
- **Secrets**: Never commit `.env` files or secrets to version control

---

## Testing Instructions

### Test Organization

| Test Type | Location |
|-----------|----------|
| Unit tests | Inline via `#[cfg(test)]` in source files |
| Integration tests | `crates/ralph/tests/` |
| Test utilities | `crates/ralph/src/testsupport/` |

### Integration Testing Pattern

Use `testsupport` helpers for CLI tests:

```rust
let dir = test_support::temp_dir_outside_repo();
test_support::git_init(dir.path())?;
test_support::ralph_init(dir.path())?;

test_support::write_queue(dir.path(), &tasks)?;
let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "archive"]);
anyhow::ensure!(status.success(), "...\nstdout:\n{stdout}\nstderr:\n{stderr}");
```

### Test Isolation

- Tests run in isolated temp directories (`${TMPDIR:-/tmp}/ralph-ci.*`)
- Use `--non-interactive` flag when calling `ralph init` in tests
- Set `RALPH_CI_KEEP_TMP=1` to preserve temp directories for debugging
- Use `serial_test` for tests that modify global state

---

## Security Considerations

### Sensitive Data Handling

- **Redaction**: Built-in redaction masks API keys, tokens, AWS keys, SSH keys
- **Debug Mode**: `--debug` flag writes raw (unredacted) output to `.ralph/logs/debug.log`
- **Safeguard Dumps**: Use redacted dumps by default; raw dumps require `RALPH_RAW_DUMP=1` or `--debug`

### What to Never Commit

- `.env` files (MUST be in `.gitignore`)
- `.ralph/logs/` directory
- `*.bak` files in source directories
- Debug logs or raw safeguard dumps

### Environment Safety Checks

The CI runs `check-env-safety` which fails if `.env` is tracked in git.

---

## Git Hygiene

- **Commit messages**: `RQ-####: <short summary>` (task id + summary)
- **Do not commit** if `make ci` is failing
- **This repo is local-CI-first**; avoid adding remote CI (e.g., GitHub Actions) as a substitute for `make ci`
- **Keep secrets out of git/logs**: `.env` is for local use only and MUST remain untracked

---

## Pull Request Guidelines

- Include "what changed" + "how to verify" sections (expected: `make ci`)
- Call out breaking behavior explicitly and update docs/help accordingly
- When working from an issue/PR, prefer `gh` for context:

  ```bash
  gh issue view <number>
  gh pr view <number>
  ```

---

## Configuration

Config precedence (highest to lowest):

1. CLI flags
2. Project config: `.ralph/config.jsonc` (`.json` fallback)
3. Global config: `~/.config/ralph/config.jsonc` (`.json` fallback)
4. Schema defaults: `schemas/config.schema.json`

See `docs/configuration.md` for key fields (runner/model/phases/RepoPrompt toggles/CI gate settings).
Runner/model specifics live in `README.md`.

---

## Workflow Contracts

### Queue and Prompts

- **Queue is the source of truth**: `.ralph/queue.json` (active) and `.ralph/done.json` (archive)
- **Task ordering**: Queue file order is execution order (top runs first). Draft tasks are skipped unless `--include-draft`
- **Prompt composition**: Embedded defaults in `crates/ralph/assets/prompts/`, overridden by `.ralph/prompts/*.md`
- **Planning cache**: Phase 1 plans are written to `.ralph/cache/plans/<TASK_ID>.md`

See `docs/workflow.md` and `docs/queue-and-tasks.md` for full contract and schema details.

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

See `docs/workflow.md` for more details.

---

## Migrations

When making breaking changes to config keys or file formats, use the migration system:

- **Migration registry**: `crates/ralph/src/migration/registry.rs`
- **Migration types**: `ConfigKeyRename`, `ConfigKeyRemove`, `FileRename`, `ReadmeUpdate`
- **History tracking**: `.ralph/cache/migrations.json` (auto-generated)
- **CLI command**: `ralph migrate` (check/list/apply)

See `crates/ralph/src/migration/mod.rs` for invariants/assumptions (idempotency, JSONC comment preservation, backups).

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

- **CI gate**: `make ci` MUST pass before claiming completion, committing, or merging
- **Source docs**: Every new/changed source file MUST have module docs (see [Coding Standards](#coding-standards))
- **Test coverage**: All new/changed behavior must be covered (success + failure modes)
- **Feature parity**: When changing user-visible workflows, maintain parity between the CLI and the macOS app (or document divergence)
- **CLI help**: User-facing commands/flags MUST have `--help` text with examples (keep `docs/cli.md` in sync)
- **Secrets**: Never commit or print secrets; redact runner output before copying into `.ralph/queue.json` notes

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| CI failing | Run `make ci`; first failing step is printed (common: formatting, Clippy warnings, tests) |
| `.env tracked` error | Run `git rm --cached .env` and ensure `.env` is in `.gitignore` |
| `Backup artifacts` error | Remove any `*.bak` files under `crates/ralph/src/` |
| Queue lock | Investigate `.ralph/lock`; use `--force` only when you understand why the lock is stale |
| Runner issues | Verify runner binary is on `PATH` (e.g., `codex --help`) and check runner/model settings in config |
