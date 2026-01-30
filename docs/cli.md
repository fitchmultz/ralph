# CLI Reference

Purpose: Summarize Ralph commands, flags, and customization points with examples for common workflows.

## Platform Requirements

Ralph is developed and tested on **Unix-like systems** (Linux, macOS). Windows support is limited:

- **Fully supported**: Linux, macOS
- **Limited on Windows**:
  - Process group control (Ctrl-C handling, timeout interrupts) is Unix-only
  - PID liveness detection for stale lock detection is Unix-only
  - Directory sync operations are no-ops on non-Unix platforms
  - The Makefile requires a Unix environment (WSL recommended for Windows users)
  - `ralph queue unlock` error messages reference Unix commands (`rm -rf`)

For Windows users, we recommend using WSL2 for full functionality.

## Global Flags
- `--force`: force operations (bypass stale queue locks; bypass clean-repo safety checks for commands that enforce them, e.g. `run one`, `run loop`, and `scan`).
- `-v`, `--verbose`: increase output verbosity.
- `--color <auto|always|never>`: control color output (default: `auto`).
- `--no-color`: disable colored output (alias for `--color never`).

Color output is automatically enabled when stdout is a TTY and disabled when piped or redirected. The `NO_COLOR` environment variable is also respected.

Examples:
```bash
ralph --verbose queue list
ralph --force queue archive
ralph --color never queue list
ralph --no-color queue list
NO_COLOR=1 ralph queue list
```

## Core Commands

* `ralph init`: bootstrap `.ralph/queue.json`, `.ralph/done.json`, and `.ralph/config.json` with optional interactive wizard.
* `ralph context <subcommand>`: manage project context (AGENTS.md) for AI agents.
* `ralph queue <subcommand>`: inspect, search, validate, and maintain `.ralph/queue.json` + `.ralph/done.json`.
* `ralph run <subcommand>`: run tasks via a runner (codex/opencode/gemini/claude/cursor/kimi/pi).
* `ralph tui`: launch the interactive UI (queue + execution + loop).
* `ralph prompt <subcommand>`: render compiled prompts for inspection.
* `ralph task`: create a task from a request.
* `ralph prd <subcommand>`: convert PRD (Product Requirements Document) markdown to tasks.
* `ralph scan`: generate new tasks via scanning.
* `ralph doctor`: verify environment readiness.
* `ralph completions <shell>`: generate shell completion scripts.
* `ralph version`: display version information.

## `ralph completions`

Generate shell completion scripts for bash, zsh, fish, PowerShell, and Elvish.

The completion script is written to stdout. Redirect to the appropriate location for your shell.

### Supported Shells

* `bash` - Bash shell completions
* `zsh` - Zsh shell completions
* `fish` - Fish shell completions
* `powershell` - PowerShell completions
* `elvish` - Elvish shell completions

### Installation Examples

**Bash:**
```bash
# Generate and install
ralph completions bash > ~/.local/share/bash-completion/completions/ralph

# Or system-wide (may require sudo)
ralph completions bash > /etc/bash_completion.d/ralph
```

**Zsh:**
```bash
# Generate and install
ralph completions zsh > ~/.zfunc/_ralph

# Add to ~/.zshrc if not already present:
fpath+=~/.zfunc
```

**Fish:**
```bash
# Generate and install
ralph completions fish > ~/.config/fish/completions/ralph.fish
```

**PowerShell:**
```powershell
# Generate and install to current user's profile
ralph completions powershell > $PROFILE.CurrentUserAllHosts

# Or view the completions without installing
ralph completions powershell
```

**Elvish:**
```bash
# Generate and install
ralph completions elvish > ~/.local/share/elvish/lib/ralph.elv
```

### Usage

Once installed, completions work automatically:

```bash
# Type 'ralph ' then press Tab to see subcommands
ralph <TAB>
# queue   run   task   scan   tui   ...

# Type 'ralph queue ' then press Tab to see queue subcommands
ralph queue <TAB>
# list   show   validate   archive   ...

# Flags also complete
ralph queue list --<TAB>
# --all   --format   --limit   --status   ...
```

### Generating Without Installing

To preview the completion script without installing:

```bash
ralph completions bash
ralph completions zsh
ralph completions fish
ralph completions powershell
ralph completions elvish
```

## `ralph version`

Display the Ralph CLI version information.

### Flags

* `--verbose` (`-v`): Show extended build information including git commit and build date.

### Examples

```bash
# Show version
ralph version

# Show extended version info
ralph version --verbose
```

## `ralph init`

Bootstrap Ralph files in the current repository with an optional interactive onboarding wizard.

### Interactive Wizard

When running in a TTY (or with `--interactive`), the wizard guides new users through:

1. **Runner Selection**: Choose from Claude, Codex, OpenCode, Gemini, or Cursor
2. **Model Selection**: Pick the appropriate model for your chosen runner
3. **Workflow Mode**: Select 1-phase (quick), 2-phase (standard), or 3-phase (full) workflow
4. **First Task**: Optionally create your first task with title, description, and priority

The wizard explains each option and generates a properly configured `.ralph/config.json` and `.ralph/queue.json`.

### Flags

* `--force`: Overwrite existing files if they already exist.
* `--interactive` (`-i`): Force interactive wizard mode (even if not a TTY).
* `--non-interactive`: Skip interactive prompts even if running in a TTY (use defaults).

### Workflow Modes

* **3-phase (Full)**: Plan → Implement + CI → Review + Complete [Recommended]
* **2-phase (Standard)**: Plan → Implement (faster, less review)
* **1-phase (Quick)**: Single-pass execution (simple fixes only)

### Examples

```bash
# Auto-detect TTY and run wizard if interactive
ralph init

# Force wizard mode
ralph init --interactive

# Skip wizard, use defaults (good for CI/scripts)
ralph init --non-interactive

# Overwrite existing files with wizard
ralph init --force --interactive
```

## `ralph context`

Manage project context (AGENTS.md) for AI agents. This command generates and maintains an `AGENTS.md` file that documents project conventions, build commands, testing guidelines, and workflow contracts for AI agents working on the codebase.

### Subcommands

* `init`: Generate initial AGENTS.md from project detection.
* `update`: Update AGENTS.md with new learnings.
* `validate`: Validate AGENTS.md is up to date with project structure.

### `ralph context init`

Generate an initial AGENTS.md file based on detected project type (Rust, Python, TypeScript, Go, or Generic).

Flags:

* `--force`: Overwrite existing AGENTS.md if it already exists.
* `--project-type <rust|python|typescript|go|generic>`: Override auto-detection with a specific project type.
* `--output <PATH>`: Output path for AGENTS.md (default: `AGENTS.md` in repo root).
* `--interactive`: Interactive mode to guide through context creation (not yet implemented).

Examples:

```bash
ralph context init
ralph context init --force
ralph context init --project-type rust
ralph context init --project-type python --output docs/AGENTS.md
```

### `ralph context update`

Update AGENTS.md with new learnings. This appends content to specific sections without regenerating the entire file.

Flags:

* `--section <NAME>`: Section to update (can be specified multiple times).
* `--file <PATH>`: File containing new learnings to append.
* `--interactive`: Interactive mode to select sections and input learnings (not yet implemented).
* `--dry-run`: Preview changes without writing to disk.
* `--output <PATH>`: Output path (default: existing AGENTS.md location).

Examples:

```bash
ralph context update --section troubleshooting
ralph context update --section troubleshooting --section git-hygiene
ralph context update --file new_learnings.md
ralph context update --section troubleshooting --dry-run
```

### `ralph context validate`

Validate that AGENTS.md exists and contains required sections.

Flags:

* `--strict`: Fail if any recommended sections are missing (not just required sections).
* `--path <PATH>`: Path to AGENTS.md (default: auto-discover in repo root).

Examples:

```bash
ralph context validate
ralph context validate --strict
ralph context validate --path docs/AGENTS.md
```

### AGENTS.md Structure

The generated AGENTS.md includes these sections:

* **Non-Negotiables**: Rules that must always be followed (CI gate, documentation requirements, testing).
* **Repository Map**: Overview of the codebase structure.
* **Build, Test, and CI**: Commands for building, testing, and running CI.
* **Language Conventions**: Language-specific conventions (Rust/Python/TypeScript/Go).
* **Testing**: Testing guidelines and patterns.
* **Workflow Contracts**: How tasks are tracked and executed.
* **Configuration**: Config precedence and key settings.
* **Git Hygiene**: Commit message format and git practices.
* **Documentation Maintenance**: When to update documentation.
* **Troubleshooting**: Common issues and solutions.

## `ralph doctor`

Verify environment readiness by checking Git, queue, runner binaries, and project configuration.

### Runner Binary Detection

`ralph doctor` detects runner binaries by trying multiple common flags in order:
1. `--version`
2. `-V`
3. `--help`
4. `help`

The check passes if any of these invocations succeed. This allows `ralph doctor` to work with runner CLIs that may not support `--version` but do support `--help` or `-V`.

### Actionable Error Guidance

When a runner binary check fails, `ralph doctor` provides actionable guidance:

```
FAIL: runner binary 'codex' (Codex) check failed: tried: --version, -V, --help, help

To fix this issue:
  1. Install the runner binary, or
  2. Configure a custom path in .ralph/config.json:
     {
       "agent": {
         "codex_bin": "/path/to/codex"
       }
     }
  3. Run 'ralph doctor' to verify the fix
```

Config keys for each runner:
- Codex: `codex_bin`
- OpenCode: `opencode_bin`
- Gemini: `gemini_bin`
- Claude: `claude_bin`
- Cursor: `cursor_bin`
- Kimi: `kimi_bin`
- Pi: `pi_bin`

Examples:

```bash
ralph doctor
ralph --verbose doctor
```

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
  * `p`: toggle progress panel visibility
  * `f`: toggle flowchart overlay

### TUI Execution View Flowchart Overlay

When running a task in the TUI, press `f` to open the workflow flowchart overlay. This provides a visual representation of the current position in the 3-phase workflow:

* **Phase visualization**: Shows the workflow topology with connected phase nodes
  * `>` (yellow): Currently active phase
  * `+` (green): Completed phase
  * `o` (gray): Pending phase
* **Phase descriptions**: Brief explanation of what each phase does
* **Phase timing**: Elapsed time per phase (if started)

The flowchart adapts to the configured workflow:
* **1-phase**: Shows "Single Phase" (Execute task)
* **2-phase**: Shows Planning → Implementation
* **3-phase** (default): Shows Planning → Implementation → Review

Press `f`, `Esc`, `h`, or `?` to close the flowchart overlay.

Use `--visualize` flag with `ralph run one -i` or `ralph run loop -i` to show the flowchart immediately on TUI start:

```bash
ralph run one -i --visualize
ralph run loop -i --visualize
```

### TUI Execution View Progress Panel

When running a task in the TUI, the execution view displays a progress panel showing:

* **Phase indicators**: Visual indicators for each phase (Planning → Implementation → Review)
  * `▶` (yellow): Currently active phase
  * `✓` (green): Completed phase
  * `○` (gray): Pending phase
* **Phase timing**: Elapsed time per phase in MM:SS format
* **Total execution time**: Overall duration since task start

The progress panel automatically appears when a task starts and adapts to the configured workflow:
* **1-phase**: Shows "Single Phase" indicator
* **2-phase**: Shows Planning → Implementation
* **3-phase** (default): Shows Planning → Implementation → Review

Press `p` in the execution view to toggle the progress panel visibility. This is useful when you need more screen space for log output.

Phase transitions are detected automatically from runner output (e.g., "# IMPLEMENTATION MODE" header).

Examples:

```bash
ralph tui
ralph tui --read-only
ralph tui --runner codex --model gpt-5.2-codex --effort high
```

### Terminal Compatibility

The TUI automatically detects terminal capabilities and adjusts rendering accordingly:

* **Color support**: Auto-detected from `TERM`, `COLORTERM`, and `TERM_PROGRAM` environment variables. Supports truecolor, 256-color, 16-color, and monochrome modes.
* **Mouse support**: Enabled by default on terminals that support it. Use `--no-mouse` to disable mouse capture for terminals with broken mouse support.
* **Unicode support**: Uses Unicode box-drawing characters by default. Use `--ascii-borders` for ASCII-only terminals.

Terminal compatibility flags:

* `--no-mouse`: Disable mouse capture (useful for terminals with broken mouse support or when running over SSH).
* `--color <auto|always|never>`: Control color output. `auto` detects terminal capabilities (default), `always` forces colors, `never` disables colors. Also respects the `NO_COLOR` environment variable.
* `--ascii-borders`: Use ASCII characters (`+`, `-`, `|`) for borders instead of Unicode box-drawing characters.

Examples:

```bash
# Disable mouse capture
ralph tui --no-mouse

# Force colors even in pipes or non-TTY environments
ralph tui --color always

# Disable colors entirely
ralph tui --color never

# Use ASCII borders for older terminals
ralph tui --ascii-borders

# Combine options for maximum compatibility
ralph tui --no-mouse --color never --ascii-borders
```

### Tested Terminals

The TUI has been tested on the following terminal applications:

| Terminal | Color | Mouse | Unicode | Notes |
|----------|-------|-------|---------|-------|
| iTerm2 (macOS) | Full | Yes | Yes | Primary development target |
| Terminal.app (macOS) | Full | Yes | Yes | Default macOS terminal |
| Windows Terminal | Full | Yes | Yes | Modern Windows terminal |
| GNOME Terminal | Full | Yes | Yes | Common Linux terminal |
| Konsole | Full | Yes | Yes | KDE terminal |
| Alacritty | Full | Yes | Yes | GPU-accelerated terminal |
| WezTerm | Full | Yes | Yes | Modern terminal emulator |
| tmux | Full | Yes | Yes | Terminal multiplexer |
| screen | 16-color | Basic | Yes | Legacy multiplexer |
| VS Code terminal | Full | Yes | Yes | Embedded terminal |

If you encounter issues with a specific terminal, try the compatibility flags above.

## `ralph run`

### Subcommands

* `one`: run exactly one task (optionally by ID or via interactive TUI).
* `loop`: run tasks until none remain (or `--max-tasks` reached).

Run iterations are controlled by config and task settings:
- `agent.iterations` (config) or `task.agent.iterations` (per task) repeat the selected phases.
- `agent.followup_reasoning_effort` or `task.agent.followup_reasoning_effort` applies to follow-up iterations.
- `task.agent.model_effort` overrides `agent.reasoning_effort` for Codex tasks (`default` defers to config).

### Phases

Use `--phases <1|2|3>` (or `agent.phases` in config) to control execution shape:
- `1`: single-pass execution (no mandated planning step).
- `2`: plan -> implement.
- `3`: plan -> implement+CI -> review+complete.

Use `--quick` as a shorthand for `--phases 1` to skip the planning phase and run single-pass execution immediately.

### Interactive flags

* `ralph run one -i` launches the same TUI as `ralph tui`.
* `ralph run loop -i` launches the same TUI and auto-starts loop mode.

### Workflow visualization

* `--visualize`: Show the workflow flowchart overlay immediately on TUI start (interactive mode only). Useful for understanding the current position in the 3-phase workflow before execution begins.

### Draft tasks

By default, draft tasks (`status: draft`) are skipped during task selection (so they won't be auto-selected for execution).

* `--include-draft`: Include draft tasks (`status: draft`) when selecting what to run.

### Pre-run task update

* `--update-task`: Automatically run `ralph task update <TASK_ID>` once per task immediately before the supervisor marks the task as `doing` and starts execution. This updates task fields (scope, evidence, plan, notes, tags, depends_on) based on current repository state, priming agents with better task information. This runs only once per task, before the first iteration (not before subsequent iterations if `iterations > 1`). Can also be enabled via config: `agent.update_task_before_run: true`.
* `--no-update-task`: Disable automatic pre-run task update (overrides config).

### Normalized runner CLI options

These flags configure a normalized runner CLI behavior surface across Codex/OpenCode/Gemini/Claude/Cursor/Kimi/Pi. Unsupported options are dropped by default with a warning (see `--unsupported-option-policy`).

* `--approval-mode <default|auto-edits|yolo|safe>`: approval/permission behavior (default: `yolo`).
* `--sandbox <default|enabled|disabled>`: runner sandbox behavior when supported.
* `--verbosity <quiet|normal|verbose>`: runner verbosity when supported.
* `--plan-mode <default|enabled|disabled>`: Cursor plan/read-only mode control.
* `--output-format <stream-json|json|text>`: execution requires `stream-json`.
* `--unsupported-option-policy <ignore|warn|error>`: handling for unsupported options (default: `warn`).

Examples:

```bash
ralph run one
ralph run one --phases 3
ralph run one --phases 2
ralph run one --phases 1
ralph run one --quick
ralph run one --include-draft
ralph run one -i
ralph run one -i --visualize
ralph run one --update-task
ralph run loop --max-tasks 0
ralph run loop --phases 3 --max-tasks 0
ralph run loop --quick --max-tasks 1
ralph run loop --include-draft --max-tasks 1
ralph run loop --update-task --max-tasks 1
ralph run loop --repo-prompt tools --max-tasks 1
ralph run loop --repo-prompt off --max-tasks 1
ralph run loop -i --max-tasks 3
ralph run loop -i --visualize --max-tasks 1
ralph run loop --max-tasks 1 --debug
ralph run one --git-commit-push-off
ralph run one --approval-mode yolo --sandbox disabled
ralph run one --approval-mode auto-edits --runner claude
```

Clean-repo checks for `run one` and `run loop` allow changes to `.ralph/config.json`
(alongside `.ralph/queue.json` and `.ralph/done.json`). Use `--force` to bypass the
clean-repo check entirely if needed.

## `ralph scan`

Generate new tasks by scanning the repository.

Key flags:

* `[PROMPT]`: Optional focus prompt as a positional argument (alternative to `--focus`).
* `--focus <TEXT>`: Optional focus prompt to guide the scan (backward compatible).
* `--mode <maintenance|innovation>` (alias: `-m`): Scan mode determining the focus of the repository scan.
  * `maintenance` (default): Find bugs, workflow gaps, design flaws, repo rules violations. Focus on code hygiene and break-fix maintenance.
  * `innovation`: Find feature gaps, use-case completeness issues, enhancement opportunities. Focus on new features and strategic additions.
* `--runner <codex|opencode|gemini|claude|cursor|kimi|pi>`, `--model <model-id>`, `--effort <low|medium|high|xhigh>` (alias: `-e`): Override runner/model/effort for this invocation.
* `--repo-prompt <tools|plan|off>` (alias: `-rp`): `tools` = tooling reminders only, `plan` = planning requirement + tooling reminders, `off` = disable both.
* Runner CLI overrides: `--approval-mode <default|auto-edits|yolo|safe>`, `--sandbox <default|enabled|disabled>`, `--verbosity <quiet|normal|verbose>`, `--plan-mode <default|enabled|disabled>`, `--output-format <stream-json|json|text>`, `--unsupported-option-policy <ignore|warn|error>`.

Clean-repo checks for `scan` allow changes to `.ralph/queue.json` and `.ralph/done.json`
only (unlike `run`, changes to `.ralph/config.json` are *not* allowed). Use `--force` to
bypass the clean-repo check (and stale queue locks) entirely if needed.

Examples:

```bash
ralph scan
ralph scan "production readiness gaps"                              # Positional prompt
ralph scan --focus "production readiness gaps"                        # Flag-based prompt
ralph scan --mode maintenance "security audit"                        # Maintenance mode (default)
ralph scan --mode innovation "feature gaps for CLI"                   # Innovation mode
ralph scan -m innovation "enhancement opportunities"                  # Short flag for mode
ralph scan --runner opencode --model gpt-5.2 "CI and safety gaps"     # With runner overrides
ralph scan --force "scan even with uncommitted changes"
ralph scan --approval-mode auto-edits --runner claude "auto edits review"
ralph scan --sandbox disabled --runner codex "sandbox audit"
ralph scan --repo-prompt plan "Deep codebase analysis"
ralph scan --repo-prompt off "Quick surface scan"
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
* `archive`: move terminal tasks (done/rejected) from queue.json to done.json.
* `repair`: repair the queue and done files (fix missing fields, duplicates, timestamps).
* `unlock`: remove the queue lock file.
* `sort`: sort tasks by priority (reorders the queue file).
* `stats`: show task statistics (completion rate, avg duration, tag breakdown).
* `history`: show task history timeline (creation/completion events by day).
* `burndown`: show burndown chart of remaining tasks over time.
* `schema`: print the JSON schema for the queue file.
* `export`: export task data to CSV, TSV, or JSON format.

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
ralph --verbose queue validate
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

Flags:

* `--count <N>` (or `-n`): Number of sequential IDs to generate (default: 1, max: 100).

```bash
ralph queue next-id
ralph queue next-id --count 5
ralph queue next-id -n 3
ralph --verbose queue next-id
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

Move terminal tasks (done/rejected) from `.ralph/queue.json` to `.ralph/done.json`.

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

### `ralph queue graph`

Visualize task dependencies as a graph with critical path highlighting.

Flags:

* `--task <TASK_ID>`: Focus on a specific task (show its dependency tree).
* `--format <tree|dot|json|list>`: Output format (default: `tree`).
* `--include-done`: Include completed tasks in output.
* `--critical`: Show only critical path.
* `--reverse`: Show reverse dependencies (what this task blocks).

Examples:

```bash
# Show full dependency graph as ASCII tree
ralph queue graph

# Show dependency tree for specific task
ralph queue graph --task RQ-0001

# Export to Graphviz DOT format for external rendering
ralph queue graph --format dot > deps.dot
dot -Tpng deps.dot -o deps.png

# Show what tasks are blocked by a specific task
ralph queue graph --task RQ-0001 --reverse

# Show only critical path
ralph queue graph --critical

# Include completed tasks
ralph queue graph --include-done

# JSON output for programmatic use
ralph queue graph --format json
```

### `ralph queue export`

Export task data to CSV, TSV, or JSON format for external analysis and reporting.

Flags:

* `--format <csv|tsv|json>`: output format (default: `csv`).
* `--output <PATH>` (or `-o`): output file path (default: stdout).
* `--status <draft|todo|doing|done|rejected>`: filter by status (repeatable).
* `--tag <TAG>`: filter by tag (repeatable, case-insensitive).
* `--scope <TOKEN>`: filter by scope token (repeatable, case-insensitive; substring match).
* `--id-pattern <PATTERN>`: filter by task ID substring match.
* `--created-after <DATE>`: filter tasks created after date (RFC3339 or YYYY-MM-DD).
* `--created-before <DATE>`: filter tasks created before date (RFC3339 or YYYY-MM-DD).
* `--include-archive`: include tasks from `.ralph/done.json` archive.
* `--only-archive`: only export tasks from `.ralph/done.json` (ignores active queue).

CSV/TSV output includes all task fields with arrays flattened to delimited strings:
* `tags`, `scope`, `depends_on`: comma-separated
* `evidence`, `plan`, `notes`: semicolon-separated
* `custom_fields`: key=value pairs, comma-separated

```bash
# Export all tasks to CSV (default)
ralph queue export

# Export to file
ralph queue export --format csv --output tasks.csv

# Export completed tasks to JSON
ralph queue export --format json --status done

# Export tasks with specific tags to TSV
ralph queue export --format tsv --tag rust --tag cli

# Include archive tasks
ralph queue export --include-archive --format csv

# Export only archived tasks from last 30 days
ralph queue export --only-archive --format csv --created-after 2026-01-01

# Export tasks matching ID pattern
ralph queue export --id-pattern RQ-01
```

## `ralph task`

Create tasks and edit task fields from CLI.

Common subcommands:
- `ralph task <request>`: create a task from a freeform request.
- `ralph task --template <name> [target] <request>`: create a task from a template with optional target.
- `ralph task show <TASK_ID>`: show task details (queue + done). Alias: `details`.
- `ralph task status <draft|todo|doing|done|rejected> <TASK_ID>`: update status.
- `ralph task edit <FIELD> <VALUE> <TASK_ID>`: edit any task field (default + custom).
- `ralph task field <KEY> <VALUE> <TASK_ID>`: set one custom field.
- `ralph task update [TASK_ID]`: refresh task fields based on current repo state (omit `TASK_ID` to update all tasks).
- `ralph task template list`: list available templates.
- `ralph task template show <name>`: show template details.
- `ralph task template build <name> [target] <request>`: build a task from a template.

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

**TUI Parity**: The TUI also supports building tasks with agent overrides. Press `N` in the TUI, enter a description, then configure optional overrides (runner, model, effort, tags, scope, repo-prompt mode) before building. See `docs/tui-task-management.md` for details.

### ralph task show

Show a task by ID (searches queue + done).

Flags:

* `--format <json|compact>`: output format (default: `json`).

Alias:

* `ralph task details` (same flags and behavior).

```bash
ralph task show RQ-0001
ralph task show RQ-0001 --format compact
ralph task details RQ-0001 --format compact
```

### ralph task edit

Edit any task field directly. Supports all task fields including custom fields.

Fields that can be edited:
- `title` - task title (cannot be empty)
- `status` - draft, todo, doing, done, rejected (empty value cycles to next status)
- `priority` - critical, high, medium, low (empty value cycles to next priority)
- `tags`, `scope`, `evidence`, `plan`, `notes`, `depends_on` - comma/newline-separated lists
- `request` - task request description (empty value clears the field)
- `custom_fields` - key=value pairs, comma/newline-separated
- `created_at`, `updated_at`, `completed_at` - RFC3339 timestamps

Flags:
- `--dry-run` - preview changes without modifying the queue

Examples:
```bash
# Edit task fields
ralph task edit title "New title" RQ-0001
ralph task edit status doing RQ-0001
ralph task edit priority high RQ-0001
ralph task edit tags "cli, rust" RQ-0001
ralph task edit custom_fields "severity=high, owner=ralph" RQ-0001
ralph task edit request "" RQ-0001

# Preview changes without applying
ralph task edit --dry-run title "Preview title" RQ-0001
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
- `--runner/--model/--effort` (`-e`) - runner override for this invocation
- `--repo-prompt <tools|plan|off>` (alias: `-rp`) - RepoPrompt planning/tooling mode
- `--dry-run` - preview the prompt that would be sent to the runner (actual changes depend on runner analysis)
- Runner CLI overrides: `--approval-mode <default|auto-edits|yolo|safe>`, `--sandbox <default|enabled|disabled>`, `--verbosity <quiet|normal|verbose>`, `--plan-mode <default|enabled|disabled>`, `--output-format <stream-json|json|text>`, `--unsupported-option-policy <ignore|warn|error>`.

Examples:
```bash
ralph task update
ralph task update RQ-0001
ralph task update --fields scope,evidence,plan RQ-0001
ralph task update --runner opencode --model gpt-5.2 RQ-0001
ralph task update --approval-mode auto-edits --runner claude RQ-0001
ralph task update --repo-prompt plan RQ-0001
ralph task update --repo-prompt off --fields scope,evidence RQ-0001
ralph task update --fields tags RQ-0042

# Preview what would be updated (shows prompt preview)
ralph task update --dry-run RQ-0001
```

### ralph task template

Use task templates to quickly create well-structured tasks for common development patterns. Templates pre-fill task fields like tags, scope, priority, and plan structure.

**Built-in Templates:**

| Template | Description | Priority | Tags |
|----------|-------------|----------|------|
| `bug` | Bug fix with reproduction steps | high | bug, fix |
| `feature` | New feature with design and docs | medium | feature, enhancement |
| `refactor` | Code refactoring | medium | refactor, cleanup |
| `test` | Test addition or improvement | high | test, coverage |
| `docs` | Documentation update | low | docs, documentation |
| `add-tests` | Add tests for existing code | high | test, coverage, quality |
| `refactor-performance` | Performance optimization | medium | refactor, performance |
| `fix-error-handling` | Fix error handling | high | bug, error-handling |
| `add-docs` | Add documentation for a file | low | docs, documentation |
| `security-audit` | Security audit | critical | security, audit |

**Template Variables:**

Templates support variable substitution using `{{variable}}` syntax:
- `{{target}}` - The target file/path you specify
- `{{module}}` - Module name derived from target (e.g., `src/cli/task.rs` → `cli::task`)
- `{{file}}` - Filename only (e.g., `task.rs`)
- `{{branch}}` - Current git branch name

**Subcommands:**

- `ralph task template list` - List all available templates
- `ralph task template show <name>` - Show template details
- `ralph task template build <name> [target] <request>` - Build task from template

**Examples:**

```bash
# List available templates
ralph task template list

# Show template details
ralph task template show add-tests

# Create task from template (with target for variable substitution)
ralph task template build add-tests src/module.rs "Add unit tests for module"

# Create task from template (shorthand)
ralph task --template add-tests src/module.rs "Add unit tests"

# Interactive template selection (when running in TTY without --template)
ralph task "Add tests for the module"
# (prompts to select template and enter target)
```

**Custom Templates:**

Create custom templates in `.ralph/templates/<name>.json`. Custom templates override built-in templates with the same name.

Example custom template (`.ralph/templates/api-test.json`):
```json
{
  "title": "Add API tests for {{target}}",
  "status": "todo",
  "priority": "high",
  "tags": ["test", "api", "{{module}}"],
  "scope": ["{{target}}"],
  "plan": [
    "Identify API endpoints in {{target}}",
    "Write integration tests",
    "Add edge case coverage",
    "Run make ci"
  ]
}
```

### ralph task build refactor

Automatically create refactoring tasks for large files exceeding a LOC threshold.

Scans the specified directory for Rust files exceeding the LOC threshold and creates refactoring tasks using the built-in "refactor" template. Files are grouped based on the batch mode:
- `auto`: Groups related files (e.g., test files with their source) in the same directory.
- `never`: Creates one task per file.
- `aggressive`: Groups all large files in the same module/directory.

Generated tasks include:
- Title indicating the file(s) and LOC count
- Scope pointing to the relevant file(s)
- Tags: "refactor", "large-file", plus any user-specified tags
- The "refactor" template plan

Flags:
- `--threshold <N>` - LOC threshold (default: 1000). Files exceeding this are flagged.
- `--path <DIR>` - Directory to scan (default: crates/ralph/src).
- `--dry-run` - Preview tasks without creating them.
- `--batch <MODE>` - Batching behavior: `auto`, `never`, or `aggressive` (default: `auto`).
- `--tags <TAGS>` - Additional tags for generated tasks (comma-separated).
- `--runner/--model/--effort` (`-e`) - runner override for this invocation
- `--repo-prompt <tools|plan|off>` (alias: `-rp`) - RepoPrompt planning/tooling mode
- Runner CLI overrides: `--approval-mode <default|auto-edits|yolo|safe>`, `--sandbox <default|enabled|disabled>`, `--verbosity <quiet|normal|verbose>`, `--plan-mode <default|enabled|disabled>`, `--output-format <stream-json|json|text>`, `--unsupported-option-policy <ignore|warn|error>`.

Examples:
```bash
# Scan default directory with default threshold (1000 LOC)
ralph task build refactor

# Use lower threshold
ralph task build refactor --threshold 700

# Scan specific directory
ralph task build refactor --path crates/ralph/src/cli

# Preview without creating tasks
ralph task build refactor --dry-run

# Create individual tasks per file (no batching)
ralph task build refactor --batch never

# Add custom tags
ralph task build refactor --tags urgent,technical-debt

# Combine options
ralph task build refactor --threshold 500 --path src --dry-run
```

## `ralph prd`

Convert PRD (Product Requirements Document) markdown files to Ralph tasks automatically.

### Subcommands

* `create <PATH>`: Create task(s) from a PRD markdown file.

### `ralph prd create`

Parses a PRD markdown file and converts it to one or more Ralph tasks.

By default, creates a single consolidated task containing all PRD content. Use `--multi` to create one task per user story found in the PRD.

#### PRD Format

The PRD should follow standard markdown structure:

* **Title**: First `# Heading` becomes the task title
* **Introduction/Overview** (optional): Content under `## Introduction` or `## Overview`
* **User Stories** (optional): `### US-XXX: Story Title` format with:
  * Description (the "As a... I want... so that..." part)
  * Acceptance Criteria (checkbox list `- [ ]`)
* **Functional Requirements** (optional): Bulleted or numbered list
* **Non-Goals** (optional): Out of scope items

Example PRD structure:

```markdown
# New Feature PRD

## Introduction

Overview of the feature and its purpose.

## User Stories

### US-001: User Authentication
**Description:** As a user, I want to log in so that I can access my account.

**Acceptance Criteria:**
- [ ] Login form validates email format
- [ ] Password must be at least 8 characters
- [ ] Session persists for 24 hours

### US-002: Password Reset
**Description:** As a user, I want to reset my password so that I can recover access.

**Acceptance Criteria:**
- [ ] Reset link sent to verified email
- [ ] Link expires after 1 hour

## Functional Requirements

1. Support email/password authentication
2. Implement OAuth2 for Google/GitHub
3. Store passwords hashed with bcrypt

## Non-Goals

- Two-factor authentication (future phase)
- Social media profile import
```

#### Flags

* `--multi`: Create multiple tasks (one per user story) instead of a single consolidated task.
* `--dry-run`: Preview generated tasks without inserting into the queue.
* `--priority <low|medium|high|critical>`: Set priority for generated tasks (default: medium).
* `--tag <TAG>`: Add tags to all generated tasks (repeatable).
* `--draft`: Create tasks with draft status instead of todo.

#### Task Generation

**Single Task Mode (default):**
* `title`: PRD title
* `request`: PRD introduction + reference
* `plan`: Functional requirements + acceptance criteria from all user stories
* `notes`: Non-goals section
* `tags`: "prd" + any `--tag` values

**Multi Task Mode (`--multi`):**
* One task per user story (US-XXX)
* `title`: "[PRD Title] - [Story Title]"
* `request`: Story description
* `plan`: Story acceptance criteria
* `depends_on`: Previous story ID (creates sequential dependency chain)
* `tags`: "prd", "user-story" + any `--tag` values

#### Examples

```bash
# Create a single consolidated task from PRD
ralph prd create docs/prd/new-feature.md

# Create one task per user story
ralph prd create docs/prd/new-feature.md --multi

# Preview without modifying queue
ralph prd create docs/prd/new-feature.md --dry-run

# Create with high priority and tags
ralph prd create docs/prd/new-feature.md --priority high --tag feature --tag v2.0

# Create as draft tasks
ralph prd create docs/prd/new-feature.md --draft

# Multi-task mode with custom priority and tags
ralph prd create docs/prd/new-feature.md --multi --priority medium --tag user-story
```

## `ralph prompt`

Manage and inspect prompt templates. This command provides:

1. **Prompt previews** - See the exact compiled prompt sent to runners
2. **Template management** - List, view, export, and sync embedded prompt templates

### Preview Compiled Prompts

Key flags:
- `ralph prompt worker --phase <1|2|3>`: choose phase prompt.
- `--iterations` / `--iteration-index`: simulate follow-up iteration context.
- `--plan-text` / `--plan-file`: embed phase 2 plan text for previewing.
- `--repo-prompt <tools|plan|off>` (alias: `-rp`): RepoPrompt planning/tooling mode.

Examples:

```bash
ralph prompt worker --phase 1 --repo-prompt plan
ralph prompt worker --phase 2 --plan-text "Plan body"
ralph prompt worker --phase 2 --iteration-index 2 --iterations 3
ralph prompt worker --phase 3 --task-id RQ-0001
```

### List Available Templates

List all available embedded prompt templates with descriptions:

```bash
ralph prompt list
```

Output shows:
- Template name (e.g., `worker`, `worker_phase1`, `scan`)
- Description of each template's purpose
- `[override]` indicator if a custom version exists in `.ralph/prompts/`

### Show Raw Templates

View raw embedded prompt content or the effective prompt (with user overrides applied):

```bash
# View raw embedded default (what ships with Ralph)
ralph prompt show worker --raw

# View effective prompt (user override if exists, else embedded)
ralph prompt show worker
```

Template names accept:
- Snake_case: `worker_phase1`, `task_builder`
- Kebab-case: `worker-phase1`, `task-builder`
- Case-insensitive: `WORKER`, `Worker_Phase1`

### Export Templates

Export embedded prompts to `.ralph/prompts/` for customization:

```bash
# Export all templates
ralph prompt export --all

# Export single template
ralph prompt export worker

# Overwrite existing files
ralph prompt export worker --force
```

Exported files include a header with version information:
```markdown
<!-- Exported from Ralph embedded defaults -->
<!-- Template: worker -->
<!-- Version: 0.5.0 -->
<!-- Hash: hash:abc123... -->
<!-- Exported at: 2026-01-28T22:30:00Z -->
<!-- WARNING: This file may be overwritten by 'ralph prompt sync' unless you rename it -->
```

### Sync Templates

Check for outdated prompts and sync with embedded defaults:

```bash
# Preview changes without applying
ralph prompt sync --dry-run

# Sync (updates outdated, creates missing, preserves user modified)
ralph prompt sync

# Force overwrite of user modifications
ralph prompt sync --force
```

Sync categories:
- **Up to date**: File matches embedded default (no action)
- **Outdated**: File was exported but embedded default has changed (safe to update)
- **User modified**: File differs from both embedded and exported version (preserved by default)
- **Missing**: Not yet exported (will be created)

### Diff Templates

Show differences between user override and embedded default:

```bash
ralph prompt diff worker
```

Shows unified diff format. If no local override exists, reports that the embedded default is being used.

## Runner and Model Overrides

These flags are supported on `task`, `scan`, `run one`, `run loop`, and `tui`:

* `--runner <codex|opencode|gemini|claude|cursor|kimi|pi>`
* `--model <model-id>`
* `--effort <low|medium|high|xhigh>` (codex only; alias: `-e`)
* `--repo-prompt <tools|plan|off>` (alias: `-rp`) — `tools` = tooling reminders only, `plan` = planning requirement + tooling reminders, `off` = disable both.
* `--approval-mode <default|auto-edits|yolo|safe>`
* `--sandbox <default|enabled|disabled>`
* `--verbosity <quiet|normal|verbose>`
* `--plan-mode <default|enabled|disabled>`
* `--output-format <stream-json|json|text>` (execution requires `stream-json`)
* `--unsupported-option-policy <ignore|warn|error>`

Note: `--rp-on`/`--rp-off` were removed in favor of `--repo-prompt <tools|plan|off>`.

Claude permission precedence:
- CLI `--approval-mode` overrides config `agent.runner_cli.*` approval defaults.
- `approval-mode=auto-edits` maps to Claude `acceptEdits`; `approval-mode=yolo` maps to `bypassPermissions`.
- `approval-mode=default|safe` uses `agent.claude_permission_mode` (if set); otherwise runner defaults apply.

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
* `--notify`: Enable desktop notification on task completion (overrides config).
* `--no-notify`: Disable desktop notification on task completion (overrides config).
* `--notify-fail`: Enable desktop notification on task failure (overrides config).
* `--no-notify-fail`: Disable desktop notification on task failure (overrides config).
* `--notify-sound`: Enable sound alert with notification (works with notification flags or when enabled in config).
* `--git-revert-mode <ask|enabled|disabled>`
* `--git-commit-push-on` / `--git-commit-push-off`
* `--debug` (capture raw supervisor + runner output to `.ralph/logs/debug.log`)

Examples:

```bash
ralph run one --include-draft
ralph run one --update-task
ralph run one --no-update-task
ralph run one --notify
ralph run one --notify --notify-sound
ralph run one --no-notify
ralph run one --notify-fail
ralph run one --no-notify-fail
ralph run one --git-revert-mode disabled
ralph run one --git-commit-push-off
ralph run loop --include-draft --max-tasks 1
ralph run loop --update-task --max-tasks 1
ralph run loop --max-tasks 1 --debug
```

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

## Help Output

For the full, authoritative list of flags and examples, run:

```bash
ralph --help
ralph tui --help
ralph queue --help
ralph run --help
```
