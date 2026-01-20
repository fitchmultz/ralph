# Ralph

Ralph is a tool for managing AI agent loops with a structured YAML task queue.

## Status

The Ralph CLI is in `crates/ralph/`.

- Queue (source of truth): `.ralph/queue.json`
- Done archive: `.ralph/done.json`
- Prompt templates: built-in defaults; override in `.ralph/prompts/`
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

- `.ralph/prompts/worker.md`
- `.ralph/prompts/task_builder.md`
- `.ralph/prompts/scan.md`

If a file is missing, Ralph falls back to the embedded default. Any override must keep required
placeholders (for example `{{USER_REQUEST}}` in the task builder prompt).

## Runners (Codex + OpenCode + Gemini + Claude)

Ralph supports Codex, OpenCode, Gemini, and Claude CLIs as runners.

Quick usage:
- Ensure runner binaries are installed and on `PATH`.
- Use `--runner <kind>` on `task build`, `scan`, or `run`:
  - `cargo run -p ralph -- task build --runner opencode --model gpt-5.2 "Add tests for X"`
  - `cargo run -p ralph -- scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
  - `cargo run -p ralph -- run one --runner claude --model opus`

Defaults and config:
- `ralph run one` pulls runner/model from the task `agent` block if present, otherwise from config.
- Configure defaults in `.ralph/config.json` (or `~/.config/ralph/config.json`):

```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "two_pass_plan": true,
    "require_repoprompt": false
  }
}
```

**Allowed models by runner:**
- **Codex**: `gpt-5.2-codex`, `gpt-5.2` (only these two)
- **OpenCode**: arbitrary model IDs (e.g., `zai-coding-plan/glm-4.7`)
- **Gemini**: `gemini-3-pro-preview`, `gemini-3-flash-preview`, or arbitrary IDs
- **Claude**: `sonnet` (default), `opus`, or arbitrary model IDs

### RepoPrompt Integration
Ralph can explicitly require RepoPrompt usage. When enabled via config (`require_repoprompt: true`) or CLI (`--rp-on`), Ralph instructs the agent to use RepoPrompt tools for exploration and planning.

### Two-phase Planning
When enabled (`two_pass_plan: true`, default: true), Ralph orchestrates execution in two phases for ALL runners:
1. **Phase 1 (Planning)**: The agent generates a detailed plan and caches it in `.ralph/cache/plans/<TASK_ID>.md`.
2. **Phase 2 (Implementation)**: The agent implements the cached plan.

Use `ralph run one --phases 2` to run both phases sequentially (default), or `ralph run one --phases 1` for single-pass execution.

## Configuration

Ralph uses a two-layer JSON config:
- Global: `~/.config/ralph/config.json`
- Project: `.ralph/config.json` (overrides global)

## Project Types

 Ralph supports a configurable `project_type` (`code` or `docs`) to tune prompts and workflows. This is read from config and injects a project-type-specific guidance section into all prompts (worker, scan, and task builder).

 The guidance section appears at the end of each prompt if the `{{PROJECT_TYPE_GUIDANCE}}` placeholder is not present in a custom prompt override.

 See `.ralph/README.md` for Rust runtime-file details.
