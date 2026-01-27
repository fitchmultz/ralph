# Contributor Guide

Purpose: Capture repo-wide operating expectations for contributors and agents working on Ralph, a Rust-based AI agent task queue CLI.

## Project Structure & Architecture

**Active Components:**
- `crates/ralph/`: Rust CLI application (primary codebase)
  - `src/`: Core implementation (CLI commands, runner integration, TUI, queue management)
  - `assets/prompts/`: Embedded prompt templates (worker phases, task builder, scan)
- `docs/`: User-facing documentation (CLI reference, configuration, workflow)
- `schemas/`: JSON schemas for config and queue validation
- `.ralph/`: Repo-local runtime state (not committed)
  - `queue.json`: Source of truth for active work
  - `done.json`: Archive of completed tasks
  - `config.json`: Project-specific configuration (overrides global)
  - `prompts/*.md`: Optional prompt overrides

**Key Architectural Patterns:**
- **Queue-first**: Task queue is the primary source of truth; agents read/write tasks via `.ralph/queue.json`
- **Runner-agnostic**: Supports multiple AI runners (Codex, OpenCode, Gemini, Claude, Cursor) through a unified interface
- **Three-phase workflow**: Planning → Implementation + CI → Review + Completion (configurable)
- **Prompt composition**: Worker prompts combine base `worker.md` with phase-specific wrappers

## Build, Test, and Development Commands

**REQUIRED CI GATE**: `make ci` — Agents MUST run this before claiming task completion, committing, or merging PRs. This intentionally installs the binary and is a hard requirement. Under no circumstances can the CI gate be changed to exclude the install step.

**Core Make Targets:**
- `make build`: Build all crates (debug)
- `make build-release`: Build release binary (for installation)
- `make install`: Build and install binary to `~/.local/bin/ralph` (or fallback path)
- `make test`: Run all tests (unit + doc) in temp directories
- `make lint`: Run Clippy with `-D warnings`
- `make type-check`: Run `cargo check` for type validation
- `make format`: Format code with `cargo fmt`
- `make generate`: Generate JSON schemas from Rust code
- `make clean`: Remove build artifacts, logs, and `.ralph/` cache
- `make ci`: Complete validation pipeline (generate → format → type-check → lint → build → test → install)

**Development Iteration (not a substitute for `make ci`):**
- `cargo test -p ralph`: Run tests for the ralph crate only
- `cargo run -p ralph -- <command>`: Run CLI locally (see `docs/cli.md` for commands)
- `cargo clippy -p ralph`: Quick linting for single crate

**Validation Commands:**
- `cargo run -p ralph -- queue validate`: Verify queue format
- `cargo run -p ralph -- config schema`: View config schema
- `cargo run -p ralph -- queue schema`: View queue schema

## Coding Style & Rust Conventions

**Formatting & Linting:**
- Use `cargo fmt` for formatting (enforced by CI)
- Use `cargo clippy` with `-D warnings` for linting (enforced by CI)
- All warnings are treated as errors in CI

**Rust Patterns:**
- Prefer explicit types over `let` inference for public APIs
- Use `Result<T, E>` for fallible operations with descriptive error types
- Prefer `thiserror` for error type definitions
- Keep modules focused and cohesive; split large files when appropriate
- Use `#[cfg(test)]` for unit tests alongside implementation

**Naming Conventions:**
- Functions: `snake_case`
- Types: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`
- CLI commands: `kebab-case` (e.g., `run one`, `queue list`)

**Dead Code Management:**
- Prefer explicit, minimal usage patterns (e.g., type annotations) over `#[allow(dead_code)]` when preserving public APIs
- Remove truly dead code rather than suppressing warnings

## Testing Guidelines

**Testing Framework:**
- Rust's built-in `cargo test` framework
- Unit tests alongside implementation (`#[cfg(test)]`)
- Integration tests in `tests/` (if added)

**Test Execution:**
- `make test` runs unit, doc, and release build tests
- Tests run in isolated temp directories (under `target/tmp/ralph-ci-tmp/`)
- Set `RALPH_CI_KEEP_TMP=1` to preserve test temp directories for debugging

**Coverage Expectations:**
- All user-facing CLI commands should have tests
- Queue operations (read/write/move) must be covered
- Prompt template rendering should be tested
- Critical paths (CI gate, git operations, runner integration) require tests

**Stress Testing:**
- `make stress`: Runs burn-in stress tests for queue contract validation
- Use `RALPH_STRESS_BURN_IN=1` to enable stress test mode

## Queue & Prompt Contract

**Queue Source of Truth:**
- `.ralph/queue.json`: Active work (JSON array of tasks)
- `.ralph/done.json`: Archived completed/rejected tasks (same schema)
- Task order follows file order (top runs first)

**Task Fields:**
- **Required**: `id`, `title`, `created_at`, `updated_at` (RFC3339)
- **Optional**: `tags`, `scope`, `evidence`, `plan`, `notes`, `status`, `priority`, `request`, `completed_at`, `agent`, `depends_on`, `custom_fields`
- **Defaults**: `status: todo`, `priority: medium`
- See `docs/queue-and-tasks.md` for complete schema

**Task Creation:**
- New tasks inserted at top (position 0) unless first task is `doing` (then position 1)
- Draft tasks (`status: draft`) are skipped unless `--include-draft` is set

**Prompt Templates:**
- Embedded defaults in `crates/ralph/assets/prompts/`
- Override in `.ralph/prompts/*.md` (files referenced by name)
- Worker prompts: base `worker.md` + phase wrappers (`worker_phase1.md`, `worker_phase2.md`, `worker_phase2_handoff.md`, `worker_phase3.md`, `worker_single_phase.md`)

**Two-Phase Planning:**
- Phase 1 agents MUST write plans to `.ralph/cache/plans/<TASK_ID>.md` (do not print inline)
- RepoPrompt produces the plan, but the agent owns correctness—fix discrepancies before committing

**Supervision-Aware Completion:**
- `ralph task done` writes completion signal to `.ralph/cache/completions/<TASK_ID>.json`
- Supervisor consumes signal, runs `queue::complete_task`, then `post_run_supervise` for CI/commit/push
- Prevents lock contention while recording agent completion intent

## Git & CI Expectations

**Lifecycle:**
- Execution agent owns: update queue status → run `make ci` → commit → push
- Supervisor (`ralph run`) verifies repo cleanliness, commits/pushes only if needed
- Draft mode can bypass commit/push for testing

**Commit Conventions:**
- Prefer format: `RQ-####: <short summary>` (where `####` is task ID)
- Examples: `RQ-0042: Add CI schema validation`, `RQ-0007: Fix queue archive race condition`

**Pre-Commit Validation:**
- `make ci` must pass before any commit
- CI gate intentionally includes binary install step—never remove this

## CLI Help Documentation

**Adding New CLI Arguments:**
- Always update `after_long_help` or doc comments with examples
- Examples must cover: new flags, purpose, typical usage patterns
- Verification: Run `cargo run -p ralph -- <command> --help` to review before committing

**Common Gaps to Watch For:**
- Missing `--phases` examples
- `--interactive` (`-i`) flag documentation
- `--repo-prompt <tools|plan|off>` (alias: `-rp`) RepoPrompt mode
- Runner/model override examples
- Git behavior flags (`--git-commit-push-on`, `--git-revert-mode`)

**Documentation Sync:**
- Keep `docs/cli.md` updated when adding/modifying commands
- See `docs/cli.md` for complete command reference and examples

## Configuration

**Config Layers (precedence, highest to lowest):**
1. CLI flags (single run)
2. Project config (`.ralph/config.json`)
3. Global config (`~/.config/ralph/config.json`)
4. Schema defaults (`schemas/config.schema.json`)

**Key Configuration:**
- `agent.runner`: Codex, OpenCode, Gemini, Claude, or Cursor
- `agent.model`: Model ID string
- `agent.phases`: Number of phases (1, 2, or 3)
- `agent.reasoning_effort`: Low, medium, high, xhigh (Codex only)
- `agent.repoprompt_plan_required`: Require RepoPrompt planning step (true/false)
- `agent.repoprompt_tool_injection`: Inject RepoPrompt tooling reminders (true/false)
- `agent.ci_gate_command`: CI validation command (default: `make ci`)
- `agent.ci_gate_enabled`: Enable/disable CI gate (default: true)
- `queue.file`: Queue file path (default: `.ralph/queue.json`)
- `queue.done_file`: Done archive path (default: `.ralph/done.json`)

**RepoPrompt Integration:**
- When `repoprompt_plan_required: true`, agents MUST use RepoPrompt tools during planning (use `context_builder`)
- When `repoprompt_tool_injection: true`, prompts include RepoPrompt tooling reminders; follow them
- CLI `--repo-prompt <tools|plan|off>` (alias: `-rp`) controls both flags together
- RepoPrompt produces plans, but agent owns correctness—fix conflicts before writing
- Preflight: Validate task assumptions and identify relevant files before invoking `context_builder`
- Selection hygiene: If `context_builder` misses files, append them (don't replace selection)

**Entry Point Parity:**
- If multiple entrypoints exist (CLI/API/UI/scripts), implement parity across all
- Don't downgrade requirements or docs for less-capable entrypoints

See `docs/configuration.md` for complete configuration documentation.

## Documentation Maintenance

**Update Triggers:**
- Config defaults/schemas change → update `docs/configuration.md`
- CLI flags change → update `docs/cli.md` and help text
- Task fields change → update `docs/queue-and-tasks.md`
- New features added → update relevant documentation files

**Quality Standards:**
- Keep examples in sync with source of truth
- All `docs/*.md` files should be current
- Update `AGENTS.md` when learning repo lessons or patterns

## Security & Best Practices

**Secrets:**
- Never commit real secrets (API keys, tokens, private URLs) to public repos
- Treat runner output as potentially sensitive; avoid copying raw output into `.ralph/queue.json` notes without redaction

**Testing Safety:**
- Use temp repos for runner/output tests
- Avoid mutating `.ralph/queue.json` in the main repo during tests

**First-Principles Simplicity:**
- Start from fundamentals, strip to essentials, rebuild simplest working path
- Delete before adding: remove dead code, redundant layers, stale comments
- Complexity budget: add components only when they reduce risk/maintenance or increase value
- Evidence over opinion: tests, data constraints, benchmarks settle debates
- Centralize early: consolidate similar logic into shared helpers/modules

## Operational Lessons

**Streaming Output:**
- Validate with real runner CLIs and non-trivial prompts to exercise tool usage + reasoning
- Keep streaming logs user-readable: include tool arguments (paths/commands) in summaries, not raw JSON

**Temp Directory Management:**
- CI tests use `target/tmp/ralph-ci-tmp/` (disposable, easy to clean)
- Legacy temp dirs under `/private/var/folders/` are cleaned up automatically
- Set `RALPH_CI_KEEP_TMP=1` to preserve temp directories for debugging

**Task Status Behavior:**
- Draft tasks (`status: draft`) are skipped by `run one` and `run loop` unless `--include-draft` is set
- Only `done` and `rejected` tasks should exist in `.ralph/done.json`

## Troubleshooting Common Issues

**CI Gate Failures:**
- Run `make ci` manually to see full error output
- Check for format issues: `cargo fmt --check`
- Check for linting: `cargo clippy -- -D warnings`
- Verify tests pass: `cargo test --workspace`

**Queue Lock Issues:**
- Use `--force` to bypass stale queue locks
- Check for orphaned processes holding `.ralph/lock`

**Runner Integration Issues:**
- Verify runner binaries are on `PATH`
- Test runner directly (e.g., `codex --help`)
- Check `agent.runner`, `agent.model`, and binary path overrides in config

**RepoPrompt Issues:**
- Ensure RepoPrompt is installed and configured
- Check `repoprompt_plan_required` and/or `repoprompt_tool_injection` in config (or use `--repo-prompt plan`)
- Verify selection includes relevant files before invoking `context_builder`
