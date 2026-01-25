# CLI Reference

Purpose: Summarize Ralph commands, flags, and customization points with examples for common workflows.

## Global Flags
- `--force`: force operations (e.g., bypass stale queue locks).
- `-v`, `--verbose`: increase output verbosity.

Examples:
```bash
ralph --verbose queue list
ralph --force queue archive
```

## Core Commands

* `ralph init`: bootstrap `.ralph/queue.json`, `.ralph/done.json`, and `.ralph/config.json`.
* `ralph queue <subcommand>`: validate, list, search, and batch-maintain tasks.
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

Examples:

```bash
ralph run one
ralph run one -i
ralph run loop --max-tasks 0
ralph run loop -i --max-tasks 3
ralph run one --git-commit-push-off
```

## `ralph task`

Create tasks and edit task fields from CLI.

Common subcommands:
- `ralph task <request>`: create a task from a freeform request.
- `ralph task status <draft|todo|doing|done|rejected> <TASK_ID>`: update status.
- `ralph task edit <FIELD> <VALUE> <TASK_ID>`: edit any task field (default + custom).
- `ralph task field <KEY> <VALUE> <TASK_ID>`: set one custom field.
- `ralph task update <TASK_ID>`: refresh task fields based on current repo state.

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

This command inspects the repo and refreshes task fields like scope, evidence, plan, notes, tags, and depends_on to reflect current code reality. Useful for keeping tasks synchronized with an evolving codebase.

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

* `--runner <codex|opencode|gemini|claude>`
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

* `--git-revert-mode <ask|enabled|disabled>`
* `--git-commit-push-on` / `--git-commit-push-off`

Examples:

```bash
ralph run one --git-revert-mode disabled
ralph run one --git-commit-push-off
```

## Help Output

For the full, authoritative list of flags and examples, run:

```bash
ralph --help
ralph tui --help
ralph queue --help
ralph run --help
```
