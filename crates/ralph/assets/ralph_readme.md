<!-- RALPH_README_VERSION: 5 -->
# Ralph runtime files

This repo is using Ralph. The `.ralph/` directory holds repo-local state.

## Files

- `.ralph/queue.json` — JSON task queue (source of truth for active work).
- `.ralph/done.json` — JSON archive of completed tasks (same schema as queue); only `done`/`rejected` statuses are valid.
- `.ralph/prompts/` — optional prompt overrides (defaults are embedded in the Ralph binary).
- `.ralph/cache/` — runtime cache for plans, completions, and temporary state.

## Troubleshooting

### Duplicate Task ID Error

If `ralph queue validate` reports a duplicate task ID (e.g., `RQ-XXXX exists in both queue.json and done.json`), this usually means a new task was added without incrementing the ID. **Do not delete tasks.** Instead:

1. Run `ralph queue next-id` to get the next available ID
2. Edit `.ralph/queue.json` and change the colliding task ID to the next available one
3. Re-run `ralph queue validate` to confirm

Task IDs must be unique across both `queue.json` (active tasks) and `done.json` (completed tasks).

### Generating Multiple Task IDs

When adding multiple tasks at once, use the `--count` flag to generate all IDs in one call:

```bash
# Generate 7 sequential IDs
ralph queue next-id --count 7
```

**Important:** `next-id` does NOT reserve IDs. It simply shows the next available ID(s) based on the current queue state. Re-running the command without modifying the queue will return the same IDs. To avoid duplicates:

1. Generate all IDs you need in one call using `--count N`
2. Assign the printed IDs to your tasks in order (first ID = highest priority task)
3. Insert all tasks into `.ralph/queue.json` before running any other queue commands

## Core Commands

### Queue Management

- Validate queue:
  - `ralph queue validate`
- Bootstrap repo files (queue + done + config):
  - `ralph init`
- Inspect queue:
  - `ralph queue list`
  - `ralph queue next --with-title`
- Next task ID:
  - `ralph queue next-id`
  - `ralph queue next-id --count 7` (generate 7 sequential IDs for batch task creation)
- Show task details:
  - `ralph queue show RQ-0001`
- Archive completed tasks:
  - `ralph queue archive`
- Repair queue issues:
  - `ralph queue repair`
- Remove queue lock:
  - `ralph queue unlock`
- Sort tasks by priority:
  - `ralph queue sort`
- Search tasks:
  - `ralph queue search "authentication"`
  - `ralph queue search "TODO" --status todo`
- Queue statistics:
  - `ralph queue stats`
- Task history timeline:
  - `ralph queue history --days 14`
- Burndown chart:
  - `ralph queue burndown --days 30`
- Prune old done tasks:
  - `ralph queue prune --age 90 --keep-last 100`

### Task Creation & Management

- Build a task from a request:
  - `ralph task "Add tests for X"`
- Update task fields from repo state:
  - `ralph task update RQ-0001`
  - `ralph task update` (update all tasks)
- Edit task fields:
  - `ralph task edit title "New title" RQ-0001`
  - `ralph task edit tags "rust, cli" RQ-0001`
- Change task status:
  - `ralph task status doing RQ-0001`
- Show task details:
  - `ralph task show RQ-0001`

### PRD to Tasks

- Convert PRD markdown to task(s):
  - `ralph prd create docs/prd/feature.md`
  - `ralph prd create docs/prd/feature.md --multi` (one task per user story)
  - `ralph prd create docs/prd/feature.md --dry-run`

### Context Management

- Generate AGENTS.md from project detection:
  - `ralph context init`
  - `ralph context init --project-type rust`
- Update AGENTS.md with new learnings:
  - `ralph context update --section troubleshooting`
- Validate AGENTS.md is current:
  - `ralph context validate`

### Execution

- Launch interactive TUI:
  - `ralph tui`
  - `ralph tui --read-only`
- Run one task:
  - `ralph run one`
  - `ralph run one --phases 3` (full workflow)
  - `ralph run one --quick` (single-phase, shorthand for `--phases 1`)
  - `ralph run one --include-draft`
  - `ralph run one --update-task`
- Run multiple tasks:
  - `ralph run loop --max-tasks 0`
  - `ralph run loop --phases 2 --max-tasks 0`
  - `ralph run loop --quick --max-tasks 1`
  - `ralph run loop --include-draft --max-tasks 1`

### Environment & Diagnostics

- Verify environment readiness:
  - `ralph doctor`
- Render prompt previews:
  - `ralph prompt worker --phase 1`
  - `ralph prompt worker --phase 2 --plan-text "Plan body"`

### Scanning

- Seed tasks from a scan:
  - `ralph scan --focus "CI gaps"`
  - `ralph scan --focus "risk audit" --runner claude --model sonnet`

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

Worker prompts are composed from a base prompt plus phase-specific wrappers. All
default prompts are embedded in the Ralph binary. You can create override files
in `.ralph/prompts/` to customize behavior for this project.

Optional override locations:
- Base: `.ralph/prompts/worker.md`
- Phase wrappers: `.ralph/prompts/worker_phase1.md`, `.ralph/prompts/worker_phase2.md`,
  `.ralph/prompts/worker_phase2_handoff.md`, `.ralph/prompts/worker_phase3.md`,
  `.ralph/prompts/worker_single_phase.md`
- Shared supporting prompts: `.ralph/prompts/completion_checklist.md`,
  `.ralph/prompts/phase2_handoff_checklist.md`, `.ralph/prompts/iteration_checklist.md`,
  `.ralph/prompts/code_review.md`

If a repo-local override is missing, Ralph falls back to the embedded defaults.

### Viewing Default Prompts

To preview the composed prompts that will be sent to the runner:
- `ralph prompt worker --phase 1` — Preview the Phase 1 planning prompt
- `ralph prompt worker --phase 2` — Preview the Phase 2 implementation prompt
- `ralph prompt worker --phase 3` — Preview the Phase 3 review prompt

To view raw embedded default prompts (useful as a base for customization):
- `ralph prompt list` — List all available templates
- `ralph prompt show worker --raw` — View raw embedded default
- `ralph prompt diff worker` — Show differences between override and embedded

To export and customize prompts:
- `ralph prompt export --all` — Export all templates to `.ralph/prompts/`
- `ralph prompt export worker` — Export single template
- `ralph prompt sync --dry-run` — Preview what would change
- `ralph prompt sync` — Sync with embedded defaults (preserves your modifications)

## Runners (Codex + OpenCode + Gemini + Claude + Cursor)

Ralph can use Codex, OpenCode, Gemini, Claude, or Cursor CLI as a runner.

One-off usage:
- `ralph task --runner opencode --model gpt-5.2 "Add tests for X"`
- `ralph scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
- `ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"`
- `ralph scan --runner claude --model sonnet --focus "risk audit"`
- `ralph task --runner claude --model opus --repo-prompt plan "Add tests for X"`
- `ralph run one --phases 3` (3-phase: plan, implement+CI, review+complete)
- `ralph run one --phases 2` (2-phase: plan then implement)
- `ralph run one --quick` (single-pass execution, shorthand for `--phases 1`)

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

CLI `--repo-prompt <tools|plan|off>` (alias: `-rp`) controls both flags together:
- `tools`: tooling reminders only
- `plan`: planning requirement + tooling reminders
- `off`: disable both

Breaking change: `--rp-on/--rp-off` were removed in favor of `--repo-prompt <tools|plan|off>`.

### Three-phase Workflow (Default)

Ralph supports a 3-phase workflow by default (configured via `agent.phases: 3`):
1. **Phase 1 (Planning)**: The agent generates a detailed plan and caches it in `.ralph/cache/plans/<TASK_ID>.md`.
2. **Phase 2 (Implementation + CI)**: The agent implements the plan and must pass the configured CI gate command (default `make ci`) when enabled, then stops without completing the task. When the CI gate fails during Phase 2, Ralph automatically sends a compliance message to the agent and retries up to 2 times without user intervention.
3. **Phase 3 (Code Review + Completion)**: The agent reviews the pending diff against hardcoded standards, refines as needed, re-runs the configured CI gate command (default `make ci`) when enabled, completes the task, and (when auto git commit/push is enabled) commits and pushes.

Use `ralph run one --phases 3` for full 3-phase execution. You can also set `agent.phases` in config to control the default.

Use `--quick` as a shorthand for `--phases 1` to skip the planning phase and run single-pass execution immediately.

### Git Revert Policy

Ralph can control whether uncommitted changes are reverted when runner/supervision errors occur:
- `ask` (default): prompt on stdin (non-interactive defaults to keep changes).
- `enabled`: always revert uncommitted changes.
- `disabled`: never revert automatically.

Ralph can also toggle automatic git commit/push after successful runs:
- `agent.git_commit_push_enabled: true` (default): commit and push after completion.
- `agent.git_commit_push_enabled: false`: skip automatic commit/push (repo may remain dirty).

Examples:
- `ralph run one --git-revert-mode disabled`
- `ralph run one --git-commit-push-off`

## Security: Safeguard Dumps and Redaction

When runner operations fail (timeouts, non-zero exits, scan validation errors), Ralph writes safeguard dumps to temp directories for troubleshooting. By default, these dumps are **redacted** to prevent secrets from being written to disk.

### Redaction Behavior

- **Default (redacted)**: Secrets like API keys, bearer tokens, AWS keys, SSH keys, and hex tokens are masked with `[REDACTED]` before writing.
- **Raw dumps**: Only available with explicit opt-in (see below).

### Opt-In for Raw Dumps

Raw (non-redacted) safeguard dumps require explicit opt-in via one of:

1. **Environment variable**: `RALPH_RAW_DUMP=1`
2. **Debug mode**: `--debug` flag (implies you want verbose/raw output)

```bash
# Redacted dumps (default) - secrets are masked
ralph run one

# Raw dumps with env var - secrets written to disk
RALPH_RAW_DUMP=1 ralph run one

# Raw dumps via debug mode - secrets in debug.log and dumps
ralph run one --debug
```

### Security Considerations

- **Never commit safeguard dumps** to version control. They may contain sensitive data even when redacted.
- **Debug mode (`--debug`)** writes raw runner output to `.ralph/logs/debug.log`. This is intentional for troubleshooting but may contain secrets.
- Temp directories for safeguard dumps are created under `/tmp/ralph/` (or platform equivalent) with `ralph_` prefixes.

## Common Flags Reference

### Task Selection & Execution
- `--quick`: Shorthand for `--phases 1` (single-pass execution)
- `--include-draft`: Include draft tasks (`status: draft`) when selecting what to run
- `--update-task`: Automatically run `ralph task update` before execution
- `--visualize`: Show workflow flowchart immediately (TUI mode)

### Runner Configuration
- `--runner <codex|opencode|gemini|claude|cursor>`: Override runner
- `--model <model-id>`: Override model
- `--effort <low|medium|high|xhigh>`: Override reasoning effort (Codex only)
- `--repo-prompt <tools|plan|off>` / `-rp`: RepoPrompt mode control

### Git & CI
- `--git-revert-mode <ask|enabled|disabled>`: Control revert behavior on errors
- `--git-commit-push-on` / `--git-commit-push-off`: Toggle auto commit/push
- `--debug`: Capture raw output to `.ralph/logs/debug.log` (implies raw dumps)

### Global
- `--force`: Force operations (bypass locks, overwrite files)
- `-v`, `--verbose`: Increase output verbosity
