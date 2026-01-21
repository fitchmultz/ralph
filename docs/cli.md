# CLI Reference

Purpose: Summarize Ralph commands, flags, and customization points with examples for common workflows.

## Global Flags
- `--force`: force operations (e.g., bypass stale queue locks).
- `-v`, `--verbose`: increase output verbosity.

Examples:
```bash
ralph --verbose queue list
ralph --force queue done
```

## Core Commands
- `ralph init`: bootstrap `.ralph/queue.json`, `.ralph/done.json`, and `.ralph/config.json`.
- `ralph queue <subcommand>`: validate, list, search, and update tasks.
- `ralph run <subcommand>`: run tasks via a runner (codex/opencode/gemini/claude).
- `ralph task`: create a task from a request (default subcommand; `ralph task build` still works).
- `ralph task ready`: promote a draft task to todo.
- `ralph task done`: mark a task done/rejected and archive it.
- `ralph scan`: generate new tasks via scanning.
- `ralph prompt <subcommand>`: render compiled prompts.
- `ralph config <subcommand>`: inspect config and paths.
- `ralph doctor`: verify environment readiness.

## Runner and Model Overrides
These flags are supported on `task`, `scan`, `run one`, and `run loop` (see each section for full usage):
- `--runner <codex|opencode|gemini|claude>`
- `--model <model-id>`
- `--effort <minimal|low|medium|high>` (codex only)
- `--rp-on` / `--rp-off` (force RepoPrompt requirement on or off)

Examples:
```bash
ralph task --runner opencode --model gpt-5.2 "Add tests for X"
ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"
ralph run one --runner codex --model gpt-5.2-codex --effort high
```

## `ralph queue`
### Subcommands
- `validate`: validate queue + done archive.
- `prune`: prune old tasks from the done archive.
- `next`: print next todo task ID (optionally with title).
- `next-id`: print next available task ID.
- `show`: show a task by ID.
- `list`: list tasks with filtering and sorting.
- `search`: search task content with filters.
- `done`: move completed tasks (done/rejected) from queue to archive.
- `repair`: repair queue/done files.
- `unlock`: remove queue lock.
- `set-status`: update a task status in the active queue.
- `set-field`: set a custom field on a task.
- `sort`: reorder queue by priority (or another field).
- `stats`: show queue statistics.
- `history`: show task history timeline.
- `burndown`: show remaining-task burndown.
- `schema`: print the queue JSON schema.

Note: `ralph queue complete` has been removed. Use `ralph task done <TASK_ID> <done|rejected>` for single-task completion.

### Common Flags and Arguments
- `next`: `--with-title`
- `show`: `TASK_ID` and `--format <json|compact>`
- `list`:
  - `--status <draft|todo|doing|done|rejected>` (repeatable)
  - `--tag <tag>` (repeatable)
  - `--scope <token>` (repeatable)
  - `--filter-deps <TASK_ID>`
  - `--include-done` / `--only-done`
  - `--format <compact|long>`
  - `--limit <N>` / `--all`
  - `--sort-by <field>` / `--descending`
- `search`:
  - `QUERY`
  - `--regex` / `--match-case`
  - `--status <draft|todo|doing|done|rejected>` (repeatable)
  - `--tag <tag>` (repeatable)
  - `--scope <token>` (repeatable)
  - `--include-done` / `--only-done`
  - `--format <compact|long>`
  - `--limit <N>` / `--all`
- `set-status`: `TASK_ID` `STATUS` and optional `--note "..."`
- `set-field`: `TASK_ID` `KEY` `VALUE`
- `sort`: `--sort-by <field>` and `--descending`
- `stats`: `--tag <tag>` (repeatable)
- `history`: `--days <N>`
- `burndown`: `--days <N>`
- `prune`:
  - `--age <days>`
  - `--status <draft|todo|doing|done|rejected>` (repeatable)
  - `--keep-last <N>`
  - `--dry-run`
- `repair`: `--dry-run`

Examples:
```bash
ralph queue list --status todo --tag rust
ralph queue list --status draft
ralph queue list --include-done --limit 20
ralph queue list --filter-deps RQ-0100
ralph queue search "RQ-\\d{4}" --regex
ralph queue show RQ-0001 --format compact
ralph queue next --with-title
ralph queue set-status RQ-0002 doing --note "Starting work"
ralph queue set-field RQ-0003 severity high
ralph queue prune --age 30 --status done --keep-last 50
ralph queue repair --dry-run
```

## `ralph run`
### Subcommands
- `one`: run exactly one task (optionally by ID or via interactive TUI).
- `loop`: run tasks until none remain (or `--max-tasks` reached).

### Flags (run one / run loop)
- `--runner <codex|opencode|gemini|claude>`
- `--model <model-id>`
- `--effort <minimal|low|medium|high>` (codex only)
- `--phases <1|2|3>`
- `--rp-on` / `--rp-off`
- `--git-revert-mode <ask|enabled|disabled>`
- `--include-draft`
- `-i`, `--interactive`
- `run one` only: `--id <TASK_ID>` (non-interactive only)
- `run loop` only: `--max-tasks <N>`

Examples:
```bash
ralph run one
ralph run one --id RQ-0001
ralph run one -i
ralph run one --phases 3
ralph run one --include-draft
ralph run loop --max-tasks 0
ralph run loop --include-draft --max-tasks 1
ralph run loop --git-revert-mode disabled --max-tasks 1
```

## `ralph task`
### Flags
- `REQUEST` (positional; if omitted, reads from stdin)
- `--tags <tag1,tag2>`
- `--scope <path-or-token>`
- `--runner <codex|opencode|gemini|claude>`
- `--model <model-id>`
- `--effort <minimal|low|medium|high>` (codex only)
- `--rp-on` / `--rp-off`

Examples:
```bash
ralph task "Add integration tests"
ralph task --tags cli,rust --scope crates/ralph "Fix queue parsing"
ralph task ready RQ-0005
ralph task done RQ-0001 done --note "Finished work"
echo "Triage flaky CI" | ralph task --runner codex --model gpt-5.2-codex --effort medium
ralph task build "Explicit build subcommand still works"
```

## `ralph scan`
### Flags
- `--focus "..."`
- `--runner <codex|opencode|gemini|claude>`
- `--model <model-id>`
- `--effort <minimal|low|medium|high>` (codex only)
- `--rp-on` / `--rp-off`

Examples:
```bash
ralph scan --focus "production readiness gaps"
ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"
```

## `ralph prompt`
### Subcommands and Flags
- `worker`:
  - `--single`
  - `--phase <1|2|3>`
  - `--task-id <TASK_ID>`
  - `--plan-file <path>`
  - `--plan-text "..."`
  - `--rp-on` / `--rp-off`
  - `--explain`
- `scan`:
  - `--focus "..."`
  - `--rp-on` / `--rp-off`
  - `--explain`
- `task-builder`:
  - `--request "..."`
  - `--tags "..."`
  - `--scope "..."`
  - `--rp-on` / `--rp-off`
  - `--explain`

Examples:
```bash
ralph prompt worker --phase 1 --rp-on
ralph prompt worker --single --explain
ralph prompt scan --focus "risk audit" --rp-off
ralph prompt task-builder --request "Add tests" --tags rust --scope crates/ralph
```

## `ralph config`
### Subcommands
- `show`: print resolved config JSON.
- `paths`: print queue/done/config paths.
- `schema`: print the config JSON schema.

Examples:
```bash
ralph config show
ralph config paths
ralph config schema
```

## `ralph init`
### Flags
- `--force`: overwrite existing files.

Behavior:
- Creates `.ralph` directory and files if missing.
- Validates existing `config.json`, `queue.json`, and `done.json` if they exist (fails if invalid unless `--force` is used).
- Checks/creates `README.md` if referenced by prompts.

Example:
```bash
ralph init --force
```

## `ralph doctor`
No flags.

Example:
```bash
ralph doctor
```

## Help Output
For the full, authoritative list of flags and examples, run:
```bash
ralph --help
ralph queue --help
ralph run --help
```
