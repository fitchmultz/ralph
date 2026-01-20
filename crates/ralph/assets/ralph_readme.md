# Ralph runtime files

This repo is using Ralph. The `.ralph/` directory holds repo-local state.

## Files

- `.ralph/queue.json` — JSON task queue (source of truth for active work).
- `.ralph/done.json` — JSON archive of completed tasks (same schema as queue).
- `.ralph/prompts/` — optional prompt overrides (defaults are embedded in the Rust CLI).

## Minimal Rust Commands

- Validate queue:
  - `ralph queue validate`
- Bootstrap repo files (queue + done + config):
  - `ralph init`
- Inspect queue:
  - `ralph queue list`
  - `ralph queue next --with-title`
- Next task ID:
  - `ralph queue next-id`
- Archive completed tasks:
  - `ralph queue done`
- Build a task from a request:
  - `ralph task build "<request>"`
- Seed tasks from a scan:
  - `ralph scan --focus "<focus>"`
- Run one task:
  - `ralph run one`
- Run multiple tasks:
  - `ralph run loop --max-tasks 0`

## Runners (OpenCode + Gemini + Claude)

Ralph can use the OpenCode, Gemini, or Claude CLI as a runner.

One-off usage:
- `ralph task build --runner opencode --model gpt-5.2 "Add tests for X"`
- `ralph scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
- `ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"`
- `ralph scan --runner claude --model sonnet --focus "risk audit"`
- `ralph task build --runner claude --model opus "Add tests for X"`

Defaults via config (`.ralph/config.json` or `~/.config/ralph/config.json`):

```json
{
  "version": 1,
  "agent": {
    "runner": "opencode",
    "model": "gpt-5.2",
    "opencode_bin": "opencode",
    "gemini_bin": "gemini",
    "claude_bin": "claude",
    "two_pass_plan": true
  }
}
```

**Allowed models by runner:**
- **Codex**: `gpt-5.2-codex`, `gpt-5.2` (only these two)
- **OpenCode**: arbitrary model IDs (e.g., `zai-coding-plan/glm-4.7`)
- **Gemini**: `gemini-3-pro-preview`, `gemini-3-flash-preview`, or arbitrary IDs
- **Claude**: `sonnet` (default), `opus`, or arbitrary model IDs

**Two-pass plan mode**: When enabled (`two_pass_plan: true`), Claude first generates a plan in plan mode, then implements it with auto-approval. This provides better structure and visibility into planned changes. If plan generation fails, falls back to direct implementation. Currently supported for Claude runner only; will expand to OpenCode in the future.
