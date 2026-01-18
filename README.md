# Ralph

Ralph is a tool for managing AI agent loops with a structured YAML task queue.

## Current Status (Rust rewrite)

The canonical implementation is the Rust CLI in `crates/ralph/`.

- Queue (source of truth): `.ralph/queue.yaml`
- Done archive: `.ralph/done.yaml`
- Prompt templates: built-in defaults; override in `ralph/prompts/`

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

## OpenCode Runner

Ralph supports the OpenCode CLI as a runner alongside Codex.

Quick usage:
- Ensure `opencode` is installed and on `PATH` (or set `agent.opencode_bin`).
- Use `--runner opencode` on `task build` or `scan`:
  - `cargo run -p ralph -- task build --runner opencode --model gpt-5.2 "Add tests for X"`
  - `cargo run -p ralph -- scan --runner opencode --model gpt-5.2 --focus "CI gaps"`

Defaults and config:
- `ralph run one` pulls runner/model from the task `agent` block if present, otherwise from config.
- Configure defaults in `.ralph/config.yaml` (or `~/.config/ralph/config.yaml`):

```yaml
version: 1
agent:
  runner: opencode
  model: gpt-5.2
  opencode_bin: opencode
```

Allowed models: `gpt-5.2-codex`, `gpt-5.2`, `glm-4.7`. Note: `glm-4.7` is not supported for the
Codex runner, but is valid for OpenCode.

## Configuration

Ralph uses a two-layer YAML config:
- Global: `~/.config/ralph/config.yaml`
- Project: `.ralph/config.yaml` (overrides global)

## Project Types

 Ralph supports a configurable `project_type` (`code` or `docs`) to tune prompts and workflows. This is read from config and injects a project-type-specific guidance section into all prompts (worker, scan, and task builder).

 The guidance section appears at the end of each prompt if the `{{PROJECT_TYPE_GUIDANCE}}` placeholder is not present in a custom prompt override.

 See `.ralph/README.md` for Rust runtime-file details.
