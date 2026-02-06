# Repository Guidelines (Ralph)

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.
This file is a fast path for contributors/agents; for deeper detail start at `docs/index.md` and `CONTRIBUTING.md`.

## Quick Start

```bash
# Before committing or merging, always run:
make ci
```

The CI gate runs: `check-env-safety → check-backup-artifacts → deps → format → type-check → lint → test → build → generate → install`

## Project Structure

```
crates/
  ralph/              # Primary Rust CLI crate
    src/              # CLI commands, runner integration, queue management, TUI
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

## Build, Test, and Development

### Essential Commands

| Command | Purpose |
|---------|---------|
| `make ci` | Local CI gate — **must pass before committing** |
| `make install` | Install `ralph` to `~/.local/bin/ralph` (or writable fallback) |
| `make test` | Run workspace unit + doc tests in isolated temp dirs |
| `make lint` | Run Clippy with `-D warnings` (warnings are errors) |
| `make format` | Run `cargo fmt --all` |
| `make type-check` | Run `cargo check --workspace --all-targets` |
| `make generate` | Regenerate JSON schemas into `schemas/` |
| `make update` | Update Cargo dependencies (`cargo update`) |
| `make clean` | Remove build artifacts, logs, and most cache entries |

### Development Iteration

```bash
# Quick test cycle (not a substitute for `make ci`)
cargo test -p ralph
cargo run -p ralph -- <command>
cargo run -p ralph -- queue validate
```

## Coding Standards

### Rust Conventions

- **Formatting**: `cargo fmt` + Clippy with `-D warnings` (CI treats warnings as errors)
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

See `docs/error-handling.md` for full guidelines.

### Module Documentation

Every new/changed source file MUST start with module docs (`//!`) stating:

- What the file is responsible for
- What it explicitly does NOT handle
- Any invariants/assumptions callers must respect

## Testing Guidelines

- **Unit tests**: Colocate with implementation via `#[cfg(test)]`
- **Integration tests**: Use `crates/ralph/tests/` when cross-module behavior is the subject
- **Temp dirs**: CI tests run in `target/tmp/ralph-ci-tmp/` (set `RALPH_CI_KEEP_TMP=1` to keep)
- **Init tests**: When calling `ralph init` in tests, always use `--non-interactive`:
  ```rust
  ralph init --force --non-interactive
  ```
  Without this flag, TTY detection may trigger the interactive wizard in test environments.

## Git Hygiene

- **Commit messages**: `RQ-####: <short summary>` (task id + summary)
- **Do not commit** if `make ci` is failing
- **This repo is local-CI-first**; avoid adding remote CI (e.g., GitHub Actions) as a substitute for `make ci`
- **Keep secrets out of git/logs**: `.env` is for local use only and MUST remain untracked

## Pull Request Guidelines

- Include "what changed" + "how to verify" sections (expected: `make ci`)
- Call out breaking behavior explicitly and update docs/help accordingly
- When working from an issue/PR, prefer `gh` for context:
  ```bash
  gh issue view <number>
  gh pr view <number>
  ```

## Configuration

Config precedence (highest to lowest):

1. CLI flags
2. Project config: `.ralph/config.json`
3. Global config: `~/.config/ralph/config.json`
4. Schema defaults: `schemas/config.schema.json`

See `docs/configuration.md` for key fields (runner/model/phases/RepoPrompt toggles/CI gate settings).
Runner/model specifics live in `README.md`.

## Workflow Contracts

### Queue and Prompts

- **Queue is the source of truth**: `.ralph/queue.json` (active) and `.ralph/done.json` (archive)
- **Task ordering**: Queue file order is execution order (top runs first). Draft tasks are skipped unless `--include-draft`
- **Prompt composition**: Embedded defaults in `crates/ralph/assets/prompts/`, overridden by `.ralph/prompts/*.md`
- **Planning cache**: Phase 1 plans are written to `.ralph/cache/plans/<TASK_ID>.md`
- **Supervision-aware completion**: `ralph task done` writes `.ralph/cache/completions/<TASK_ID>.json`

See `docs/workflow.md` and `docs/queue-and-tasks.md` for full contract and schema details.

### Runner Session Handling

Ralph manages runner sessions explicitly for reliable crash recovery:

**Session ID Format**: `ralph-{task_id}-p{phase}-{timestamp}-{pid}`  
**Example**: `ralph-RQ-0001-p2-1704153600-12345`

**Key Behaviors**:
- Each phase (1, 2, 3) generates its own unique session ID at phase start
- Session IDs are passed to runners via `--session` flag (not `--continue`)
- The same session ID is reused for all continue/resume operations within a phase

**Implementation**: `crates/ralph/src/commands/run/phases/mod.rs` (`generate_phase_session_id`)

See `docs/workflow.md` for more details.

## Migrations

When making breaking changes to config keys or file formats, use the migration system:

- **Migration registry**: `crates/ralph/src/migration/registry.rs`
- **Migration types**: `ConfigKeyRename`, `FileRename`, `ReadmeUpdate`
- **History tracking**: `.ralph/cache/migrations.json` (auto-generated)
- **CLI command**: `ralph migrate` (check/list/apply)

See `crates/ralph/src/migration/mod.rs` for invariants/assumptions (idempotency, JSONC comment preservation, backups).

## Documentation Maintenance

When making changes, keep docs in sync:

| Change Type | Files to Update |
|-------------|-----------------|
| Schema changes | `schemas/*.schema.json`, `docs/configuration.md` |
| CLI changes | Help text/examples, `docs/cli.md` |
| Queue/task fields | `docs/queue-and-tasks.md` |
| Migrations | This file + migration module docs |

## Non-Negotiables

- **CI gate**: `make ci` MUST pass before claiming completion, committing, or merging
- **Source docs**: Every new/changed source file MUST have module docs (see [Coding Standards](#coding-standards))
- **Test coverage**: All new/changed behavior must be covered (success + failure modes)
- **Feature parity**: When changing user-visible workflows, maintain parity between CLI and TUI (or document divergence)
- **CLI help**: User-facing commands/flags MUST have `--help` text with examples (keep `docs/cli.md` in sync)
- **Secrets**: Never commit or print secrets; redact runner output before copying into `.ralph/queue.json` notes

## Troubleshooting

| Issue | Solution |
|-------|----------|
| CI failing | Run `make ci`; first failing step is printed (common: formatting, Clippy warnings, tests) |
| `.env tracked` error | Run `git rm --cached .env` and ensure `.env` is in `.gitignore` |
| `Backup artifacts` error | Remove any `*.bak` files under `crates/ralph/src/` |
| Queue lock | Investigate `.ralph/lock`; use `--force` only when you understand why the lock is stale |
| Runner issues | Verify runner binary is on `PATH` (e.g., `codex --help`) and check runner/model settings in config |
