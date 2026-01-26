# CLI Reference

Purpose: Summarize Ralph commands, flags, and customization points with examples for common workflows.

## Global Flags
- `--force`: force operations (bypass stale queue locks; bypass clean-repo safety checks for commands that enforce them, e.g. `run one`, `run loop`, and `scan`).
- `-v`, `--verbose`: increase output verbosity.

Examples:
```bash
ralph --verbose queue list
ralph --force queue archive
```

## Core Commands

* `ralph init`: bootstrap `.ralph/queue.json`, `.ralph/done.json`, and `.ralph/config.json`.
* `ralph queue <subcommand>`: inspect, search, validate, and maintain `.ralph/queue.json` + `.ralph/done.json`.
* `ralph run <subcommand>`: run tasks via a runner (codex/opencode/gemini/claude).
* `ralph tui`: launch the interactive UI (queue + execution + loop).
* `ralph prompt <subcommand>`: render compiled prompts for inspection.
* `ralph task`: create a task from a request.
* `ralph scan`: generate new tasks via scanning.
* `ralph doctor`: verify environment readiness.

## `ralph tui`

Launch the interactive TUI. This is the primary user-facing entry point.

Behavior:

* Execution is enabled by default (press Enter to run selected task).
* Use `--read-only` to disable execution.
* Loop mode is available inside the TUI (press `l` to toggle).
* Archive done/rejected tasks inside the TUI (press `a`, then confirm).
* Use `:` to open the command palette for discoverability.
* The footer shows status messages and errors as actions run.

Keybindings (task list unless noted otherwise):

* Help overlay: `?` or `h` opens help, `Esc` (or `?`/`h`) closes.
* Navigation
  * `Up`/`Down` or `j`/`k`: move selection
  * `Enter`: run selected task
* Actions
  * `l`: toggle loop mode
  * `a`: archive done/rejected tasks (confirmation)
  * `d`: delete selected task (confirmation)
  * `e`: edit task fields
  * `n`: create a new task
  * `c`: edit project config
  * `g`: scan repository
  * `r`: reload queue from disk
  * `q` (or `Esc` from the task list): quit (prompts if a task is running)
* Filters & Search
  * `/`: search tasks
  * `t`: filter by tags
  * `f`: cycle status filter
  * `x`: clear filters
* Quick Changes
  * `s`: cycle task status
  * `p`: cycle priority
* Command Palette
  * `:`: open palette (type to filter, `Enter` to run, `Esc` to cancel)
* Execution View
  * `Esc`: return to task list
  * `Up`/`Down` or `j`/`k`: scroll logs
  * `PgUp`/`PgDn`: page logs
  * `a`: toggle auto-scroll
  * `l`: stop loop mode

Examples:

```bash
ralph tui
ralph tui --read-only
ralph tui --runner codex --model gpt-5.2-codex --effort high
```

## `ralph run`

### Subcommands

* `one`: run exactly one task (optionally by ID or via interactive TUI).
* `loop`: run tasks until none remain (or `--max-tasks` reached).

Run iterations are controlled by config and task settings:
- `agent.iterations` (config) or `task.agent.iterations` (per task) repeat the selected phases.
- `agent.followup_reasoning_effort` or `task.agent.followup_reasoning_effort` applies to follow-up iterations.
- `task.agent.model_effort` overrides `agent.reasoning_effort` for Codex tasks (`default` defers to config).

### Interactive flags

* `ralph run one -i` launches the same TUI as `ralph tui`.
* `ralph run loop -i` launches the same TUI and auto-starts loop mode.

### Draft tasks

By default, draft tasks (`status: draft`) are skipped during task selection (so they won't be auto-selected for execution).

* `--include-draft`: Include draft tasks (`status: draft`) when selecting what to run.

### Pre-run task update

* `--update-task`: Automatically run `ralph task update <TASK_ID>` once per task immediately before the supervisor marks the task as `doing` and starts execution. This updates task fields (scope, evidence, plan, notes, tags, depends_on) based on current repository state, priming agents with better task information. This runs only once per task, before the first iteration (not before subsequent iterations if `iterations > 1`). Can also be enabled via config: `agent.update_task_before_run: true`.
* `--no-update-task`: Disable automatic pre-run task update (overrides config).

Examples:

```bash
ralph run one
ralph run one --include-draft
ralph run one -i
ralph run one --update-task
ralph run loop --max-tasks 0
ralph run loop --include-draft --max-tasks 1
ralph run loop --update-task --max-tasks 1
ralph run loop -i --max-tasks 3
ralph run loop --max-tasks 1 --debug
ralph run one --git-commit-push-off
```

Clean-repo checks for `run one` and `run loop` allow changes to `.ralph/config.json`
(alongside `.ralph/queue.json` and `.ralph/done.json`). Use `--force` to bypass the
clean-repo check entirely if needed.

## `ralph scan`

Generate new tasks by scanning the repository.

Key flags:

* `--focus <TEXT>`: Optional focus prompt to guide the scan.
* `--runner <codex|opencode|gemini|claude|cursor>`, `--model <model-id>`, `--effort <low|medium|high|xhigh>`: Override runner/model/effort for this invocation.
* `--rp-on` / `--rp-off`: Force RepoPrompt tooling reminders on/off for this invocation.

Clean-repo checks for `scan` allow changes to `.ralph/queue.json` and `.ralph/done.json`
only (unlike `run`, changes to `.ralph/config.json` are *not* allowed). Use `--force` to
bypass the clean-repo check (and stale queue locks) entirely if needed.

Examples:

```bash
ralph scan
ralph scan --focus "production readiness gaps"
ralph scan --runner opencode --model gpt-5.2 --focus "CI and safety gaps"
ralph scan --force --focus "scan even with uncommitted changes"
```

## `ralph queue`

Inspect and manage the task queue (`.ralph/queue.json`) and done archive (`.ralph/done.json`).

### Subcommands

* `validate`: validate the active queue (and done archive if present).
* `prune`: prune tasks from `.ralph/done.json` based on age/status/keep-last rules.
* `next`: print the next todo task (ID by default).
* `next-id`: print the next available task ID (across queue + done archive).
* `show`: show a task by ID.
* `list`: list tasks in queue order.
* `search`: search tasks by content (title, evidence, plan, notes, request, tags, scope, custom fields).
* `archive`: move completed tasks from queue.json to done.json.
* `repair`: repair the queue and done files (fix missing fields, duplicates, timestamps).
* `unlock`: remove the queue lock file.
* `sort`: sort tasks by priority (reorders the queue file).
* `stats`: show task statistics (completion rate, avg duration, tag breakdown).
* `history`: show task history timeline (creation/completion events by day).
* `burndown`: show burndown chart of remaining tasks over time.
* `schema`: print the JSON schema for the queue file.

### Queue Flags

Common flag families across `ralph queue` subcommands:

* Status filters (`list`, `search`):
  * `--status <draft|todo|doing|done|rejected>` (repeatable)
* Tag filters (`list`, `search`, `stats`):
  * `--tag <TAG>` (repeatable; case-insensitive)
* Scope filters (`list`, `search`):
  * `--scope <TOKEN>` (repeatable; substring match; case-insensitive)
* Done archive selection (`list`, `search`):
  * `--include-done`: include tasks from `.ralph/done.json` after active queue output
  * `--only-done`: only use `.ralph/done.json` (ignore active queue)
  * `--include-done` and `--only-done` are mutually exclusive.
* Output format:
  * `list`, `search`: `--format <compact|long>` (default: `compact`)
  * `show`: `--format <json|compact>` (default: `json`)
  * `stats`, `history`, `burndown`: `--format <text|json>` (default: `text`)
* Limits (`list`, `search`):
  * `--limit <N>` (default: 50; `0` = no limit)
  * `--all`: ignore `--limit`
* Sorting:
  * `list`: `--sort-by priority` and `--order <ascending|descending>` (sorts output only)
  * `sort`: `--sort-by priority` and `--order <ascending|descending>` (reorders queue file)

### `ralph queue validate`

Validate `.ralph/queue.json` (and `.ralph/done.json` if present).

```bash
ralph queue validate
```

### `ralph queue prune`

Prune removes old tasks from `.ralph/done.json` while preserving recent history.

Safety:

* `--keep-last` always protects N most recently completed tasks (by `completed_at`).
* If no filters are provided, all tasks are pruned except those protected by `--keep-last`.
* Missing or invalid `completed_at` timestamps are treated as oldest for keep-last ordering
  but do NOT match the age filter (safety-first).

Flags:

* `--age <DAYS>`: only prune tasks completed at least N days ago.
* `--status <draft|todo|doing|done|rejected>`: filter by status (repeatable).
* `--keep-last <N>`: keep N most recently completed tasks regardless of filters.
* `--dry-run`: show what would be pruned without writing to disk.

```bash
ralph queue prune --dry-run --age 30 --status rejected
ralph queue prune --keep-last 100
ralph queue prune --age 90
ralph queue prune --age 30 --status done --keep-last 50
```

### `ralph queue next`

Print the next runnable task (ID by default). If no runnable task exists, prints the next available ID.

Flags:

* `--with-title`: include task title after ID.

```bash
ralph queue next
ralph queue next --with-title
```

### `ralph queue next-id`

Print the next available task ID (across queue + done archive).

```bash
ralph queue next-id
```

### `ralph queue show`

Show a task by ID.

Flags:

* `--format <json|compact>`: output format (default: `json`).

```bash
ralph queue show RQ-0001
ralph queue show RQ-0001 --format compact
```

### `ralph queue list`

List tasks in queue order.

Flags:

* `--status <draft|todo|doing|done|rejected>`: filter by status (repeatable).
* `--tag <TAG>`: filter by tag (repeatable, case-insensitive).
* `--scope <TOKEN>`: filter by scope token (repeatable, case-insensitive; substring match).
* `--filter-deps <TASK_ID>`: filter by tasks that depend on the given task ID (recursively).
* `--include-done`: include tasks from `.ralph/done.json` after active queue output.
* `--only-done`: only list tasks from `.ralph/done.json` (ignores active queue).
* `--format <compact|long>`: output format (default: `compact`).
* `--limit <N>`: maximum tasks to show (default: 50; `0` = no limit).
* `--all`: show all tasks (ignores `--limit`).
* `--sort-by <priority>`: sort output by field.
* `--order <ascending|descending>`: sort order (default: `descending`).

```bash
ralph queue list
ralph queue list --status todo --tag rust
ralph queue list --status doing --scope crates/ralph
ralph queue list --include-done --limit 20
ralph queue list --only-done --all
ralph queue list --filter-deps=RQ-0100
```

### `ralph queue search`

Search tasks by content (title, evidence, plan, notes, request, tags, scope, custom fields).

Flags:

* `--regex`: interpret query as a regular expression.
* `--match-case`: case-sensitive search (default: case-insensitive).
* `--status <draft|todo|doing|done|rejected>`: filter by status (repeatable).
* `--tag <TAG>`: filter by tag (repeatable, case-insensitive).
* `--scope <TOKEN>`: filter by scope token (repeatable, case-insensitive; substring match).
* `--include-done`: include tasks from `.ralph/done.json` in search.
* `--only-done`: only search tasks in `.ralph/done.json` (ignores active queue).
* `--format <compact|long>`: output format (default: `compact`).
* `--limit <N>`: maximum results to show (default: 50; `0` = no limit).
* `--all`: show all results (ignores `--limit`).

```bash
ralph queue search "authentication"
ralph queue search "RQ-\d{4}" --regex
ralph queue search "TODO" --match-case
ralph queue search "fix" --status todo --tag rust
ralph queue search "refactor" --scope crates/ralph --tag rust
```

### `ralph queue archive`

Move completed tasks from `.ralph/queue.json` to `.ralph/done.json`.

```bash
ralph queue archive
```

### `ralph queue repair`

Repair the queue and done files (fix missing fields, duplicates, timestamps).

Flags:

* `--dry-run`: show what would be changed without writing to disk.

```bash
ralph queue repair
ralph queue repair --dry-run
```

### `ralph queue unlock`

Remove the queue lock file/directory.

```bash
ralph queue unlock
```

### `ralph queue sort`

Sort tasks by priority (reorders the queue file).

Flags:

* `--sort-by <priority>`: sort by field (default: `priority`).
* `--order <ascending|descending>`: sort order (default: `descending`, highest priority first).

```bash
ralph queue sort
ralph queue sort --order descending
ralph queue sort --order ascending
```

### `ralph queue stats`

Queue reports default to human-readable text but can emit JSON for scripting.

Summarize completion rates, durations, and tag breakdowns.

Flags:

* `--tag <TAG>`: filter by tag (repeatable, case-insensitive).
* `--format <text|json>`: output format (default: `text`).

```bash
ralph queue stats
ralph queue stats --tag rust --tag cli
ralph queue stats --format json
```

### `ralph queue history`

Show creation/completion events by day.

Flags:

* `--days <N>`: number of days to show (default: 7).
* `--format <text|json>`: output format (default: `text`).

```bash
ralph queue history
ralph queue history --days 14
ralph queue history --format json
```

### `ralph queue burndown`

Render remaining-task counts over time.

Flags:

* `--days <N>`: number of days to show (default: 7).
* `--format <text|json>`: output format (default: `text`).

```bash
ralph queue burndown
ralph queue burndown --days 30
ralph queue burndown --format json
```

### `ralph queue schema`

Print the JSON schema for the queue file.

```bash
ralph queue schema
```

## `ralph task`

Create tasks and edit task fields from CLI.

Common subcommands:
- `ralph task <request>`: create a task from a freeform request.
- `ralph task status <draft|todo|doing|done|rejected> <TASK_ID>`: update status.
- `ralph task edit <FIELD> <VALUE> <TASK_ID>`: edit any task field (default + custom).
- `ralph task field <KEY> <VALUE> <TASK_ID>`: set one custom field.
- `ralph task update [TASK_ID]`: refresh task fields based on current repo state (omit `TASK_ID` to update all tasks).

Field formats (for `ralph task edit`):
- Lists (`tags`, `scope`, `evidence`, `plan`, `notes`, `depends_on`): comma/newline-separated.
- `custom_fields`: `key=value` pairs, comma/newline-separated.
- Optional text (`request`, `completed_at`): pass `""` to clear.
- Required timestamps (`created_at`, `updated_at`) must be RFC3339 strings and should not be cleared.

Examples:

```bash
ralph task "Add CLI task edit command"
ralph task status doing RQ-0001
ralph task edit title "Update queue edit docs" RQ-0001
ralph task edit tags "cli, rust" RQ-0001
ralph task edit custom_fields "severity=high, owner=ralph" RQ-0001
ralph task edit request "" RQ-0001
```

### ralph task update

Update existing task fields based on current repository state.

This command inspects the repo and refreshes task fields like scope, evidence, plan, notes, tags, and depends_on to reflect current code reality. Useful for keeping tasks synchronized with an evolving codebase. Omit `TASK_ID` to refresh every task in the active queue.

Common use cases:
- Refresh task scope after code refactoring or file moves
- Update evidence after implementation changes
- Adjust plan after project structure changes
- Clean up dependencies after tasks are completed/archived

Fields that can be updated (all refreshed by default):
- `scope` - file paths and/or commands relevant to the task
- `evidence` - observations about task context
- `plan` - sequential implementation steps
- `notes` - additional notes or observations
- `tags` - task categorization tags
- `depends_on` - dependency task IDs

Fields preserved (not changed):
- `id`, `title`, `status`, `priority`, `created_at`, `request`, `agent`, `completed_at`, `custom_fields`

Flags:
- `--fields <FIELD_NAMES>` - specific fields to update (comma-separated, default: all)
- `--runner/--model/--effort` - runner override for this invocation
- `--rp-on`/`--rp-off` - force RepoPrompt requirement

Examples:
```bash
ralph task update
ralph task update RQ-0001
ralph task update --fields scope,evidence,plan RQ-0001
ralph task update --runner opencode --model gpt-5.2 RQ-0001
ralph task update --fields tags RQ-0042
```

## `ralph prompt`

Render prompt previews to inspect the exact text sent to runners.

Key flags:
- `ralph prompt worker --phase <1|2|3>`: choose phase prompt.
- `--iterations` / `--iteration-index`: simulate follow-up iteration context.
- `--plan-text` / `--plan-file`: embed phase 2 plan text for previewing.
- `--rp-on` / `--rp-off`: force RepoPrompt requirement.

Examples:

```bash
ralph prompt worker --phase 1 --rp-on
ralph prompt worker --phase 2 --plan-text "Plan body"
ralph prompt worker --phase 2 --iteration-index 2 --iterations 3
ralph prompt worker --phase 3 --task-id RQ-0001
```

## Runner and Model Overrides

These flags are supported on `task`, `scan`, `run one`, `run loop`, and `tui`:

* `--runner <codex|opencode|gemini|claude|cursor>`
* `--model <model-id>`
* `--effort <low|medium|high|xhigh>` (codex only)
* `--rp-on` / `--rp-off`

Examples:

```bash
ralph tui --runner claude --model opus
ralph run one --runner codex --model gpt-5.2-codex --effort high
```

## Run-Specific Flags

The `run one` and `run loop` commands also support:

* `--include-draft`: Include draft tasks (`status: draft`) when selecting what to run.
* `--update-task`: Automatically run `ralph task update <TASK_ID>` once per task immediately before the supervisor marks the task as `doing` and starts execution. This updates task fields (scope, evidence, plan, notes, tags, depends_on) based on current repository state, priming agents with better task information. Runs only once per task, before the first iteration (not before subsequent iterations if `iterations > 1`). Can also be enabled via config: `agent.update_task_before_run: true`.
* `--no-update-task`: Disable automatic pre-run task update for this invocation (overrides config).
* `--git-revert-mode <ask|enabled|disabled>`
* `--git-commit-push-on` / `--git-commit-push-off`
* `--debug` (capture raw supervisor + runner output to `.ralph/logs/debug.log`)

Examples:

```bash
ralph run one --include-draft
ralph run one --update-task
ralph run one --no-update-task
ralph run one --git-revert-mode disabled
ralph run one --git-commit-push-off
ralph run loop --include-draft --max-tasks 1
ralph run loop --update-task --max-tasks 1
ralph run loop --max-tasks 1 --debug
```

## Help Output

For the full, authoritative list of flags and examples, run:

```bash
ralph --help
ralph tui --help
ralph queue --help
ralph run --help
```
