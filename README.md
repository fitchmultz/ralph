# Ralph

Purpose: Describe Ralph's CLI, defaults, and workflow entry points for contributors.

Ralph is a tool for managing AI agent loops with a structured JSON task queue.

## Status

The Ralph CLI is in `crates/ralph/`.

- Queue (source of truth): `.ralph/queue.json`
- Done archive: `.ralph/done.json`
- Prompt templates: built-in defaults; override in `.ralph/prompts/`
- **Production Verification:** See `.ralph/README.md`.

## Documentation

Start with `docs/index.md` for configuration, queue/task schema, CLI usage, workflow, and environment variables.

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
  - `cargo run -p ralph -- task "<request>"`
- Seed the backlog with a scan:
  - `cargo run -p ralph -- scan --focus "<focus>"`
- Execute the next task (first `todo` task in queue order):
  - `cargo run -p ralph -- run one`
- Archive completed tasks:
  - `cargo run -p ralph -- queue archive`

## Prompt Overrides

Ralph embeds default prompts in the Rust binary. To override them for a repo, add files here:

- `.ralph/prompts/worker.md`
- `.ralph/prompts/task_builder.md`
- `.ralph/prompts/scan.md`

If a file is missing, Ralph falls back to the embedded default. Any override must keep required
placeholders (for example `{{USER_REQUEST}}` in the task builder prompt).

## Runners (Codex + OpenCode + Gemini + Claude + Cursor)

Ralph supports Codex, OpenCode, Gemini, Claude, and Cursor CLIs as runners.

Quick usage:
- Ensure runner binaries are installed and on `PATH`.
- Use `--runner <kind>` on `task`, `scan`, or `run`:
  - `cargo run -p ralph -- task --runner opencode --model gpt-5.2 "Add tests for X"`
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
    "phases": 3,
    "gemini_bin": "gemini"
  }
}
```

**Allowed models by runner:**
- **Codex**: `gpt-5.2-codex`, `gpt-5.2` (only these two)
- **OpenCode**: arbitrary model IDs (e.g., `zai-coding-plan/glm-4.7`)
- **Gemini**: `gemini-3-pro-preview`, `gemini-3-flash-preview`, or arbitrary IDs
- **Claude**: `sonnet` (default), `opus`, or arbitrary model IDs

### RepoPrompt Integration
Ralph can independently require RepoPrompt planning and tooling reminders. Configure `repoprompt_plan_required` to inject the Phase 1 planning instructions, and `repoprompt_tool_injection` to inject RepoPrompt tooling reminders in prompts. CLI `--repo-prompt <tools|plan|off>` (alias: `-rp`) controls both flags together. Breaking change: `--rp-on/--rp-off` were removed in favor of `--repo-prompt`.

### Three-phase Workflow (Default)
Ralph supports a 3-phase workflow by default:
1. **Phase 1 (Planning)**: The agent generates a detailed plan and caches it in `.ralph/cache/plans/<TASK_ID>.md`.
2. **Phase 2 (Implementation + CI)**: The agent implements the plan and must pass `make ci`, then stops without completing the task.
3. **Phase 3 (Code Review + Completion)**: The agent reviews the pending diff against hardcoded standards, refines as needed, re-runs `make ci`, completes the task, and (when auto git commit/push is enabled) commits and pushes.

Use `ralph run one --phases 3` for full 3-phase execution (default). Use `--phases 2` for plan+implement, or `--phases 1` for single-pass execution. You can also set `agent.phases` in config to control the default.

## Configuration

Ralph uses a two-layer JSON config:
- Global: `~/.config/ralph/config.json`
- Project: `.ralph/config.json` (overrides global)

## Project Types

 Ralph supports a configurable `project_type` (`code` or `docs`) to tune prompts and workflows. This is read from config and injects a project-type-specific guidance section into all prompts (worker, scan, and task builder).

 The guidance section appears at the end of each prompt if the `{{PROJECT_TYPE_GUIDANCE}}` placeholder is not present in a custom prompt override.

 See `.ralph/README.md` for Rust runtime-file details.
