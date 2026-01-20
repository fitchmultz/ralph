# Contributor Guide

## Project Structure & Source of Truth
- `crates/ralph/`: **Active** Rust CLI.
  - Run locally via `cargo run -p ralph -- <command>`
- `.ralph/`: Repo-local runtime state.
  - `.ralph/queue.json` is the **source of truth** for active work.
  - `.ralph/done.json` archives completed tasks (same schema as queue).
  - Prompt templates are embedded in the Rust CLI; repo-local overrides can be placed in `.ralph/prompts/*.md`.

## Build, Test, and Development Commands (Rust)

**REQUIRED CI GATE**: `make ci` — **Agents MUST run this before claiming task completion, committing, or merging PRs.** This is the validation gate that must pass for any work to be considered complete.

**Development/iteration commands** (for rapid testing, not a substitute for `make ci`):
- `cargo test -p ralph`
- `cargo run -p ralph -- queue validate`
- `cargo run -p ralph -- init`
- `cargo run -p ralph -- queue next`
- `cargo run -p ralph -- queue next-id`
- `cargo run -p ralph -- queue done`
- `cargo run -p ralph -- task build "<request>"`
- `cargo run -p ralph -- scan --focus "<focus>"`
- `cargo run -p ralph -- run one`
- `cargo run -p ralph -- run one --phase 1` (generate plan only)
- `cargo run -p ralph -- run one --phase 2` (implement cached plan)
- `cargo run -p ralph -- run loop --max-tasks 0`

## Queue & Prompt Contract (Rust)
- Source of truth is `.ralph/queue.json` (JSON). Task order is priority (top runs first).
- Completed tasks must be moved to `.ralph/done.json` and removed from `.ralph/queue.json`.
- New tasks must include: `id`, `status`, `title`, `tags`, `scope`, `evidence`, `plan` (and typically `request`, `created_at`, `updated_at`).
- Prompt templates are embedded in the Rust CLI; overrides can be placed in `.ralph/prompts/` and reference these files.
- **Two-phase planning**: Agents in Phase 1 MUST output their plan wrapped in `<<RALPH_PLAN_BEGIN>>` and `<<RALPH_PLAN_END>>`.

## Git + CI Expectations (Current Rust State)
- The execution agent owns the lifecycle: update queue status, run `make ci`, commit, and push.
- The supervisor (`ralph run`) verifies the repo is clean and will commit/push only if needed.
- Prefer commit messages like `RQ-####: <short summary>`.

## CLI Help Documentation
- **When adding new CLI arguments**: Always update help text (clap `after_long_help`, doc comments) to include examples.
- **Help examples must cover**: new flags, their purpose, and typical usage patterns.
- **Verification**: Run `cargo run -p ralph -- <command> --help` to review output before committing.
- **Common gaps to watch for**: missing `--phase` examples, `--interactive` (`-i`), `--rp-on`/`--rp-off`, runner/model overrides.

## Configuration
- Two-layer JSON config:
  - Global: `~/.config/ralph/config.json`
  - Project: `.ralph/config.json` (overrides global)
- CLI flags can override at runtime; they should not be relied on as persisted config.
- Runner usage: set `agent.runner: claude` or `agent.runner: gemini` (and `agent.opencode_bin`/`agent.gemini_bin` if needed); allowed models include `gpt-5.2-codex`, `gpt-5.2`, `zai-coding-plan/glm-4.7`, `gemini-3-pro-preview`, `gemini-3-flash-preview`, `sonnet`, `opus` (Codex supports only `gpt-5.2-codex` + `gpt-5.2`; OpenCode/Gemini/Claude accept arbitrary model IDs).
- **RepoPrompt**: When `agent.require_repoprompt: true` (or `--rp-on`), agents MUST use RepoPrompt tools (`read_file`, `context_builder`, etc.).

## Configuration & Security
- Do not commit real secrets if the repo is public.
- Treat runner output as potentially sensitive; avoid copying raw output into `.ralph/queue.json` notes without redaction.

## First-Principles Simplicity
- Start from the fundamentals, strip to essentials, then rebuild the simplest working path.
- Delete before adding: remove dead code, redundant layers, and stale comments; net-negative diffs are wins when behavior stays correct.
- Complexity budget: add components only when they reduce total risk/maintenance or increase measurable value.
- Evidence over opinion: tests, data constraints, and benchmarks settle debates; formatters/linters settle style.
- Centralize early: if similar logic exists, consolidate into shared helpers/modules.
