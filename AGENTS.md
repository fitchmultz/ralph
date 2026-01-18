# Contributor Guide

## Project Structure & Source of Truth
- `crates/ralph/`: **Active** Rust CLI (the Rust rewrite).
  - Run locally via `cargo run -p ralph -- <command>`
- `.ralph/`: Repo-local runtime state for the Rust CLI.
  - `.ralph/queue.yaml` is the **source of truth** for active work.
  - `.ralph/done.yaml` archives completed tasks (same schema as queue).
  - Prompt templates are embedded in the Rust CLI; repo-local overrides can be placed in `ralph/prompts/*.md`.

## Build, Test, and Development Commands (Rust)
- `cargo test -p ralph`
- `cargo run -p ralph -- queue validate`
- `cargo run -p ralph -- init`
- `cargo run -p ralph -- queue next`
- `cargo run -p ralph -- queue next-id`
- `cargo run -p ralph -- queue done`
- `cargo run -p ralph -- task build "<request>"`
- `cargo run -p ralph -- scan --focus "<focus>"`
- `cargo run -p ralph -- run one`
- `cargo run -p ralph -- run loop --max-tasks 0`

## Queue & Prompt Contract (Rust)
- Source of truth is `.ralph/queue.yaml` (YAML). Task order is priority (top runs first).
- Completed tasks must be moved to `.ralph/done.yaml` and removed from `.ralph/queue.yaml`.
- New tasks must include: `id`, `status`, `title`, `tags`, `scope`, `evidence`, `plan` (and typically `request`, `created_at`, `updated_at`).
- Prompt templates are embedded in the Rust CLI; overrides can be placed in `ralph/prompts/` and reference these files.

## Git + CI Expectations (Current Rust State)
- The execution agent owns the lifecycle: update queue status, run `make ci`, commit, and push.
- The supervisor (`ralph run`) verifies the repo is clean and will commit/push only if needed.
- Prefer commit messages like `RQ-####: <short summary>`.

## Configuration
- Two-layer YAML config:
  - Global: `~/.config/ralph/config.yaml`
  - Project: `.ralph/config.yaml` (overrides global)
- CLI flags can override at runtime; they should not be relied on as persisted config.

## Configuration & Security
- Do not commit real secrets if the repo is public.
- Treat runner output as potentially sensitive; avoid copying raw output into `.ralph/queue.yaml` notes without redaction.

## First-Principles Simplicity
- Start from the fundamentals, strip to essentials, then rebuild the simplest working path.
- Delete before adding: remove dead code, redundant layers, and stale comments; net-negative diffs are wins when behavior stays correct.
- Complexity budget: add components only when they reduce total risk/maintenance or increase measurable value.
- Evidence over opinion: tests, data constraints, and benchmarks settle debates; formatters/linters settle style.
- Centralize early: if similar logic exists, consolidate into shared helpers/modules.
