# Ralph runtime files

This repo is using Ralph. The `.ralph/` directory holds repo-local state.

## Files

- `.ralph/queue.json` — JSON task queue (source of truth for active work).
- `.ralph/done.json` — JSON archive of completed tasks (same schema as queue); only `done`/`rejected` statuses are valid.
- `.ralph/prompts/` — optional prompt overrides (defaults are embedded in the Rust CLI under `crates/ralph/assets/prompts/`).

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
  - `ralph task "<request>"`
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

## Prompt Organization

Worker prompts are composed from a base prompt plus phase-specific wrappers:
- Base: `.ralph/prompts/worker.md`
- Phase wrappers: `.ralph/prompts/worker_phase1.md`, `.ralph/prompts/worker_phase2.md`,
  `.ralph/prompts/worker_phase2_handoff.md`, `.ralph/prompts/worker_phase3.md`,
  `.ralph/prompts/worker_single_phase.md`
- Shared supporting prompts: `.ralph/prompts/completion_checklist.md`,
  `.ralph/prompts/phase2_handoff_checklist.md`, `.ralph/prompts/iteration_checklist.md`,
  `.ralph/prompts/code_review.md`

If a repo-local override is missing, Ralph falls back to the embedded defaults in
`crates/ralph/assets/prompts/`.

## Runners (Codex + OpenCode + Gemini + Claude)

Ralph can use Codex, OpenCode, Gemini, or Claude CLI as a runner.

One-off usage:
- `ralph task --runner opencode --model gpt-5.2 "Add tests for X"`
- `ralph scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
- `ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"`
- `ralph scan --runner claude --model sonnet --focus "risk audit"`
- `ralph task --runner claude --model opus --rp-on "Add tests for X"`
- `ralph run one --phases 3` (3-phase: plan, implement+CI, review+complete, default)
- `ralph run one --phases 2` (2-phase: plan then implement, default)
- `ralph run one --phases 1` (single-pass execution)


Defaults via config (`.ralph/config.json` or `~/.config/ralph/config.json`):

```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "iterations": 1,
    "repoprompt_plan_required": false,
    "repoprompt_tool_injection": false,
    "git_revert_mode": "ask",
    "git_commit_push_enabled": true,
    "ci_gate_command": "make ci",
    "ci_gate_enabled": true
  }
}
```

**Allowed models by runner:**
- **Codex**: `gpt-5.2-codex`, `gpt-5.2` (only these two)
- **OpenCode**: arbitrary model IDs (e.g., `zai-coding-plan/glm-4.7`)
- **Gemini**: `gemini-3-pro-preview`, `gemini-3-flash-preview`, or arbitrary IDs
- **Claude**: `sonnet` (default), `opus`, or arbitrary model IDs

### RepoPrompt Integration
Ralph can independently control RepoPrompt planning and tooling reminders:
1. `repoprompt_plan_required`: injects the Phase 1 planning instructions, including the `context_builder` requirement.
2. `repoprompt_tool_injection`: injects RepoPrompt tooling reminders into prompts.

CLI `--rp-on/--rp-off` toggles both flags.

### Three-phase Workflow (Default)
Ralph supports a 3-phase workflow by default (configured via `agent.phases: 3`):
1. **Phase 1 (Planning)**: The agent generates a detailed plan and caches it in `.ralph/cache/plans/<TASK_ID>.md`.
2. **Phase 2 (Implementation + CI)**: The agent implements the plan and must pass the configured CI gate command (default `make ci`) when enabled, then stops without completing the task. When the CI gate fails during Phase 2, Ralph automatically sends a strict compliance message to the agent and retries up to 2 times without user intervention. After 2 automatic retries, the user is prompted to decide how to proceed (keep changes, revert, or continue with custom message).
3. **Phase 3 (Code Review + Completion)**: The agent reviews the pending diff against hardcoded standards, refines as needed, re-runs the configured CI gate command (default `make ci`) when enabled, completes the task, and (when auto git commit/push is enabled) commits and pushes.

Use `ralph run one --phases 3` for full 3-phase execution. You can also set `agent.phases` in config to control the default.

### Git Revert Policy
Ralph can control whether uncommitted changes are reverted when runner/supervision errors occur:
- `ask` (default): prompt on stdin (non-interactive defaults to keep changes).
- `enabled`: always revert uncommitted changes.
- `disabled`: never revert automatically.

Ralph can also toggle automatic git commit/push after successful runs:
- `agent.git_commit_push_enabled: true` (default): commit and push after completion.
- `agent.git_commit_push_enabled: false`: skip automatic commit/push (repo may remain dirty).

Example:
- `ralph run one --git-revert-mode disabled`
