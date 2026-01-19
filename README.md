# Ralph

Ralph is a tool for managing AI agent loops with a structured YAML task queue.

## Current Status (Rust rewrite)

The canonical implementation is the Rust CLI in `crates/ralph/`.

- Queue (source of truth): `.ralph/queue.yaml`
- Done archive: `.ralph/done.yaml`
- Prompt templates: built-in defaults; override in `ralph/prompts/`
- **Production Verification:** See `.ralph/README.md`.

## Quick Start (Rust)

- Install the `ralph` binary to `~/.local/bin`:
  - `make install`
- Run tests:
  - `cargo test --workspace`
- Validate queue:
  - `cargo run -p ralph -- queue validate`
- Inspect queue:
  - `cargo run -p ralph -- queue list`
- Add a task from a request:
  - `cargo run -p ralph -- task build "<request>"`
- Seed the backlog with a scan:
  - `cargo run -p ralph -- scan --focus "<focus>"`
- Execute the next task (first `todo` task in queue order):
  - `cargo run -p ralph -- run one`
- Archive completed tasks:
  - `cargo run -p ralph -- queue done`

## Prompt Overrides

Ralph embeds default prompts in the Rust binary. To override them for a repo, add files here:

- `ralph/prompts/worker.md`
- `ralph/prompts/task_builder.md`
- `ralph/prompts/scan.md`

If a file is missing, Ralph falls back to the embedded default. Any override must keep required
placeholders (for example `{{USER_REQUEST}}` in the task builder prompt).

## Runners (OpenCode + Gemini)

Ralph supports the OpenCode and Gemini CLIs as runners alongside Codex.

Quick usage:
- Ensure `opencode` is installed and on `PATH` (or set `agent.opencode_bin`).
- Ensure `gemini` is installed and on `PATH` (or set `agent.gemini_bin`).
- Use `--runner opencode` on `task build` or `scan`:
  - `cargo run -p ralph -- task build --runner opencode --model gpt-5.2 "Add tests for X"`
  - `cargo run -p ralph -- scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
- Use `--runner gemini`:
  - `cargo run -p ralph -- scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"`

Defaults and config:
- `ralph run one` pulls runner/model from the task `agent` block if present, otherwise from config.
- Configure defaults in `.ralph/config.yaml` (or `~/.config/ralph/config.yaml`):

```yaml
version: 1
agent:
  runner: opencode
  model: gpt-5.2
  opencode_bin: opencode
  gemini_bin: gemini
```

Allowed models: `gpt-5.2-codex`, `gpt-5.2`, `zai-coding-plan/glm-4.7`, `gemini-3-pro-preview`, `gemini-3-flash-preview`. Note: Codex
supports only `gpt-5.2-codex` and `gpt-5.2`; OpenCode/Gemini accept arbitrary model IDs.

Gemini runner prepends a RepoPrompt tooling instruction at the top of every prompt.

## Configuration

Ralph uses a two-layer YAML config:
- Global: `~/.config/ralph/config.yaml`
- Project: `.ralph/config.yaml` (overrides global)

## Project Types

 Ralph supports a configurable `project_type` (`code` or `docs`) to tune prompts and workflows. This is read from config and injects a project-type-specific guidance section into all prompts (worker, scan, and task builder).

 The guidance section appears at the end of each prompt if the `{{PROJECT_TYPE_GUIDANCE}}` placeholder is not present in a custom prompt override.

 See `.ralph/README.md` for Rust runtime-file details.
