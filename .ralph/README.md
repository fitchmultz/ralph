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
  - `ralph run loop --phases 2 --max-tasks 0` (two-pass, default)
  - `ralph run loop --phases 1 --max-tasks 1` (single-pass)

## Template Variables

Prompt templates support variable interpolation for environment variables and config values:

### Environment Variables
- `${VAR}` — expand environment variable (leaves literal if not set)
- `${VAR:-default}` — expand with default value if not set
- Example: `API endpoint: ${API_URL:-https://api.example.com}`

### Config Values
- `{{config.section.key}}` — expand from config (supports nested paths)
- Supported paths:
  - `{{config.agent.runner}}` — current runner (e.g., `Claude`)
  - `{{config.agent.model}}` — current model (e.g., `gpt-5.2-codex`)
  - `{{config.queue.id_prefix}}` — task ID prefix (e.g., `RQ`)
  - `{{config.queue.id_width}}` — task ID width (e.g., `4`)
  - `{{config.project_type}}` — project type (e.g., `Code`)
- Example: `Using {{config.agent.model}} via {{config.agent.runner}}`

### Escaping
- `$${VAR}` — escaped, outputs literal `${VAR}`
- `\${VAR}` — escaped, outputs literal `${VAR}`

Note: Standard placeholders like `{{USER_REQUEST}}` are still processed after variable expansion.

## Runners (Codex + OpenCode + Gemini + Claude)

Ralph can use Codex, OpenCode, Gemini, or Claude CLI as a runner.

One-off usage:
- `ralph task build --runner opencode --model gpt-5.2 "Add tests for X"`
- `ralph scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
- `ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"`
- `ralph scan --runner claude --model sonnet --focus "risk audit"`
- `ralph task build --runner claude --model opus --rp-on "Add tests for X"`
- `ralph run one --phases 2` (two-pass: plan then implement, default)
- `ralph run one --phases 1` (single-pass execution)

Defaults via config (`.ralph/config.json` or `~/.config/ralph/config.json`):

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
Ralph can explicitly require the usage of RepoPrompt tools. When enabled via config (`require_repoprompt: true`) or CLI (`--rp-on`), Ralph will:
1. Instruct the agent to use RepoPrompt tools for exploration.
2. During planning, require the agent to use the `context_builder` tool to gather context AND generate the plan in a single step.

### Two-phase Planning
When enabled (`two_pass_plan: true`, default: true), Ralph orchestrates execution in two phases for all runners:
1. **Phase 1 (Planning)**: The agent generates a detailed plan and caches it in `.ralph/cache/plans/<TASK_ID>.md`.
2. **Phase 2 (Implementation)**: The agent implements the cached plan.

Use `ralph run one --phases 2` to run both phases sequentially (default), or `ralph run one --phases 1` for single-pass execution.
