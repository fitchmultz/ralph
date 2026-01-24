# Contributor Guide

Purpose: Capture repo-wide operating expectations for contributors and agents.

## Project Structure & Source of Truth
- `crates/ralph/`: **Active** Rust CLI.
  - Run locally via `cargo run -p ralph -- <command>`
- `.ralph/`: Repo-local runtime state.
  - `.ralph/queue.json` is the **source of truth** for active work.
  - When creating tasks via `ralph task`, new tasks are inserted at the top **unless** the first task is `doing`, in which case new tasks are inserted just below it (position 1).
  - `.ralph/done.json` archives completed tasks (same schema as queue).
  - Prompt templates are embedded in the Rust CLI and organized under `crates/ralph/assets/prompts/`; repo-local overrides can be placed in `.ralph/prompts/*.md`.

## Build, Test, and Development Commands (Rust)

**REQUIRED CI GATE**: `make ci` — **Agents MUST run this before claiming task completion, committing, or merging PRs.** This is the validation gate that must pass for any work to be considered complete.

**CLI Commands**: See `docs/cli.md` for complete command reference. Agents should keep this documentation up to date when adding or modifying CLI commands.

**Development/iteration commands** (for rapid testing, not a substitute for `make ci`):
- `cargo test -p ralph`
- `cargo run -p ralph -- <command>` (see `docs/cli.md` for available commands)

## Queue & Prompt Contract (Rust)
- Source of truth is `.ralph/queue.json` (JSON). Task order follows file order (top runs first).
- Completed tasks must be moved to `.ralph/done.json` and removed from `.ralph/queue.json`.
- New tasks must include: `id`, `title`, `created_at`, `updated_at`, `tags`, `scope`, `evidence`, `plan`, `notes`, `depends_on`, `custom_fields` (arrays/objects can be empty; only `id` and `title` must be non-empty).
- Optional task fields: `status` (defaults to `todo`), `priority` (defaults to `medium`), `request`, `completed_at`, `agent`.
- See `docs/queue-and-tasks.md` for complete task schema documentation.
- Prompt templates are embedded in the Rust CLI and organized under `crates/ralph/assets/prompts/`; overrides can be placed in `.ralph/prompts/` and reference these files.
- Worker prompts are composed from a base prompt (`worker.md`) plus phase-specific wrappers (`worker_phase1.md`, `worker_phase2.md`, `worker_phase2_handoff.md`, `worker_phase3.md`, `worker_single_phase.md`).
- **Two-phase planning**: Agents in Phase 1 MUST write their plan to `.ralph/cache/plans/<TASK_ID>.md` and avoid printing the plan inline.
- **Supervision-aware completion**: `ralph task done` detects supervision and writes a completion signal to `.ralph/cache/completions/<TASK_ID>.json`. The supervisor consumes the signal, runs `queue::complete_task`, and then `post_run_supervise` (for done tasks) to finish CI/commit/push. This prevents lock contention while still recording the agent's completion intent.

## Git + CI Expectations (Current Rust State)
- The execution agent owns the lifecycle: update queue status, run `make ci`, commit, and push.
- The supervisor (`ralph run`) verifies the repo is clean and will commit/push only if needed.
- Prefer commit messages like `RQ-####: <short summary>`.

## CLI Help Documentation
- **When adding new CLI arguments**: Always update help text (clap `after_long_help`, doc comments) to include examples.
- **Help examples must cover**: new flags, their purpose, and typical usage patterns.
- **Verification**: Run `cargo run -p ralph -- <command> --help` to review output before committing.
- **Common gaps to watch for**: missing `--phases` examples, `--interactive` (`-i`), `--rp-on`/`--rp-off`, runner/model overrides.

## Configuration
- See `docs/configuration.md` for complete configuration documentation. Agents should keep this documentation up to date when adding or modifying configuration options.
- Two-layer JSON config:
  - Global: `~/.config/ralph/config.json`
  - Project: `.ralph/config.json` (overrides global)
- CLI flags can override at runtime; they should not be relied on as persisted config.
- **RepoPrompt**: When `agent.require_repoprompt: true` (or `--rp-on`), agents MUST use RepoPrompt tools (`read_file`, `context_builder`, etc.).
- **RepoPrompt responsibility**: RepoPrompt produces the plan, but the agent owns correctness. If the plan conflicts with repo reality, fix it before writing the plan to `.ralph/cache/plans/<TASK_ID>.md`.
- **RepoPrompt preflight**: Before invoking `context_builder`, perform a quick repo reality check (validate task assumptions + identify relevant files) and include those findings in the `context_builder` instructions.
- **Selection hygiene**: If `context_builder` misses key files, append them to the selection (do NOT replace selection) and ask a follow-up before finalizing the plan.
- **Entry-point parity**: If multiple user-facing entrypoints exist (CLI/API/UI/scripts), implement parity rather than downgrading requirements or docs.

## Documentation Maintenance
- When config defaults, schemas, CLI flags, or task fields change, update `docs/` and keep examples in sync with the source of truth.
- All documentation in `docs/` should be kept up to date. When adding or modifying features, update the relevant documentation files accordingly.

## Operational Lessons
- When changing streamed runner output, validate with real runner CLIs and a non-trivial prompt so tool usage + reasoning events are exercised.
- Keep streaming logs user-readable by including tool arguments (paths/commands) in summaries, not raw JSON.
- Use temp repos for runner/output tests and avoid mutating `.ralph/queue.json` in this repo.
- Draft tasks (`status: draft`) are skipped by `run one` and `run loop` unless `--include-draft` is set.

## Configuration & Security
- Do not commit real secrets if the repo is public.
- Treat runner output as potentially sensitive; avoid copying raw output into `.ralph/queue.json` notes without redaction.

## First-Principles Simplicity
- Start from the fundamentals, strip to essentials, then rebuild the simplest working path.
- Delete before adding: remove dead code, redundant layers, and stale comments; net-negative diffs are wins when behavior stays correct.
- Complexity budget: add components only when they reduce total risk/maintenance or increase measurable value.
- Evidence over opinion: tests, data constraints, and benchmarks settle debates; formatters/linters settle style.
- Centralize early: if similar logic exists, consolidate into shared helpers/modules.
- Dead-code linting: prefer an explicit, minimal usage pattern (e.g., a type annotation) over suppressing with allow attributes when preserving a public API.
