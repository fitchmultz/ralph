# CLI Reference

![CLI Commands](assets/images/2026-02-07-cli-commands.png)

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
- `--auto-fix`: automatically approve all migrations and fixes without prompting (useful for CI/scripting).
- `--no-sanity-checks`: skip all startup sanity checks.

Color output is automatically enabled when stdout is a TTY and disabled when piped or redirected. The `NO_COLOR` environment variable is also respected.

### Sanity Checks

Ralph runs automatic startup health checks on certain commands (`run one`, `run loop`, `queue validate`) to catch common configuration issues:

1. **README Auto-Update**: If the embedded README template is newer than `.ralph/README.md`, it is automatically updated (no prompt - users should not edit this file manually).
2. **Config Migrations**: Detects deprecated/renamed config keys and prompts for migration.
3. **Unknown Config Keys**: Detects unknown config keys and prompts to remove, keep, or rename them.

Use `--auto-fix` to automatically apply all fixes without prompting (useful for CI). Use `--no-sanity-checks` to skip all health checks entirely. For `run one`, use `--non-interactive` to skip prompts even in a TTY environment.

Examples:
```bash
ralph --verbose queue list
ralph --force queue archive
ralph --color never queue list
ralph --no-color queue list
NO_COLOR=1 ralph queue list

# Auto-approve all migrations without prompting
ralph --auto-fix run one

# Skip sanity checks entirely
ralph --no-sanity-checks run loop
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
* `ralph productivity <subcommand>`: view productivity analytics (streaks, velocity, milestones).
* `ralph plugin <subcommand>`: manage plugins (list, validate, install, uninstall, init).
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

## `ralph daemon`

Manage Ralph as a background daemon (continuous execution mode). The daemon runs `ralph run loop --continuous --wait-when-blocked` in the background, automatically executing tasks as they appear in the queue.

**Note:** Daemon mode is Unix-only (Linux, macOS). On Windows, use `ralph run loop --continuous` in a terminal or configure a Windows service.

### Subcommands

* `ralph daemon start`: Start the daemon in the background.
* `ralph daemon stop`: Stop the daemon gracefully.
* `ralph daemon status`: Show daemon status (running, stopped, or stale).

### `ralph daemon start`

Start Ralph as a background daemon. The daemon detaches from the terminal and logs to `.ralph/logs/daemon.log`.

#### Flags

* `--empty-poll-ms <MS>`: Poll interval in milliseconds while waiting for new tasks when queue is empty (default: 30000, min: 50).
* `--wait-poll-ms <MS>`: Poll interval in milliseconds while waiting for blocked tasks (default: 1000, min: 50).
* `--notify-when-unblocked`: Notify when queue becomes unblocked (desktop + webhook).

#### Behavior

- Acquires a dedicated daemon lock at `.ralph/cache/daemon.lock`
- Writes daemon state to `.ralph/cache/daemon.json`
- Redirects stdout/stderr to `.ralph/logs/daemon.log`
- Runs until stopped via `ralph daemon stop` or `ralph queue stop`
- Uses continuous mode: waits for new tasks when queue is empty
- Uses wait-when-blocked: waits for dependencies/schedules to resolve

#### Examples

```bash
# Start the daemon with default settings
ralph daemon start

# Start with faster polling for empty queue
ralph daemon start --empty-poll-ms 5000

# Start with notifications when unblocked
ralph daemon start --notify-when-unblocked
```

#### Service Templates

**systemd (Linux):**

Create `~/.config/systemd/user/ralph.service`:

```ini
[Unit]
Description=Ralph Daemon
After=network.target

[Service]
Type=simple
WorkingDirectory=/path/to/your/repo
ExecStart=/home/username/.local/bin/ralph daemon serve
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
```

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable ralph
systemctl --user start ralph
```

**launchd (macOS):**

Create `~/Library/LaunchAgents/com.ralph.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.ralph.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/username/.local/bin/ralph</string>
        <string>daemon</string>
        <string>serve</string>
    </array>
    <key>WorkingDirectory</key>
    <string>/path/to/your/repo</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/path/to/your/repo/.ralph/logs/daemon.log</string>
    <key>StandardErrorPath</key>
    <string>/path/to/your/repo/.ralph/logs/daemon.log</string>
</dict>
</plist>
```

Load and start:

```bash
launchctl load ~/Library/LaunchAgents/com.ralph.daemon.plist
launchctl start com.ralph.daemon
```

### `ralph daemon stop`

Stop the daemon gracefully by sending a stop signal. Waits up to 10 seconds for the daemon to exit.

#### Examples

```bash
ralph daemon stop
```

### `ralph daemon status`

Show the current daemon status: running (with PID and start time), stopped, or stale (state file exists but process is dead).

#### Examples

```bash
ralph daemon status
```

## `ralph webhook`

Test webhook configuration and inspect webhook payloads.

### Subcommands

* `ralph webhook test`: Send a test webhook event.

### `ralph webhook test`

Send a test webhook event to verify your configuration.

#### Flags

* `--event <EVENT>`: Event type to send (default: `task_created`).
  - Task events: `task_created`, `task_started`, `task_completed`, `task_failed`, `task_status_changed`
  - Loop events: `loop_started`, `loop_stopped`
  - Phase events: `phase_started`, `phase_completed`
* `--url <URL>`: Override webhook URL (uses config if not specified).
* `--task-id <ID>`: Task ID to use in test payload (default: `TEST-0001`).
* `--task-title <TITLE>`: Task title to use in test payload.
* `--print-json`: Print the JSON payload without sending (useful for debugging).
* `--pretty`: Pretty-print JSON (default: `true`, only used with `--print-json`).

#### Examples

```bash
# Test with default event (task_created) and configured URL
ralph webhook test

# Test a specific task event
ralph webhook test --event task_completed

# Test new event types (phase/loop events are opt-in)
ralph webhook test --event phase_started
ralph webhook test --event loop_started

# Print JSON payload without sending
ralph webhook test --event phase_completed --print-json

# Compact JSON output
ralph webhook test --event task_created --print-json --pretty false

# Test with custom URL
ralph webhook test --url https://example.com/webhook
```

## `ralph config`

Inspect and manage Ralph configuration. This command displays the resolved configuration (after merging global and project configs), prints file paths, and outputs the JSON schema.

### Subcommands

* `show`: Display the resolved configuration (YAML by default, JSON with `--format json`).
* `paths`: Print paths to queue, done archive, and config files.
* `schema`: Print the JSON schema for configuration validation.
* `profiles`: List and inspect configuration profiles.

### `ralph config show`

Display the resolved Ralph configuration after merging global and project configs.

#### Output Formats

* `--format yaml` (default): Human-readable YAML output.
* `--format json`: Machine-readable JSON output for scripting.
* `--format text`: Alias for `yaml` (backward compatibility).

The default YAML format is suitable for human inspection. Use `--format json` when piping to tools like `jq` for automated processing.

#### Scripting Examples

```bash
# Machine-readable config for scripting
ralph config show --format json | jq '.agent.runner'
ralph config show --format json | jq '.agent.model'

# Check if CI gate is enabled
ralph config show --format json | jq '.agent.ci_gate_enabled'

# Get queue file path
ralph config show --format json | jq '.queue.file'

# Human-readable config
ralph config show

# Explicit YAML output
ralph config show --format yaml
```

### `ralph config paths`

Print paths to Ralph-related files (queue, done archive, global config, project config).

```bash
ralph config paths
```

Output includes:
* `repo_root`: Root of the current repository
* `queue`: Path to `.ralph/queue.json`
* `done`: Path to `.ralph/done.json`
* `global_config`: Path to global config (if available)
* `project_config`: Path to project config (if available)

### `ralph config schema`

Print the JSON schema for the Ralph configuration file. This schema can be used for validation in editors that support JSON Schema.

```bash
ralph config schema
```

### `ralph config profiles`

List and inspect configuration profiles for quick workflow switching.

#### `ralph config profiles list`

List all available profiles (built-in + user-defined from config):

```bash
ralph config profiles list
```

Output includes profile name, whether it's built-in, and a summary of configured overrides.

#### `ralph config profiles show <NAME>`

Display the effective configuration patch for a specific profile:

```bash
ralph config profiles show quick
ralph config profiles show thorough
ralph config profiles show my-custom-profile
```

## `ralph init`

Bootstrap Ralph files in the current repository with an optional interactive onboarding wizard.

### Interactive Wizard

When running with both stdin and stdout as TTYs (or with `--interactive`), the wizard guides new users through:

1. **Runner Selection**: Choose from Claude, Codex, OpenCode, Gemini, or Cursor
2. **Model Selection**: Pick the appropriate model for your chosen runner
3. **Workflow Mode**: Select 1-phase (quick), 2-phase (standard), or 3-phase (full) workflow
4. **First Task**: Optionally create your first task with title, description, and priority

The wizard explains each option and generates a properly configured `.ralph/config.json` and `.ralph/queue.json`.

### Flags

* `--force`: Overwrite existing files if they already exist.
* `--interactive` (`-i`): Force interactive wizard mode (requires both stdin and stdout to be TTYs). Fails if TTY is not available.
* `--non-interactive`: Skip interactive prompts even if running in a TTY (use defaults).

### Workflow Modes

* **3-phase (Full)**: Plan → Implement + CI → Review + Complete [Recommended]
* **2-phase (Standard)**: Plan → Implement (faster, less review)
* **1-phase (Quick)**: Single-pass execution (simple fixes only)

### Examples

```bash
# Auto-detect TTY (requires both stdin and stdout TTYs) and run wizard if interactive
ralph init

# Force wizard mode (fails without TTY)
ralph init --interactive

# Skip wizard, use defaults (good for CI/scripts)
ralph init --non-interactive

# Overwrite existing files with wizard
ralph init --force --interactive

# Check README without interactive prompts (works in non-TTY)
ralph init --check
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
* `--interactive`: Interactive mode to guide through context creation with prompts for project type, output path, build/test commands, and project description.

Examples:

```bash
ralph context init
ralph context init --force
ralph context init --project-type rust
ralph context init --project-type python --output docs/AGENTS.md

# Interactive mode with guided prompts
ralph context init --interactive
```

### `ralph context update`

Update AGENTS.md with new learnings. This appends content to specific sections without regenerating the entire file.

Flags:

* `--section <NAME>`: Section to update (can be specified multiple times).
* `--file <PATH>`: File containing new learnings to append.
* `--interactive`: Interactive mode to select sections and input learnings. Presents a multi-select menu of existing sections, then prompts for new content via editor or single-line input.
* `--dry-run`: Preview changes without writing to disk.
* `--output <PATH>`: Output path (default: existing AGENTS.md location).

Examples:

```bash
ralph context update --section troubleshooting
ralph context update --section troubleshooting --section git-hygiene
ralph context update --file new_learnings.md
ralph context update --section troubleshooting --dry-run

# Interactive mode to select sections and add content
ralph context update --interactive
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

### Flags

- `--auto-fix`: Automatically apply safe fixes without prompting (queue repair, orphaned lock removal).
- `--no-sanity-checks`: Skip sanity checks and only run doctor diagnostics.
- `--format <text|json>`: Output format (default: `text`). Use JSON for scripting/CI integration.

### Output Formats

**Text format** (default): Human-readable output with colored status indicators.

**JSON format**: Machine-readable structured output suitable for scripting and CI:

```json
{
  "success": true,
  "checks": [
    {
      "category": "git",
      "check": "git_binary",
      "severity": "Success",
      "message": "git binary found",
      "fix_available": false
    },
    {
      "category": "queue",
      "check": "queue_valid",
      "severity": "Error",
      "message": "queue validation failed: missing required field 'title'",
      "fix_available": true,
      "fix_applied": true,
      "suggested_fix": "Run 'ralph queue repair' or use --auto-fix"
    }
  ],
  "summary": {
    "total": 12,
    "passed": 9,
    "warnings": 1,
    "errors": 1,
    "fixes_applied": 1,
    "fixes_failed": 0
  }
}
```

### Auto-Fix Behavior

When `--auto-fix` is passed, `ralph doctor` will attempt to fix the following issues:

1. **Queue Repair**: Fix missing fields, invalid timestamps, duplicate IDs, and remapped dependencies in queue/done files.
2. **Orphaned Lock Removal**: Remove stale lock directories where the owning PID is no longer running.

Unsafe issues (missing runner binary, invalid git repo, missing Makefile) will still require manual intervention.

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

### Examples

```bash
# Basic doctor check
ralph doctor

# Verbose output
ralph --verbose doctor

# Auto-fix issues (repair queue, remove orphaned locks)
ralph doctor --auto-fix

# Skip sanity checks, only run doctor diagnostics
ralph doctor --no-sanity-checks

# JSON output for scripting
ralph doctor --format json

# JSON output with auto-fix and parse result
if ! ralph doctor --format json --auto-fix | jq -e '.success'; then
  echo "Doctor checks failed"
  exit 1
fi

# Check for specific issues in CI
ralph doctor --format json | jq '.checks[] | select(.severity == "Error")'
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
ralph tui --runner codex --model gpt-5.3-codex --effort high
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

### Session recovery

When a previous run was interrupted, Ralph detects the stale session on the next run and prompts whether to resume. In non-interactive environments (CI, scripts), these prompts can block indefinitely.

* `--resume`: Automatically resume an interrupted session without prompting (for `run loop`).
* `--non-interactive`: Skip interactive prompts for sanity checks and session recovery (for `run one`, `run loop`, and `run resume`). When a stale session is detected in non-interactive mode, it is cleared and execution continues with the next task instead of blocking.

#### Lock interaction during resume

When resuming (`--resume`, `run resume`, or answering "yes" to the resume prompt), Ralph attempts to clear stale queue locks automatically:

* If the queue lock is held by a **dead process** (stale PID), it is automatically cleared before resuming.
* If the queue lock is held by a **live process**, Ralph aborts immediately with recovery guidance.
* If lock metadata is **missing or unreadable**, Ralph aborts with an actionable error.

This prevents the "50 consecutive failures" abort loop that could occur when resuming with a stale lock.

**Manual recovery** (if automatic cleanup fails):

```bash
# Preferred: use the built-in unlock command (unsafe if another ralph is running)
ralph queue unlock

# Alternative: use --force to bypass the lock (also clears stale locks)
ralph run loop --force --resume

# Last resort: manually remove the lock directory
rm -rf .ralph/lock
```

Examples:

```bash
# Non-interactive single task (skips sanity check prompts)
ralph run one --non-interactive

# Non-interactive loop (safe for CI)
ralph run loop --non-interactive --max-tasks 5

# Non-interactive resume
ralph run resume --non-interactive

# Auto-resume without prompting (interactive environments)
ralph run loop --resume --max-tasks 5

# Resume with force (clears stale locks if any)
ralph run loop --resume --force --max-tasks 5
```

### Dry-run mode

Use `--dry-run` to simulate task selection without actually running any tasks. This is useful for understanding what would run and why tasks are blocked before starting execution.

Behavior:
- Performs task selection using the same logic as real runs
- Prints selected task (if any) with status
- When no task is selected, prints detailed blocker information (dependencies, schedule, status)
- Does not acquire queue lock
- Does not modify `.ralph/queue.json` or `.ralph/done.json`
- Does not invoke any runner or start sessions

Flags:

* `--dry-run`: Enable dry-run mode (selection-only introspection).

Constraints:
- Conflicts with `--interactive` (TUI mode)
- Conflicts with `--parallel-worker` and `--parallel`

Examples:

```bash
# See what task would be selected
ralph run one --dry-run

# See what task would be selected including drafts
ralph run one --dry-run --include-draft

# Check if a specific task is runnable
ralph run one --dry-run --id RQ-0001

# Simulate loop mode (reports first selection only)
ralph run loop --dry-run
```

### Wait when blocked (CLI-only)

When all remaining tasks are blocked by unmet dependencies or future schedules, the run loop normally exits with a summary of the blockers. Use `--wait-when-blocked` to keep the loop running and poll for changes instead.

Behavior:
- When blocked, the loop polls `.ralph/queue.json` and `.ralph/done.json` for changes
- When a runnable task appears (dependencies complete or schedule passes), the loop continues
- Poll interval is controlled by `--wait-poll-ms` (default: 1000ms, min: 50ms)
- Optional timeout via `--wait-timeout-seconds` (0 = no timeout)
- Optional notification when unblocked via `--notify-when-unblocked` (desktop + webhook)
- Respects stop signals (`ralph queue stop`) and Ctrl+C

Flags:

* `--wait-when-blocked`: Wait when blocked instead of exiting (default: false).
* `--wait-poll-ms <MS>`: Poll interval in milliseconds while waiting (default: 1000, min: 50).
* `--wait-timeout-seconds <SECONDS>`: Timeout for waiting, 0 = no timeout (default: 0).
* `--notify-when-unblocked`: Notify when queue becomes unblocked (default: false).

Constraints:
- Conflicts with `--parallel` (parallel mode does not support wait mode)
- Conflicts with `--interactive` (TUI mode)
- Only applies to sequential run loop mode

Examples:

```bash
# Wait indefinitely for dependencies/schedules to resolve
ralph run loop --wait-when-blocked

# Wait with a 10-minute timeout
ralph run loop --wait-when-blocked --wait-timeout-seconds 600

# Poll more frequently (250ms) for faster response
ralph run loop --wait-when-blocked --wait-poll-ms 250

# Notify when unblocked (desktop notification + webhook)
ralph run loop --wait-when-blocked --notify-when-unblocked
```

### Continuous mode (CLI-only)

When the queue becomes empty, the run loop normally exits. Use `--wait-when-empty` (alias `--continuous`) to keep the loop running and wait for new tasks instead.

Behavior:
- When the queue is empty at startup, the loop waits instead of exiting
- When the queue becomes empty during execution, the loop waits instead of exiting
- The loop wakes immediately when `.ralph/queue.json` or `.ralph/done.json` changes (using filesystem notifications with poll fallback)
- Poll interval is controlled by `--empty-poll-ms` (default: 30000ms = 30s, min: 50ms)
- No timeout in continuous mode (runs until stopped)
- Respects stop signals (`ralph queue stop`) and Ctrl+C

Combined with `--wait-when-blocked`, the loop will wait for both blocked tasks (dependencies/schedules) and empty queue states.

Flags:

* `--wait-when-empty`: Wait when queue is empty instead of exiting (default: false). Alias: `--continuous`.
* `--empty-poll-ms <MS>`: Poll interval in milliseconds while waiting for new tasks (default: 30000, min: 50).

Constraints:
- Conflicts with `--parallel` (parallel mode does not support continuous mode)
- Conflicts with `--interactive` (TUI mode)
- Only applies to sequential run loop mode

Examples:

```bash
# Continuous mode: wait indefinitely for new tasks
ralph run loop --continuous

# Same as above using long form
ralph run loop --wait-when-empty

# Poll more frequently (5s) for faster response
ralph run loop --continuous --empty-poll-ms 5000

# Combined with wait-when-blocked for always-on operation
ralph run loop --continuous --wait-when-blocked
```

### Parallel loop (CLI-only)

`--parallel [N]` runs multiple tasks concurrently in separate isolated git workspace clones. The default is `2`
when `--parallel` is provided without a value. This mode is CLI-only and conflicts with
interactive/TUI workflows.

Notes:
- `--parallel` conflicts with `--interactive`, `--wait-when-blocked`, and ignores `--resume`.
- Each task runs in its own isolated git workspace clone and branch; PRs are created/merged automatically when enabled.
- Queue and done files are coordinator-only in parallel mode; worker branches do not modify `.ralph/queue.json` or `.ralph/done.json`.
- Workers commit per-task completion signals in `.ralph/cache/completions/<TASK_ID>.json`.
- After a PR merge, the coordinator applies the completion signal to update queue/done (and stats). If a signal is missing, the merge is halted with an error.
- Parallel workers force RepoPrompt mode to `off` (no tooling or planning requirement) to keep edits inside workspace clones.
- Parallel workers honor `--git-commit-push-on/off` and `agent.git_commit_push_enabled` config. When commit/push is disabled, parallel PR automation (`auto_pr`, `auto_merge`, `draft_on_failure`) is skipped because PRs require pushed commits.
- When PR automation is disabled or PR creation fails, Ralph records the task as finished without a PR in `.ralph/cache/parallel/state.json` and will not re-run it automatically. To retry, remove the finished-without-PR entry from the state file. If the task already completed successfully, mark it done manually (since no PR exists for the coordinator to apply).
- You can set a default worker count with `parallel.workers` in `.ralph/config.json`.
- The default workspace location is `<repo-parent>/.workspaces/<repo-name>/parallel/<TASK_ID>` (configurable via `parallel.workspace_root`).
- State is persisted to `.ralph/cache/parallel/state.json` for crash recovery and coordination.
- On startup, Ralph prunes stale in-flight task records and reconciles PR records before checking the state file's base branch. If the base branch is missing or mismatched and there are no in-flight tasks, open PRs, or finished-without-PR blockers, Ralph auto-heals the state file to the current branch. Otherwise it errors with recovery guidance.

Examples:

```bash
# Run with the default of 2 workers
ralph run loop --parallel --max-tasks 4

# Run with 4 workers
ralph run loop --parallel 4 --max-tasks 8
```

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

### Configuration Profiles

Use `--profile <NAME>` to quickly switch between workflow presets:

* `--profile quick`: Use the quick profile (kimi, 1-phase)
* `--profile thorough`: Use the thorough profile (claude/opus, 3-phase)
* Custom profiles can be defined in config under the `profiles` key

Profile precedence (highest to lowest):
1. CLI flags
2. Task override (`task.agent.*`)
3. Selected profile (if `--profile` specified)
4. Config defaults (`agent.*`)

CLI flags can override specific settings from a profile:

```bash
ralph run one --profile quick --phases 2 --runner claude
```

### Phase-Specific Overrides

Use phase-specific flags to select different runners/models/effort for each execution phase:

* `--runner-phase1`, `--model-phase1`, `--effort-phase1`: Phase 1 (planning) overrides
* `--runner-phase2`, `--model-phase2`, `--effort-phase2`: Phase 2 (implementation) overrides
* `--runner-phase3`, `--model-phase3`, `--effort-phase3`: Phase 3 (review) overrides

Precedence per phase (highest to lowest):
1. CLI phase override (`--runner-phaseN`, `--model-phaseN`, `--effort-phaseN`)
2. Config phase override (`agent.phase_overrides.phaseN.*`)
3. CLI global override (`--runner`, `--model`, `--effort`)
4. Task override (`task.agent.*`)
5. Selected profile (if `--profile` specified)
6. Config defaults (`agent.*`)

Single-pass execution (`--phases 1`) uses Phase 2 overrides (behaviorally closest to implementation).

Examples:

```bash
# Use Codex for planning, Claude for implementation
ralph run one --runner-phase1 codex --model-phase1 gpt-5.2-codex --effort-phase1 high \
              --runner-phase2 claude --model-phase2 opus

# Full 3-phase with different settings per phase
ralph run one --phases 3 \
              --runner-phase1 codex --model-phase1 gpt-5.2-codex --effort-phase1 high \
              --runner-phase2 claude --model-phase2 opus \
              --runner-phase3 codex --model-phase3 gpt-5.2-codex --effort-phase3 high
```

**TUI Limitation**: The TUI task builder (press `N` in the TUI) only supports global
runner/model/effort overrides, not per-phase overrides. To use phase-specific overrides,
either configure them in `.ralph/config.json` under `agent.phase_overrides`, or use the
CLI `ralph run` commands with the phase-specific flags above.

Examples:

```bash
ralph run one
ralph run one --profile quick
ralph run one --profile thorough
ralph run one --profile quick --runner claude  # CLI overrides profile
ralph run one --phases 3
ralph run one --phases 2
ralph run one --phases 1
ralph run one --quick
ralph run one --include-draft
ralph run one -i
ralph run one -i --visualize
ralph run one --update-task
ralph run loop --max-tasks 0
ralph run loop --profile quick --max-tasks 5
ralph run loop --profile thorough --max-tasks 3
ralph run loop --phases 3 --max-tasks 0
ralph run loop --quick --max-tasks 1
ralph run loop --include-draft --max-tasks 1
ralph run loop --update-task --max-tasks 1
ralph run loop --repo-prompt tools --max-tasks 1
ralph run loop --repo-prompt off --max-tasks 1
ralph run loop --parallel --max-tasks 4
ralph run loop --parallel 4 --max-tasks 8
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
ralph scan --profile thorough "deep risk audit"
ralph scan --profile quick "quick bug fixes"
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
* `export`: export task data to CSV, TSV, JSON, Markdown, or GitHub issue format.
* `import`: import tasks from CSV, TSV, or JSON into the active queue.
* `stop`: request graceful stop of a running loop after current task completes.

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
  * `list`, `search`: `--format <compact|long|json>` (default: `compact`)
  * `show`: `--format <json|compact>` (default: `json`)
  * `stats`, `history`, `burndown`: `--format <text|json>` (default: `text`)
* Limits (`list`, `search`):
  * `--limit <N>` (default: 50; `0` = no limit)
  * `--all`: ignore `--limit`
* Sorting:
  * `list`: `--sort-by <priority|created_at|updated_at|started_at|scheduled_start|status|title>` and `--order <ascending|descending>` (sorts output only; missing/invalid timestamps sort last)
  * `sort`: `--sort-by priority` and `--order <ascending|descending>` (reorders queue file; priority only for safety)
* Scheduled filters (`list`, `search`):
  * `--scheduled`: only show tasks with `scheduled_start` set
  * `--scheduled-after <TIMESTAMP>`: filter tasks scheduled after this time (RFC3339 or relative)
  * `--scheduled-before <TIMESTAMP>`: filter tasks scheduled before this time (RFC3339 or relative)

### `ralph queue validate`

Validate `.ralph/queue.json` (and `.ralph/done.json` if present).

```bash
ralph queue validate
ralph --verbose queue validate
```

See `docs/queue-and-tasks.md` for details on resolving validation errors, including duplicate task ID collisions.

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
* `--with-eta`: include an execution-history-based ETA estimate.
* `--explain`: print an explanation when no runnable task is found (to stderr).

ETA estimates are based on historical execution times for the resolved (runner, model, phase_count) combination. When no history exists, displays `n/a`.

```bash
ralph queue next
ralph queue next --with-title
ralph queue next --with-eta
ralph queue next --with-title --with-eta
ralph queue next --explain
```

### `ralph queue explain`

Explain why tasks are (not) runnable. Provides structured runnability analysis with actionable reasons for blocked tasks.

Flags:

* `--format <text|json>`: output format (default: `text`). JSON output is versioned and stable for scripting.
* `--include-draft`: include draft tasks in the analysis.

```bash
ralph queue explain
ralph queue explain --format json
ralph queue explain --include-draft
ralph queue explain --format json --include-draft
```

Output (text format):
- Summary of queue runnability (total tasks, candidates, runnable count)
- Blocker counts by type (dependencies, schedule, status)
- First blocking task with specific reasons
- Hints for next steps (`ralph queue graph`, `ralph queue list --scheduled`, etc.)

Output (JSON format):
- Full `QueueRunnabilityReport` structure with version, timestamps, and per-task runnability rows
- Each task includes `runnable` boolean and `reasons` array with structured blocker information

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
* `--format <compact|long|json>`: output format (default: `compact`). JSON outputs an array of task objects (same shape as queue export).
* `--limit <N>`: maximum tasks to show (default: 50; `0` = no limit).
* `--all`: show all tasks (ignores `--limit`).
* `--sort-by <priority|created_at|updated_at|started_at|scheduled_start|status|title>`: sort output by field (default: `descending`). Missing/invalid timestamps sort last regardless of order.
* `--order <ascending|descending>`: sort order (default: `descending`).
* `--scheduled`: filter to only show tasks with `scheduled_start` set.
* `--scheduled-after <TIMESTAMP>`: filter tasks scheduled after this time (RFC3339 or relative expression like `+7d`).
* `--scheduled-before <TIMESTAMP>`: filter tasks scheduled before this time (RFC3339 or relative expression).
* `--with-eta`: include an execution-history-based ETA estimate column (text formats only; has no effect on `--format json`).

```bash
ralph queue list
ralph queue list --status todo --tag rust
ralph queue list --status doing --scope crates/ralph
ralph queue list --include-done --limit 20
ralph queue list --only-done --all
ralph queue list --filter-deps=RQ-0100
ralph queue list --format json
ralph queue list --format json | jq '.[] | select(.status == "todo")'
ralph queue list --scheduled
ralph queue list --scheduled-after '2026-01-01T00:00:00Z'
ralph queue list --scheduled-before '+7d'
ralph queue list --with-eta
ralph queue list --with-eta --format long
ralph queue list --sort-by updated_at
ralph queue list --scheduled --sort-by scheduled_start --order ascending
ralph queue list --sort-by status --order ascending
ralph queue list --sort-by title
```

### `ralph queue search`

Search tasks by content (title, evidence, plan, notes, request, tags, scope, custom fields).

Flags:

* `--regex`: interpret query as a regular expression.
* `--match-case`: case-sensitive search (default: case-insensitive).
* `--fuzzy`: use fuzzy matching for search (default: substring).
* `--status <draft|todo|doing|done|rejected>`: filter by status (repeatable).
* `--tag <TAG>`: filter by tag (repeatable, case-insensitive).
* `--scope <TOKEN>`: filter by scope token (repeatable, case-insensitive; substring match).
* `--include-done`: include tasks from `.ralph/done.json` in search.
* `--only-done`: only search tasks in `.ralph/done.json` (ignores active queue).
* `--format <compact|long|json>`: output format (default: `compact`). JSON outputs an array of task objects (same shape as queue export).
* `--limit <N>`: maximum results to show (default: 50; `0` = no limit).
* `--all`: show all results (ignores `--limit`).
* `--scheduled`: filter to only show tasks with `scheduled_start` set.

```bash
ralph queue search "authentication"
ralph queue search "RQ-\d{4}" --regex
ralph queue search "TODO" --match-case
ralph queue search "fix" --status todo --tag rust
ralph queue search "refactor" --scope crates/ralph --tag rust
ralph queue search "auth bug" --fuzzy
ralph queue search "fuzzy search" --fuzzy --match-case
ralph queue search "api" --format json
```

### `ralph queue archive`

Move terminal tasks (done/rejected) from `.ralph/queue.json` to `.ralph/done.json`.

```bash
ralph queue archive
```

**Note:** This command archives immediately. To enable automatic archiving based on task age, configure `queue.auto_archive_terminal_after_days` in your config. When set, the sweep runs during TUI startup/reload and after CLI task edits:

- `null` (default): No automatic sweep
- `0`: Archive all terminal tasks immediately when sweep runs
- `N`: Archive only tasks whose `completed_at` is at least `N` days old

See [Configuration](configuration.md#queue-configuration) for details.

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

**Warning**: Only use this when you are sure no other Ralph process is running. Using this while another ralph is active can cause queue corruption.

```bash
# Safe to use when no other ralph is running
ralph queue unlock

# Check if another ralph is running before unlocking
pgrep -f "ralph run" && echo "Another ralph is running - do not unlock!"
```

### `ralph queue sort`

Sort tasks by priority (reorders the queue file).

Flags:

* `--sort-by <priority>`: sort by field (default: `priority`). Only priority is supported for queue file reordering; use `ralph queue list --sort-by` for time-based triage.
* `--order <ascending|descending>`: sort order (default: `descending`, highest priority first).

```bash
ralph queue sort
ralph queue sort --order descending
ralph queue sort --order ascending
```

Note: `ralph queue sort` intentionally supports only priority sorting to prevent accidental queue reordering. For time-based triage without modifying the queue file, use `ralph queue list --sort-by <field>`.

### `ralph queue stats`

Queue reports default to human-readable text but can emit JSON for scripting.

Summarize completion rates, durations, tag breakdowns, velocity by tag/runner, slow groups, and execution-history-based ETA estimates.

**Runner Analytics**:
The `velocity.by_runner` and `slow_groups.by_runner` sections use the `runner_used` custom field when available (written automatically by Ralph at task completion), falling back to `agent.runner` for backward compatibility. This ensures accurate analytics even for tasks that don't have an explicit `agent` override set.

The execution history ETA section displays:
* **runner/model/phases**: The key used to look up historical data.
* **Samples**: Number of completed task executions in history.
* **Estimated new task**: Expected duration based on historical averages (with confidence level: high/medium/low).

When no execution history exists for the current configuration, the ETA section shows `n/a`.

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

### `ralph queue aging`

Show task aging buckets to identify stale work. Tasks are grouped by age into
fresh, warning, stale, and rotten categories based on configurable thresholds.

Age is computed from the relevant timestamp for each status:
- `todo`/`draft`: uses `created_at`
- `doing`: uses `started_at` (or `created_at` if not started)
- `done`/`rejected`: uses `completed_at`, `updated_at`, or `created_at`

Flags:

* `--status <STATUS>`: filter by status (repeatable). Default: `todo`, `doing`.
* `--format <text|json>`: output format (default: `text`).

```bash
ralph queue aging
ralph queue aging --format json
ralph queue aging --status todo --status doing
ralph queue aging --status doing
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

### `ralph queue tree`

Render a parent/child hierarchy tree based on `parent_id`. This is distinct from `ralph queue graph`, which visualizes `depends_on` relationships.

Flags:

* `--root <TASK_ID>`: Start tree from specific task (default: show all root tasks).
* `--include-done`: Include completed tasks in output.
* `--max-depth <N>`: Maximum depth to render (default: 20).

Examples:

```bash
# Show full hierarchy tree
ralph queue tree

# Show tree starting from specific root
ralph queue tree --root RQ-0001

# Include done tasks
ralph queue tree --include-done

# Limit depth
ralph queue tree --max-depth 5
```

### `ralph queue export`

Export task data to various formats for external analysis, reporting, and sharing.

Flags:

* `--format <csv|tsv|json|md|gh>`: output format (default: `csv`).
  * `csv`: Comma-separated values, good for spreadsheets
  * `tsv`: Tab-separated values, good for command-line processing
  * `json`: JSON array of task objects, good for scripting
  * `md`: Markdown table, good for human-readable summaries
  * `gh`: GitHub issue format, good for pasting into GitHub issues/PRs
* `--output <PATH>` (or `-o`): output file path (default: stdout).
* `--status <draft|todo|doing|done|rejected>`: filter by status (repeatable).
* `--tag <TAG>`: filter by tag (repeatable, case-insensitive).
* `--scope <TOKEN>`: filter by scope token (repeatable, case-insensitive; substring match).
* `--id-pattern <PATTERN>`: filter by task ID substring match.
* `--created-after <DATE>`: filter tasks created after date (RFC3339 or YYYY-MM-DD).
* `--created-before <DATE>`: filter tasks created before date (RFC3339 or YYYY-MM-DD).
* `--include-archive`: include tasks from `.ralph/done.json` archive.
* `--only-archive`: only export tasks from `.ralph/done.json` (ignores active queue).

Format-specific notes:

CSV/TSV output includes all task fields with arrays flattened to delimited strings:
* `tags`, `scope`, `depends_on`: comma-separated
* `evidence`, `plan`, `notes`: semicolon-separated
* `custom_fields`: key=value pairs, comma-separated
* `parent_id`: parent task ID (empty string if none)

Markdown (`md`) output produces a GitHub-flavored Markdown table with columns:
ID, Status, Priority, Title, Tags, Scope, Created. Tasks are sorted by ID for
stable output.

GitHub (`gh`) output produces one Markdown block per task, formatted for optimal
rendering in GitHub issue bodies. Includes clean formatting for plan, evidence,
scope, and notes.

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

# Export todo tasks as Markdown table for sharing
ralph queue export --format md --status todo

# Export high-priority tasks as GitHub issue format
ralph queue export --format gh --status todo --priority high > issue_body.md

# Export all tasks for a GitHub milestone
ralph queue export --format gh --tag milestone-v2 > milestone_tasks.md
```

#### Recommended Markdown Export Workflows

**Share backlog in PR description:**
```bash
# Include todo and doing tasks in PR description
ralph queue export --format md --status todo --status doing
```

**Create GitHub issue for a specific task:**
```bash
# Export single task in GitHub format
ralph queue export --format gh --id-pattern RQ-0042
```

**Generate weekly status report:**
```bash
# Tasks completed this week in Markdown
ralph queue export --format md --status done --created-after 2026-01-27
```


### `ralph queue issue publish`

Publish (create or update) a single Ralph task as a GitHub Issue using the `gh` CLI. The created/updated issue URL is persisted in the task's `custom_fields` for future reference and re-sync.

**Note**: This is a CLI-only command; there is no TUI workflow for issue publish.

Prerequisites:
* GitHub CLI (`gh`) must be installed and authenticated (`gh auth login`)

Flags:

* `--dry-run`: Print the rendered title/body and the `gh` command that would be executed without making any changes.
* `--label <LABEL>`: Labels to apply to the issue (repeatable).
* `--assignee <LOGIN>`: Assignees to apply to the issue (repeatable). Supports `@me` for self-assignment.
* `--repo <OWNER/REPO>`: Target repository (optional; uses current repo by default).

Behavior:

* If `custom_fields.github_issue_url` is missing: Creates a new GitHub issue via `gh issue create`, then stores the returned URL in `custom_fields.github_issue_url` and the issue number in `custom_fields.github_issue_number`.
* If `custom_fields.github_issue_url` exists: Updates the existing issue via `gh issue edit`, syncing the title and body. Labels and assignees are added (not removed).
* The task's `updated_at` timestamp is always updated when a publish operation succeeds.

Examples:

```bash
# Preview the GitHub issue markdown for a task (dry-run)
ralph queue export --format gh --id-pattern RQ-0655

# Publish a task to GitHub Issues (creates new issue)
ralph queue issue publish RQ-0655

# Re-run to sync changes after editing the task
ralph queue issue publish RQ-0655

# Add labels and assignees
ralph queue issue publish RQ-0655 --label bug --label help-wanted --assignee @me

# Target a different repository
ralph queue issue publish RQ-0655 --repo owner/repo --label feature

# Dry run to preview what would happen
ralph queue issue publish RQ-0655 --dry-run
```

Persisted custom fields:

* `github_issue_url`: The canonical GitHub issue URL (e.g., `https://github.com/owner/repo/issues/123`)
* `github_issue_number`: The issue number as a string (e.g., `"123"`)

These fields are compatible with the existing `custom_fields` schema and survive queue round-trips.


### `ralph queue import`

Import tasks from CSV, TSV, or JSON into the active queue. This complements `ralph queue export` and enables bulk backlog seeding, cross-repo task migration, and automation without hand-editing JSON.

**Note**: This is a CLI-only command; there is no TUI workflow for import.

Flags:

* `--format <csv|tsv|json>`: input format (required).
  * `csv`: Comma-separated values
  * `tsv`: Tab-separated values
  * `json`: JSON array of task objects, or `{ "version": 1, "tasks": [...] }` wrapper
* `--input <PATH>` (or `-i`): input file path. If omitted or `-`, reads from stdin.
* `--dry-run`: parse, normalize, and validate without writing to disk.
* `--on-duplicate <fail|skip|rename>`: how to handle duplicate task IDs (default: `fail`).
  * `fail`: error if an imported task ID already exists in queue or done
  * `skip`: drop duplicate tasks and continue importing others
  * `rename`: generate a fresh ID for duplicate tasks

Normalization and backfill:
* List fields are trimmed and empty items are dropped
* Missing `created_at`/`updated_at` timestamps are set to current time
* Tasks with `done`/`rejected` status get `completed_at` backfilled if missing
* Tasks without IDs get auto-generated IDs

CSV/TSV format:
The expected header matches `ralph queue export` output:
`id,title,status,priority,tags,scope,evidence,plan,notes,request,created_at,updated_at,completed_at,depends_on,custom_fields`

Delimiter rules:
* `tags`, `scope`, `depends_on`: comma-separated
* `evidence`, `plan`, `notes`: semicolon-separated
* `custom_fields`: `key=value` pairs, comma-separated

Unknown columns are ignored. Only the `title` column is required.

```bash
# Import from JSON file
ralph queue import --format json --input tasks.json

# Import from CSV with dry-run to preview changes
ralph queue import --format csv --input tasks.csv --dry-run

# Pipe export to import (round-trip test)
ralph queue export --format json | ralph queue import --format json --dry-run

# Import from stdin with duplicate handling
ralph queue export --format tsv | ralph queue import --format tsv --on-duplicate rename

# Import and skip duplicates
ralph queue import --format json --input tasks.json --on-duplicate skip
```

### `ralph queue stop`

Request graceful stop of a running `ralph run loop` after the current task completes.

This command creates a stop signal file that the run loop checks between tasks. When detected:
- The current in-flight task completes normally (all phases finish)
- The loop stops starting new tasks and exits cleanly when safe
- The stop signal is automatically cleared

This is useful when you start a long-running loop with `--max-tasks 0` but want to stop after the current work finishes, without interrupting active task execution.

**Sequential mode** (`ralph run loop`):
- The loop exits between tasks (current task completes, then exits)
- Stop signal is honored after both successful and failed task attempts

**Parallel mode** (`ralph run loop --parallel N`):
- The loop stops scheduling new tasks immediately
- Waits for all in-flight tasks to complete
- Exits once no workers are running

Notes:
- The stop signal does NOT interrupt an active task - it only prevents new tasks from starting
- To force immediate termination, press Ctrl+C in the running loop
- Multiple `ralph queue stop` commands are idempotent (subsequent calls are no-ops)

```bash
# Terminal 1: Start a long-running loop
ralph run loop --max-tasks 0
ralph run loop --profile quick --max-tasks 5
ralph run loop --profile thorough --max-tasks 3

# Terminal 2: Request graceful stop after current task
ralph queue stop
```

### ralph task clone

Clone an existing task to create a new task with the same fields. This is useful for creating task templates from well-structured existing tasks or for creating follow-up work items.

The cloned task will have:
- A new task ID (auto-generated)
- Same title (with optional prefix), priority, tags, scope, evidence, plan, notes, request, and custom fields
- Status set to `draft` by default (or specified via `--status`)
- Fresh timestamps (created_at, updated_at)
- Cleared completed_at
- Cleared depends_on (to avoid unintended dependencies)

Flags:

- `--status <draft|todo|doing>` - Status for the cloned task (default: `draft`).
- `--title-prefix <PREFIX>` - Prefix to add to the cloned task title.
- `--dry-run` - Preview the clone without modifying the queue.

Alias:

- `ralph task duplicate` (same flags and behavior).

Examples:

```bash
# Basic clone (creates draft copy)
ralph task clone RQ-0001

# Clone with specific status
ralph task clone RQ-0001 --status todo

# Clone with title prefix for visibility
ralph task clone RQ-0001 --title-prefix "[Follow-up] "

# Preview clone without creating
ralph task clone RQ-0001 --dry-run

# Using alias
ralph task duplicate RQ-0001
```

### ralph task split

Split a task into multiple child tasks for better granularity. This is useful when a task grows too large and needs to be broken down into smaller, trackable pieces.

The original task will be:
- Marked with custom field `split: "true"` for tracking
- Status changed to `rejected` (terminal state)
- Original scope, evidence, and plan preserved for reference

Child tasks will have:
- New task IDs with `parent_id` set to the original task
- Titles derived from the original (with optional prefix and index)
- Scope and evidence copied from the original
- Plan items distributed across children if `--distribute-plan` is used
- Status set to `draft` by default (or specified via `--status`)

**Flags:**

- `--number <N>` (`-n`) - Number of child tasks to create (default: 2, minimum: 2).
- `--status <draft|todo|doing>` - Status for child tasks (default: `draft`).
- `--title-prefix <PREFIX>` - Prefix to add to child task titles.
- `--distribute-plan` - Distribute plan items evenly across child tasks.
- `--dry-run` - Preview the split without modifying the queue.

**Examples:**

```bash
# Basic split (creates 2 draft child tasks)
ralph task split RQ-0001

# Split into 3 child tasks
ralph task split --number 3 RQ-0001

# Child tasks with specific status
ralph task split --status todo --number 2 RQ-0001

# Add prefix to child titles for visibility
ralph task split --title-prefix "[Part] " RQ-0001

# Distribute plan items across children
ralph task split --distribute-plan RQ-0001

# Preview without creating
ralph task split --dry-run RQ-0001

# Combined options
ralph task split --number 3 --status todo --distribute-plan RQ-0001
```

**Child Task Title Format:**

Without prefix: `"Original Title (1/3)"`, `"Original Title (2/3)"`, etc.
With prefix: `"[Part] Original Title (1/3)"`, etc.

**Plan Distribution:**

When `--distribute-plan` is used, plan items are distributed round-robin across child tasks:
- Parent plan: `["Step A", "Step B", "Step C", "Step D"]`
- With 2 children: Child 1 gets `["Step A", "Step C"]`, Child 2 gets `["Step B", "Step D"]`

### ralph task start

Start work on a task by setting the `started_at` timestamp and transitioning status to `doing`.

This command is useful for explicit time tracking - it records when you actually began working on a task, separate from when the task was created. The `started_at` field enables productivity analytics like work time calculation (started → completed) and start lag metrics (created → started).

**Arguments:**
- `TASK_ID` - Task ID to start

**Flags:**
- `--reset` - Reset `started_at` even if already set (useful for restarting work tracking)

**Examples:**
```bash
# Start work on a task
ralph task start RQ-0001

# Restart tracking (reset existing started_at)
ralph task start --reset RQ-0001
```

**Notes:**
- Cannot start terminal tasks (done/rejected)
- If task is already in `doing` status, only `started_at` is updated (unless `--reset` is used)
- The `started_at` field can also be manually edited via `ralph task edit started_at <TIMESTAMP> <TASK_ID>`

### ralph task children

List child tasks where `parent_id` matches the given task ID. This is useful for navigating task hierarchies created by `ralph task split` or manual `parent_id` assignment.

**Arguments:**
- `TASK_ID` - Parent task ID

**Flags:**
- `--include-done` - Include completed tasks from done archive.
- `--recursive` - Show entire subtree (tree view).
- `--format <compact|long|json>` - Output format (default: `compact`).

**Examples:**
```bash
# List direct children
ralph task children RQ-0001

# List children recursively
ralph task children RQ-0001 --recursive

# Include done archive
ralph task children RQ-0001 --include-done

# JSON output for scripting
ralph task children RQ-0001 --format json
```

### ralph task parent

Show the parent task for a given task (based on `parent_id`). Also displays sibling count.

**Arguments:**
- `TASK_ID` - Child task ID

**Flags:**
- `--include-done` - Search done archive if parent not found in active queue.
- `--format <compact|long|json>` - Output format (default: `compact`).

**Examples:**
```bash
# Show parent and siblings
ralph task parent RQ-0002

# Search done archive if needed
ralph task parent RQ-0002 --include-done

# JSON output
ralph task parent RQ-0002 --format json
```

## `ralph watch`

Watch files for changes and auto-detect tasks from TODO/FIXME/HACK/XXX comments. This command monitors source files and automatically creates tasks when it finds actionable comments.

### How It Works

The watch command uses a fingerprint-based deduplication system to reliably track comments across file changes:

- **Fingerprint Generation**: Each comment gets a stable fingerprint based on file path (relative), line number, and normalized content hash
- **Structured Metadata**: Watch-created tasks store source information in `custom_fields`:
  - `watch.file` - Absolute file path
  - `watch.line` - Line number
  - `watch.comment_type` - One of: `todo`, `fixme`, `hack`, `xxx`
  - `watch.fingerprint` - Stable identifier for deduplication

### Flags

* `PATH...`: Directories or files to watch (defaults to current directory).
* `--patterns <PATTERNS>`: File patterns to watch (comma-separated, default: `*.rs,*.ts,*.js,*.py,*.go,*.java,*.md,*.toml,*.json`).
* `--debounce-ms <MS>`: Debounce duration in milliseconds (default: 500).
* `--auto-queue`: Automatically create tasks without prompting.
* `--notify`: Enable desktop notifications for new tasks.
* `--ignore-patterns <PATTERNS>`: Additional gitignore-style exclusions (comma-separated).
* `--comments <TYPES>`: Comment types to detect: `todo`, `fixme`, `hack`, `xxx`, `all` (default: `all`).
* `--close-removed`: Mark watch-created tasks as done when their originating comments are removed from source.

### Deduplication

The watch command uses fingerprint-based deduplication to avoid creating duplicate tasks:

1. **Primary**: Checks `watch.fingerprint` in task `custom_fields`
2. **Fallback**: Legacy substring matching for backwards compatibility with tasks created before this feature

Fingerprints are stable across:
- Different machines (uses relative paths)
- Whitespace changes (content is normalized)
- Case changes (content is lowercased)

### Comment Reconciliation (`--close-removed`)

When `--close-removed` is enabled, the watch command will:

1. Track all detected comments across watched files
2. Compare against existing watch-tagged tasks in the queue
3. Mark tasks as `done` when their originating comment is no longer present
4. Add a note: `[timestamp] Auto-closed: originating comment was removed from source`

Only tasks with the `watch` tag are affected. Tasks in terminal states (done, rejected) are skipped.

### Examples

```bash
# Basic watch mode (suggests tasks, doesn't create)
ralph watch

# Watch specific directories
ralph watch src/ tests/

# Auto-create tasks without prompting
ralph watch --auto-queue

# Watch with custom patterns
ralph watch --patterns "*.rs,*.toml"

# Watch only TODO and FIXME comments
ralph watch --comments todo,fixme

# Enable desktop notifications
ralph watch --auto-queue --notify

# Ignore vendor and target directories
ralph watch --ignore-patterns "vendor/,target/"

# Auto-close tasks when comments are removed
ralph watch --auto-queue --close-removed

# Full workflow: auto-queue with reconciliation
ralph watch --auto-queue --close-removed --notify
```

### Recommended Workflows

**Development Workflow**:
```bash
# Terminal 1: Start watch with auto-queue
ralph watch --auto-queue --close-removed

# Terminal 2: Work on code, add TODO/FIXME comments as needed
# Tasks are automatically created and cleaned up
```

**CI/Automation Workflow**:
```bash
# One-time scan for existing comments
ralph watch --auto-queue --patterns "*.rs"

# Or run continuously with reconciliation
ralph watch --auto-queue --close-removed --patterns "*.rs"
```

### Task Lifecycle

1. **Creation**: Comment detected → Task created with `watch` tag and fingerprint
2. **Deduplication**: Same comment (by fingerprint) won't create duplicate tasks
3. **Reconciliation** (with `--close-removed`): Comment removed → Task auto-closed
4. **Manual Override**: Users can manually change task status; watch won't interfere

## `ralph task`

Create tasks and edit task fields from CLI.

Common subcommands:
- `ralph task <request>`: create a task from a freeform request.
- `ralph task --template <name> [target] <request>`: create a task from a template with optional target.
- `ralph task show <TASK_ID>`: show task details (queue + done). Alias: `details`.
- `ralph task status <draft|todo|doing|done|rejected> <TASK_ID>...`: update status for one or more tasks.
- `ralph task edit <FIELD> <VALUE> <TASK_ID>...`: edit any task field (default + custom) for one or more tasks.
- `ralph task field <KEY> <VALUE> <TASK_ID>...`: set one custom field on one or more tasks.
- `ralph task relate <TASK_ID> <RELATION> <OTHER_TASK_ID>`: add a relationship between tasks (blocks, relates_to, duplicates).
- `ralph task blocks <TASK_ID> <BLOCKED_TASK_ID>...`: mark a task as blocking other tasks (shorthand for relate).
- `ralph task mark-duplicate <TASK_ID> <ORIGINAL_TASK_ID>`: mark a task as duplicate of another (shorthand for relate).
- `ralph task batch <operation>`: perform batch operations on multiple tasks efficiently.
- `ralph task update [TASK_ID]`: refresh task fields based on current repo state (omit `TASK_ID` to update all tasks).
- `ralph task template list`: list available templates.
- `ralph task template show <name>`: show template details.
- `ralph task template build <name> [target] <request>`: build a task from a template.
- `ralph task clone <TASK_ID>`: clone an existing task to create a new task from it. Alias: `duplicate`.
- `ralph task split <TASK_ID>`: split a task into multiple child tasks for better granularity.
- `ralph task start <TASK_ID>`: start work on a task (sets `started_at` and moves to doing).
- `ralph task children <TASK_ID>`: list child tasks for a given task.
- `ralph task parent <TASK_ID>`: show the parent task for a given task.

Field formats (for `ralph task edit`):
- Lists (`tags`, `scope`, `evidence`, `plan`, `notes`, `depends_on`): comma/newline-separated.
- `custom_fields`: `key=value` pairs, comma/newline-separated.
- Optional text (`request`, `completed_at`): pass `""` to clear.
- Required timestamps (`created_at`, `updated_at`) must be RFC3339 strings and should not be cleared.

Examples:

```bash
ralph task "Add CLI task edit command"
ralph task status doing RQ-0001
ralph task status doing RQ-0001 RQ-0002 RQ-0003
ralph task status done --tag-filter ready
ralph task edit title "Update queue edit docs" RQ-0001
ralph task edit tags "cli, rust" RQ-0001 RQ-0002
ralph task field severity high RQ-0001 RQ-0002
ralph task edit custom_fields "severity=high, owner=ralph" RQ-0001
ralph task edit request "" RQ-0001

# Batch operations
ralph task batch status doing RQ-0001 RQ-0002 RQ-0003
ralph task batch field priority high --tag-filter urgent
ralph task batch edit tags "reviewed" --tag-filter rust
ralph task batch --dry-run status done --tag-filter ready
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

### ralph task status

Update a task's status (draft, todo, doing, done, rejected). Accepts multiple task IDs for batch updates.

Note: terminal statuses (done, rejected) complete and archive the task(s).

**Arguments:**
- `STATUS` - New status: draft, todo, doing, done, or rejected
- `TASK_ID...` - One or more task IDs to update

**Flags:**
- `--note <NOTE>` - Optional note to append to all affected tasks
- `--tag-filter <TAG>` - Filter tasks by tag for batch operation (alternative to explicit IDs)

**Examples:**
```bash
# Update single task status
ralph task status doing RQ-0001
ralph task status done RQ-0001

# Update multiple tasks at once
ralph task status doing RQ-0001 RQ-0002 RQ-0003

# Update all tasks with a specific tag
ralph task status doing --tag-filter rust

# Update with a note
ralph task status doing --note "Starting work" RQ-0001
```

### ralph task field

Set a custom field on one or more tasks.

**Arguments:**
- `KEY` - Custom field key (must not contain whitespace)
- `VALUE` - Custom field value
- `TASK_ID...` - One or more task IDs to update

**Flags:**
- `--tag-filter <TAG>` - Filter tasks by tag for batch operation (alternative to explicit IDs)

**Examples:**
```bash
# Set custom field on single task
ralph task field severity high RQ-0001

# Set custom field on multiple tasks
ralph task field priority high RQ-0001 RQ-0002 RQ-0003

# Set custom field on all tasks with a specific tag
ralph task field sprint "Sprint 5" --tag-filter backend
```

### ralph task edit

Edit any task field directly. Supports all task fields including custom fields. Accepts multiple task IDs for batch updates.

Fields that can be edited:
- `title` - task title (cannot be empty)
- `status` - draft, todo, doing, done, rejected (empty value cycles to next status)
- `priority` - critical, high, medium, low (empty value cycles to next priority)
- `tags`, `scope`, `evidence`, `plan`, `notes`, `depends_on` - comma/newline-separated lists
- `blocks` - comma/newline-separated list of task IDs this task blocks
- `relates_to` - comma/newline-separated list of related task IDs
- `duplicates` - single task ID this task duplicates (empty value clears)
- `request` - task request description (empty value clears the field)
- `custom_fields` - key=value pairs, comma/newline-separated
- `created_at`, `updated_at`, `completed_at`, `started_at` - RFC3339 timestamps

Flags:
- `--dry-run` - preview changes without modifying the queue
- `--tag-filter <TAG>` - filter tasks by tag for batch operation (alternative to explicit IDs)

Examples:
```bash
# Edit task fields
ralph task edit title "New title" RQ-0001
ralph task edit status doing RQ-0001
ralph task edit priority high RQ-0001
ralph task edit tags "cli, rust" RQ-0001
ralph task edit custom_fields "severity=high, owner=ralph" RQ-0001
ralph task edit request "" RQ-0001

# Edit multiple tasks at once
ralph task edit priority high RQ-0001 RQ-0002 RQ-0003

# Edit all tasks with a specific tag
ralph task edit tags "urgent, reviewed" --tag-filter rust

# Preview changes without applying
ralph task edit --dry-run title "Preview title" RQ-0001
```

### ralph task relate

Add a relationship between two tasks. Relationships provide additional semantic connections beyond dependencies.

**Relationship Types:**
- `blocks` - This task prevents the other task from running until completed. Semantically: "I prevent X" (vs `depends_on`: "I need X").
- `relates_to` - Loose coupling between related tasks. No execution constraint, just semantic association.
- `duplicates` - This task is a duplicate of another task. Singular reference (not a list).

**Arguments:**
- `TASK_ID` - Source task ID (the one establishing the relationship)
- `RELATION` - Relationship type: `blocks`, `relates_to` (or `relates`), `duplicates` (or `duplicate`)
- `OTHER_TASK_ID` - Target task ID

**Examples:**
```bash
# Mark RQ-0001 as blocking RQ-0002
ralph task relate RQ-0001 blocks RQ-0002

# Mark RQ-0001 as related to RQ-0003
ralph task relate RQ-0001 relates_to RQ-0003

# Mark RQ-0001 as duplicate of RQ-0004
ralph task relate RQ-0001 duplicates RQ-0004
```

**Validation:**
- Self-references are rejected (a task cannot block/relate-to/duplicate itself)
- Target tasks must exist in queue or done
- Circular blocking relationships are rejected
- Duplicating a done/rejected task produces a warning

### ralph task blocks

Mark a task as blocking one or more other tasks. This is a shorthand for `ralph task relate <task> blocks <blocked>`.

**Arguments:**
- `TASK_ID` - Task that does the blocking
- `BLOCKED_TASK_ID...` - One or more tasks being blocked

**Examples:**
```bash
# Mark RQ-0001 as blocking RQ-0002
ralph task blocks RQ-0001 RQ-0002

# Mark RQ-0001 as blocking multiple tasks
ralph task blocks RQ-0001 RQ-0002 RQ-0003
```

### ralph task mark-duplicate

Mark a task as a duplicate of another task. This is a shorthand for `ralph task relate <task> duplicates <original>`.

**Arguments:**
- `TASK_ID` - Task to mark as duplicate
- `ORIGINAL_TASK_ID` - Original task this duplicates

**Examples:**
```bash
# Mark RQ-0001 as duplicate of RQ-0002
ralph task mark-duplicate RQ-0001 RQ-0002
```

**Note:** Marking a task as duplicate does not automatically reject or archive it. Use `ralph task reject` if the duplicate should not be worked on.

Perform batch operations on multiple tasks efficiently.

The `batch` subcommand provides a unified interface for updating multiple tasks at once. It supports three operation types: `status`, `field`, and `edit`. By default, batch operations are atomic (all-or-nothing). Use `--continue-on-error` to allow partial success.

**Subcommands:**

- `ralph task batch status <STATUS> [TASK_ID]...` - Update status for multiple tasks
- `ralph task batch field <KEY> <VALUE> [TASK_ID]...` - Set custom field on multiple tasks
- `ralph task batch edit <FIELD> <VALUE> [TASK_ID]...` - Edit any field on multiple tasks

**Flags:**

- `--tag-filter <TAG>` - Filter tasks by tag (repeatable, case-insensitive, OR logic)
- `--dry-run` - Preview changes without modifying the queue
- `--continue-on-error` - Continue processing on individual failures (default: atomic)

**Task Selection:**

Tasks can be selected either by explicit task IDs or by tag filter:
- Explicit IDs: `ralph task batch status doing RQ-0001 RQ-0002 RQ-0003`
- Tag filter: `ralph task batch status doing --tag-filter rust`
- Multiple tags: `ralph task batch status doing --tag-filter rust --tag-filter cli`

When using tag filters, tasks are selected from the active queue only (not done archive). Tag matching is case-insensitive and uses OR logic (any tag matches).

**Atomic vs. Partial Success:**

By default, batch operations are atomic - either all tasks are updated or none are. Validation errors for any task prevent all updates. Use `--continue-on-error` to allow partial success:

```bash
# Atomic mode (default) - fails if any task doesn't exist
ralph task batch status doing RQ-0001 RQ-0002 RQ-0003

# Continue on error - updates valid tasks, reports failures
ralph task batch --continue-on-error status doing RQ-0001 RQ-0002 RQ-9999
```

**Examples:**

```bash
# Update status for multiple tasks by ID
ralph task batch status doing RQ-0001 RQ-0002 RQ-0003

# Update status for all tasks with a specific tag
ralph task batch status doing --tag-filter rust

# Set custom field on multiple tasks
ralph task batch field priority high RQ-0001 RQ-0002

# Edit priority for all urgent tasks
ralph task batch edit priority high --tag-filter urgent

# Preview changes without applying
ralph task batch --dry-run status done --tag-filter ready

# Continue on error (partial success allowed)
ralph task batch --continue-on-error status doing RQ-0001 RQ-0002 RQ-9999
```

### `ralph task batch` (Extended Batch Operations)

Perform batch operations on multiple tasks efficiently. Supports atomic-by-default semantics with `--continue-on-error` for partial success, and `--dry-run` for previewing changes.

**Subcommands:**

- `ralph task batch status <STATUS> [TASK_ID]...` - Update status for multiple tasks
- `ralph task batch field <KEY> <VALUE> [TASK_ID]...` - Set custom field on multiple tasks  
- `ralph task batch edit <FIELD> <VALUE> [TASK_ID]...` - Edit any field on multiple tasks
- `ralph task batch delete [TASK_ID]...` - Delete multiple tasks from the active queue
- `ralph task batch archive [TASK_ID]...` - Archive terminal tasks (Done/Rejected) to done.json
- `ralph task batch clone [TASK_ID]...` - Clone multiple tasks
- `ralph task batch split [TASK_ID]...` - Split multiple tasks into child tasks
- `ralph task batch plan-append [TASK_ID]...` - Append plan items to multiple tasks
- `ralph task batch plan-prepend [TASK_ID]...` - Prepend plan items to multiple tasks

**Global Batch Flags:**

- `--dry-run`: Preview changes without modifying the queue
- `--continue-on-error`: Continue processing on individual task failures (default: atomic/all-or-nothing)

**Task Selection:**

Tasks can be selected either by explicit task IDs or by using filters:
- Explicit IDs: `ralph task batch status doing RQ-0001 RQ-0002 RQ-0003`
- Tag filter: `ralph task batch status doing --tag-filter rust`
- Multiple tags: `ralph task batch status doing --tag-filter rust --tag-filter cli`

**Extended Filters:**

- `--tag-filter <TAG>`: Filter by tag (repeatable, OR logic, case-insensitive)
- `--status-filter <STATUS>`: Filter by status after tag selection (repeatable, OR logic)
- `--priority-filter <PRIORITY>`: Filter by priority (repeatable, OR logic)
- `--scope-filter <PATTERN>`: Filter by scope substring (repeatable, OR logic, case-insensitive)
- `--older-than <WHEN>`: Filter tasks whose updated_at is older than cutoff (e.g., `30d`, `2w`, `2026-01-01`, RFC3339)

**Batch Delete:**

Delete multiple tasks from the active queue:

```bash
# Delete specific tasks
ralph task batch delete RQ-0001 RQ-0002

# Delete tasks by tag filter
ralph task batch delete --tag-filter stale

# Delete old tasks not updated in 30 days
ralph task batch delete --tag-filter backlog --older-than 30d

# Preview before deleting
ralph task batch delete --dry-run --tag-filter obsolete
```

**Batch Archive:**

Archive terminal tasks (Done/Rejected) from active queue to done.json:

```bash
# Archive specific terminal tasks
ralph task batch archive RQ-0001 RQ-0002

# Archive all done tasks with a specific tag
ralph task batch archive --tag-filter done --status-filter done

# Archive old rejected tasks
ralph task batch archive --status-filter rejected --older-than 7d
```

**Batch Clone:**

Clone multiple tasks to create new tasks from existing ones:

```bash
# Clone tasks with a specific tag
ralph task batch clone --tag-filter template

# Clone with custom status and title prefix
ralph task batch clone --tag-filter template --status todo --title-prefix "[Sprint] "

# Clone specific tasks
ralph task batch clone RQ-0001 RQ-0002 --status draft
```

**Batch Split:**

Split multiple tasks into child tasks:

```bash
# Split tasks tagged as 'epic' into 3 children each
ralph task batch split --tag-filter epic --number 3

# Split with plan distribution
ralph task batch split --tag-filter epic --number 3 --distribute-plan

# Split specific tasks with custom status
ralph task batch split RQ-0001 RQ-0002 --number 2 --status todo
```

**Batch Plan Operations:**

Append or prepend plan items to multiple tasks:

```bash
# Append plan items to tasks
ralph task batch plan-append --tag-filter rust --plan-item "Run make ci" --plan-item "Update docs"

# Prepend plan items to specific task
ralph task batch plan-prepend RQ-0001 --plan-item "Confirm repro" --plan-item "Check related issues"

# Add steps to all todo tasks
ralph task batch plan-append --status-filter todo --plan-item "Run tests before committing"
```

**Combined Filter Examples:**

```bash
# Complex filter: high priority rust tasks not updated in 2 weeks
ralph task batch status doing \\
  --tag-filter rust \\
  --priority-filter high \\
  --older-than 14d

# Scope-based filtering
ralph task batch field priority low \\
  --scope-filter "legacy" \\
  --status-filter todo

# Multiple filters combined
ralph task batch archive \\
  --tag-filter maintenance \\
  --status-filter done \\
  --older-than 30d
```

**Atomic vs Continue-on-Error:**

```bash
# Atomic mode (default) - fails if any task doesn't exist
ralph task batch status doing RQ-0001 RQ-0002 RQ-0003

# Continue on error - updates valid tasks, reports failures
ralph task batch --continue-on-error status doing RQ-0001 RQ-0002 RQ-9999
```

**Examples:**

```bash
# Update status for multiple tasks by ID
ralph task batch status doing RQ-0001 RQ-0002 RQ-0003

# Update status for all tasks with a specific tag
ralph task batch status doing --tag-filter rust

# Set custom field on multiple tasks
ralph task batch field priority high RQ-0001 RQ-0002

# Edit priority for all urgent tasks
ralph task batch edit priority high --tag-filter urgent

# Preview changes without applying
ralph task batch --dry-run status done --tag-filter ready

# Continue on error (partial success allowed)
ralph task batch --continue-on-error status doing RQ-0001 RQ-0002 RQ-9999

# Delete tasks matching filter
ralph task batch delete --tag-filter obsolete --older-than 30d

# Clone template tasks for new sprint
ralph task batch clone --tag-filter template --status todo --title-prefix "[Sprint 5] "

# Split epics into smaller tasks
ralph task batch split --tag-filter epic --number 3 --distribute-plan

# Add verification step to all rust tasks
ralph task batch plan-append --tag-filter rust --plan-item "Run cargo clippy"
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
- `scope` - Scope is a starting point, not a restriction; list relevant paths/commands and expand as needed
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

**Template Validation:**

By default, unknown template variables are left as-is with a warning. Use `--strict-templates` to fail on unknown variables (useful in CI to catch typos).

* `--strict-templates`: Fail if the template contains unknown `{{variables}}`. When disabled (default), unknown variables are left as-is with a warning.

Unknown variables produce warnings like:
```
Warning: Template 'custom': Unknown template variable: {{unknown_var}}
```

Git branch detection failures also produce warnings when `{{branch}}` is used:
```
Warning: Template 'custom': Git branch detection failed: not a git repository
```

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

## `ralph productivity`

View productivity analytics including streaks, velocity metrics, and milestone achievements. Stats are persisted to `.ralph/cache/productivity.json`.

### Subcommands

* `summary`: show total completions, current streak, milestones, and recent completions.
* `velocity`: show tasks per day over a configurable window.
* `streak`: show current and longest streak details.

### Flags

All subcommands support:
* `--format <text|json>`: output format (default: `text`).

Subcommand-specific flags:
* `summary`: `--recent <N>` - number of recent completions to show (default: 5).
* `velocity`: `--days <N>` - window size in days (default: 7).

### Examples

```bash
# Show productivity summary
ralph productivity summary

# Show summary with more recent completions
ralph productivity summary --recent 10

# Show velocity for last 14 days
ralph productivity velocity --days 14

# Show streak info
ralph productivity streak

# JSON output for scripting
ralph productivity summary --format json
ralph productivity velocity --format json
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

## `ralph watch`

Watch files for changes and auto-detect tasks from TODO/FIXME/HACK/XXX comments.

The watch command monitors source files and automatically creates tasks when it detects comment markers like `TODO`, `FIXME`, `HACK`, or `XXX`. It uses structured metadata (file path, line number, content fingerprint) for reliable deduplication and lifecycle tracking.

### Key Features

* **Structured Metadata**: Watch-created tasks include `custom_fields` with:
  * `watch.file` - Absolute path to the source file
  * `watch.line` - Line number of the comment
  * `watch.comment_type` - Type of comment (todo, fixme, hack, xxx)
  * `watch.fingerprint` - SHA256 hash of normalized comment content
  * `watch.version` - Metadata format version

* **Strong Deduplication**: Uses content fingerprinting to avoid creating duplicate tasks for the same comment, even if the file is moved or line numbers change.

* **Auto-Close on Removal**: With `--close-removed`, tasks are automatically marked done when their originating comments are deleted from source files.

### Flags

* `--patterns <PATTERNS>` - File patterns to watch (comma-separated, default: `*.rs,*.ts,*.js,*.py,*.go,*.java,*.md,*.toml,*.json`)
* `--debounce-ms <MS>` - Debounce duration in milliseconds (default: 500)
* `--auto-queue` - Automatically create tasks without prompting
* `--notify` - Enable desktop notifications for new tasks
* `--comments <TYPES>` - Comment types to detect: `todo`, `fixme`, `hack`, `xxx`, `all` (default: `all`)
* `--ignore-patterns <PATTERNS>` - Additional gitignore-style exclusions (comma-separated)
* `--close-removed` - Automatically mark watch tasks as done when their originating comments are removed

### Examples

```bash
# Basic watch mode (suggests tasks, doesn't auto-create)
ralph watch

# Watch specific directories
ralph watch src/ tests/

# Auto-queue tasks without prompting
ralph watch --auto-queue

# Watch with desktop notifications
ralph watch --auto-queue --notify

# Only detect TODO and FIXME comments
ralph watch --comments todo,fixme

# Auto-close tasks when comments are removed
ralph watch --auto-queue --close-removed

# Custom patterns and debounce
ralph watch --patterns "*.rs,*.toml" --debounce-ms 1000

# Ignore vendor directories
ralph watch --ignore-patterns "vendor/,target/,node_modules/"
```

### Recommended Workflows

**Development Workflow with Auto-Close:**
```bash
# Terminal 1: Start watch with auto-queue and auto-close
ralph watch --auto-queue --close-removed

# Terminal 2: Work on code - tasks auto-create from TODOs and auto-close when resolved
```

**Code Review Cleanup:**
```bash
# After refactoring, run with close-removed to clean up stale tasks
ralph watch src/ --close-removed --auto-queue
```

### Task Lifecycle

Watch-created tasks follow this lifecycle:

1. **Detection**: File change triggers comment scanning
2. **Deduplication**: Fingerprint check prevents duplicate tasks
3. **Creation**: Task added to queue with `watch` tag and metadata
4. **Execution**: Task worked on normally via `ralph run` or TUI
5. **Reconciliation** (with `--close-removed`): If comment is deleted, task auto-completes

### Notes

* Watch tasks are tagged with `watch` and the comment type (e.g., `todo`, `fixme`)
* Deduplication uses SHA256 fingerprint of normalized comment content
* Legacy tasks without structured metadata fall back to file/line matching
* The `--close-removed` flag only affects watch-created tasks (those with `watch` tag)
* User-authored tasks without the `watch` tag are never modified by the reconciliation logic

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
ralph run one --runner codex --model gpt-5.3-codex --effort high
```

## Run-Specific Flags

The `run one` and `run loop` commands also support:

* `--include-draft`: Include draft tasks (`status: draft`) when selecting what to run.
* `--non-interactive`: Skip interactive prompts for sanity checks and session recovery (conflicts with `--interactive`). Useful for CI environments where TTY is not available.
* `--parallel [N]` (run loop only): Run tasks concurrently in isolated git workspace clones. Defaults to `2` when provided without a value. Conflicts with `--interactive` and is CLI-only (not supported in the TUI). Parallel workers do not modify `.ralph/queue.json` or `.ralph/done.json`; they commit completion signals in `.ralph/cache/completions/<TASK_ID>.json`, and the coordinator applies them after merge (errors if missing). Parallel workers force RepoPrompt mode to `off` to keep edits inside workspace clones. Parallel workers honor `--git-commit-push-on/off` and `agent.git_commit_push_enabled`; when commit/push is disabled, parallel PR automation is skipped.
* `--update-task`: Automatically run `ralph task update <TASK_ID>` once per task immediately before the supervisor marks the task as `doing` and starts execution. This updates task fields (scope, evidence, plan, notes, tags, depends_on) based on current repository state, priming agents with better task information. Runs only once per task, before the first iteration (not before subsequent iterations if `iterations > 1`). Can also be enabled via config: `agent.update_task_before_run: true`.
* `--no-update-task`: Disable automatic pre-run task update for this invocation (overrides config).
* `--notify`: Enable desktop notification on task completion (overrides config).
* `--no-notify`: Disable desktop notification on task completion (overrides config).
* `--notify-fail`: Enable desktop notification on task failure (overrides config).
* `--no-notify-fail`: Disable desktop notification on task failure (overrides config).
* `--notify-sound`: Enable sound alert with notification (works with notification flags or when enabled in config).
* `--git-revert-mode <ask|enabled|disabled>`
* `--git-commit-push-on` / `--git-commit-push-off`
* `--debug` (capture raw supervisor + runner output to `.ralph/logs/debug.log`; also enables raw safeguard dumps)

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
ralph run loop --parallel --max-tasks 4
ralph run loop --parallel 4 --max-tasks 8
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

1. **Environment variable**: `RALPH_RAW_DUMP=1` or `RALPH_RAW_DUMP=true`
2. **Debug mode**: `--debug` flag (implies you want verbose/raw output)

```bash
# Redacted dumps (default) - secrets are masked
ralph run one
ralph run one --profile quick
ralph run one --profile thorough
ralph run one --profile quick --runner claude  # CLI overrides profile

# Raw dumps with env var - secrets written to disk
RALPH_RAW_DUMP=1 ralph run one
ralph run one --profile quick
ralph run one --profile thorough
ralph run one --profile quick --runner claude  # CLI overrides profile
RALPH_RAW_DUMP=true ralph run one
ralph run one --profile quick
ralph run one --profile thorough
ralph run one --profile quick --runner claude  # CLI overrides profile

# Raw dumps via debug mode - secrets in debug.log and dumps
ralph run one --debug
```

### Security Considerations

- **Redaction is best-effort**: Pattern-based redaction may miss secrets in unexpected formats, encoded data, or novel patterns. Always review output before sharing.
- **Never commit safeguard dumps** to version control. They may contain sensitive data even when redacted.
- **Debug mode (`--debug`)** writes raw runner output to `.ralph/logs/debug.log`. This is intentional for troubleshooting but may contain secrets. Debug logs capture raw output before redaction is applied.
- **Never commit debug logs** to version control (ensure `.ralph/logs/` is in your repo root `.gitignore`).
- Temp directories for safeguard dumps are created under `/tmp/ralph/` (or platform equivalent) with `ralph_` prefixes. Clean these up periodically as they persist until manually removed.

## `ralph plugin`

Manage Ralph plugins (runners and task processors).

### Subcommands

- `ralph plugin init <PLUGIN_ID>`: Scaffold a new plugin directory with plugin.json and optional scripts
- `ralph plugin list`: List discovered plugins and their status
- `ralph plugin validate`: Validate plugin manifests and binaries
- `ralph plugin install <SOURCE> --scope <project|global>`: Install a plugin from a local directory
- `ralph plugin uninstall <PLUGIN_ID> --scope <project|global>`: Uninstall a plugin

### `ralph plugin init`

Scaffold a new plugin directory. This is the recommended starting point for plugin development.

Flags:

- `--scope <project|global>`: Where to create the plugin (default: `project`)
- `--path <DIR>`: Explicit target directory (overrides `--scope`)
- `--name <NAME>`: Manifest name (default: derived from plugin ID)
- `--version <SEMVER>`: Manifest version (default: `0.1.0`)
- `--description <TEXT>`: Optional manifest description
- `--with-runner`: Include runner stub + manifest section
- `--with-processor`: Include processor stub + manifest section
- `--dry-run`: Preview what would be written without creating files
- `--force`: Overwrite existing directory

By default, both runner and processor scripts are created. If either `--with-runner` or `--with-processor` is specified, only the requested capability is scaffolded.

Examples:

```bash
# Scaffold a plugin with both runner and processor
ralph plugin init acme.super_runner

# Scaffold with only runner support
ralph plugin init acme.super_runner --with-runner

# Scaffold with only processor support
ralph plugin init acme.super_runner --with-processor

# Scaffold in global scope
ralph plugin init acme.super_runner --scope global

# Scaffold with custom metadata
ralph plugin init acme.super_runner --name "Super Runner" --version "1.0.0"

# Preview without writing
ralph plugin init acme.super_runner --dry-run
```

Important notes:

- The plugin is NOT automatically enabled. Add to config: `{"plugins": {"plugins": {"acme.super_runner": {"enabled": true}}}}`
- Project scope plugins are created in `.ralph/plugins/<plugin_id>/`
- Global scope plugins are created in `~/.config/ralph/plugins/<plugin_id>/`

### `ralph plugin list`

List discovered plugins (both global and project scope) and whether they are enabled.

Flags:

- `--json`: Output as JSON

Examples:

```bash
ralph plugin list
ralph plugin list --json
```

### `ralph plugin validate`

Validate plugin manifests and check that referenced executables exist.

Flags:

- `--id <PLUGIN_ID>`: Validate only a specific plugin

Examples:

```bash
# Validate all discovered plugins
ralph plugin validate

# Validate specific plugin
ralph plugin validate --id acme.super_runner
```

### `ralph plugin install`

Install a plugin from a local directory (must contain plugin.json).

Flags:

- `--scope <project|global>`: Install scope (default: `project`)

Examples:

```bash
ralph plugin install ./my-plugin --scope project
ralph plugin install ./my-plugin --scope global
```

Note: Install does NOT auto-enable the plugin. Enable manually in config.

### `ralph plugin uninstall`

Uninstall a plugin by ID from the chosen scope.

Flags:

- `--scope <project|global>`: Uninstall scope (default: `project`)

Examples:

```bash
ralph plugin uninstall acme.super_runner --scope project
```

## Help Output

For the full, authoritative list of flags and examples, run:

```bash
ralph --help
ralph tui --help
ralph queue --help
ralph run --help
ralph plugin --help
ralph plugin init --help
```
