# Daemon Mode and Watch Mode

![Daemon & Watch Mode](../assets/images/2026-02-07-11-32-24-daemon-watch.png)

Comprehensive guide to Ralph's background execution capabilities: **Daemon Mode** for continuous task processing and **Watch Mode** for automatic task detection from code comments.

---

## Daemon Mode

### Overview

Daemon mode runs Ralph as a background service that continuously monitors the task queue and executes tasks automatically. It's designed for "set and forget" operation, ideal for:

- Long-running development sessions
- CI/CD integration
- Server environments
- Automated workflows

**INTENDED BEHAVIOR**: Run `ralph run loop --continuous --wait-when-blocked` as a detached background process with proper logging and lifecycle management.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Full implementation on Unix systems (Linux, macOS). Windows requires manual service configuration or running `ralph run loop --continuous` in a terminal.

> **Platform Support**: Daemon mode is Unix-only. On Windows, use `ralph run loop --continuous` directly or configure a Windows service.

---

### Commands

#### `ralph daemon start`

Start Ralph as a background daemon.

```bash
# Start with default settings
ralph daemon start

# Start with custom poll intervals
ralph daemon start --empty-poll-ms 5000 --wait-poll-ms 500

# Start with notifications when unblocked
ralph daemon start --notify-when-unblocked
```

**Flags:**

| Flag | Default | Description |
|------|---------|-------------|
| `--empty-poll-ms` | 30000 | Poll interval (ms) when queue is empty |
| `--wait-poll-ms` | 1000 | Poll interval (ms) when waiting for blocked tasks |
| `--notify-when-unblocked` | false | Enable notifications when queue becomes unblocked |

**Behavior on Start:**

1. Checks if daemon is already running (prevents duplicate instances)
2. Acquires daemon lock at `.ralph/cache/daemon.lock`
3. Creates log directory `.ralph/logs/`
4. Spawns detached process running `ralph daemon serve`
5. Writes daemon state to `.ralph/cache/daemon.json`
6. Validates successful startup (waits 500ms, checks state file)

#### `ralph daemon stop`

Stop the daemon gracefully.

```bash
ralph daemon stop
```

**Behavior:**

1. Reads daemon state from `.ralph/cache/daemon.json`
2. Verifies process is running (handles stale state files)
3. Creates stop signal at `.ralph/cache/stop_requested`
4. Waits up to 10 seconds for graceful shutdown
5. Cleans up state files on successful stop

> **Note**: If the daemon doesn't stop within 10 seconds, you'll need to manually kill it with `kill -9 <PID>`.

#### `ralph daemon status`

Check daemon status.

```bash
ralph daemon status
```

**Output States:**

| State | Description |
|-------|-------------|
| `running` | Daemon is active (shows PID, start time, command) |
| `stopped` | No daemon is running |
| `stale` | State file exists but process is dead (auto-cleaned) |

**Example Output:**
```
Daemon is running
  PID: 12345
  Started: 2026-02-07T10:30:00Z
  Command: ralph daemon serve --empty-poll-ms 30000 --wait-poll-ms 1000
```

---

### Behavior

#### Detached Process

The daemon uses Unix session management to fully detach from the terminal:

```rust
// From crates/ralph/src/commands/daemon/mod.rs
unsafe {
    command.pre_exec(|| {
        libc::setsid();  // Create new session, detach from terminal
        Ok(())
    });
}
```

This ensures:
- Continues running after terminal closes
- No SIGHUP on terminal disconnect
- Independent process group

#### Logging

Daemon output is redirected to `.ralph/logs/daemon.log`:

```bash
# View daemon logs
tail -f .ralph/logs/daemon.log

# Search for errors
grep ERROR .ralph/logs/daemon.log
```

Log contents include:
- Task execution progress
- Phase transitions
- CI gate results
- Errors and warnings
- Loop lifecycle events

#### PID Management

The daemon maintains state in `.ralph/cache/daemon.json`:

```json
{
  "version": 1,
  "pid": 12345,
  "started_at": "2026-02-07T10:30:00Z",
  "repo_root": "/path/to/repo",
  "command": "ralph daemon serve --empty-poll-ms 30000"
}
```

**Lock File**: `.ralph/cache/daemon.lock` prevents multiple daemons from starting simultaneously.

---

### Configuration

Daemon behavior can be configured via CLI flags:

#### Poll Intervals

| Scenario | Flag | Default | Minimum |
|----------|------|---------|---------|
| Empty queue | `--empty-poll-ms` | 30000ms (30s) | 50ms |
| Blocked tasks | `--wait-poll-ms` | 1000ms (1s) | 50ms |

**Tuning Guidelines:**

- **Faster response**: Lower `--empty-poll-ms` to 5000ms for quicker task pickup
- **Lower CPU**: Increase `--wait-poll-ms` to 5000ms if polling overhead matters
- **Battery life**: Use higher intervals on laptops for longer battery life

#### Notifications

Enable notifications when blocked tasks become runnable:

```bash
ralph daemon start --notify-when-unblocked
```

This triggers:
- Desktop notifications (if supported)
- Webhook events (`queue_unblocked`)

**Webhook Event Format:**

```json
{
  "event": "queue_unblocked",
  "previous_status": "blocked",
  "current_status": "runnable",
  "note": "ready=2 blocked_deps=3 blocked_schedule=1"
}
```

---

### Service Templates

#### systemd (Linux)

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

**Enable and start:**

```bash
# Reload systemd configuration
systemctl --user daemon-reload

# Enable service to start on boot
systemctl --user enable ralph

# Start the service
systemctl --user start ralph

# Check status
systemctl --user status ralph

# View logs
journalctl --user -u ralph -f
```

#### launchd (macOS)

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

**Load and start:**

```bash
# Load the plist
launchctl load ~/Library/LaunchAgents/com.ralph.daemon.plist

# Start the service
launchctl start com.ralph.daemon

# Check status
launchctl list | grep com.ralph.daemon

# Unload when needed
launchctl unload ~/Library/LaunchAgents/com.ralph.daemon.plist
```

---

### Continuous Mode

Continuous mode keeps the daemon running indefinitely, waiting for new tasks when the queue is empty.

**INTENDED BEHAVIOR**: Use filesystem notifications (`notify` crate) to watch `.ralph/queue.jsonc` and `.ralph/done.jsonc`, falling back to polling if notifications fail.

**CURRENTLY IMPLEMENTED BEHAVIOR**: The run loop uses a poll-based approach when `--wait-when-empty` is enabled. The filesystem notification optimization may not be fully implemented in all code paths.

**Activation:**

```bash
# Via daemon (always uses continuous mode)
ralph daemon start

# Via run loop directly
ralph run loop --continuous
# or
ralph run loop --wait-when-empty
```

**Characteristics:**

- No timeout (runs until stopped)
- Respects stop signals (`ralph queue stop`, `ralph daemon stop`)
- Responds to Ctrl+C when running in foreground
- Polls at `--empty-poll-ms` interval

---

### Wait When Blocked

When all remaining tasks are blocked by unmet dependencies (`depends_on`) or future schedules (`scheduled_start`), the daemon can wait instead of exiting.

**Activation:**

```bash
# Daemon always uses this
ralph daemon start

# Manual run loop
ralph run loop --wait-when-blocked
```

**Behavior:**

1. Polls queue files at `--wait-poll-ms` interval
2. Detects when blocked tasks become runnable:
   - Dependencies completed (checked in `.ralph/done.jsonc`)
   - Schedule time reached
3. Resumes execution automatically
4. Optional timeout via `--wait-timeout-seconds`

**Example:**

```bash
# Wait for blocked tasks with 10-minute timeout
ralph run loop --wait-when-blocked --wait-timeout-seconds 600 --notify-when-unblocked
```

---

### Graceful Shutdown

The daemon implements graceful shutdown through file-based signaling.

**Mechanism:**

1. **Stop Signal**: File at `.ralph/cache/stop_requested`
2. **Signal Creation**: `ralph daemon stop` or `ralph queue stop` creates this file
3. **Signal Polling**: Run loop checks for signal presence between tasks
4. **Cleanup**: Signal file is cleared at loop start (handles stale signals from crashes)

**Signal File Format:**

```
Stop requested at 2026-02-07T10:30:00Z
```

**Shutdown Sequence:**

```
1. Stop signal created
2. Current task completes (if any)
3. Loop checks signal, exits cleanly
4. Daemon removes state file
5. Lock file released
6. Process exits
```

**Timeout Handling:**

If graceful shutdown fails after 10 seconds, the stop command reports:

```
Daemon did not stop within 10 seconds. PID: 12345. 
You may need to kill it manually with `kill -9 12345`
```

---

## Watch Mode

### Overview

Watch mode monitors source files for changes and automatically creates tasks from actionable code comments (`TODO`, `FIXME`, `HACK`, `XXX`).

**Use Cases:**

- Capture technical debt as you code
- Ensure TODOs don't get forgotten
- Track refactoring ideas
- Monitor code review comments

**INTENDED BEHAVIOR**: Detect comments in real-time as files change, with intelligent deduplication and optional auto-closing when comments are removed.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Full implementation with fingerprint-based deduplication, debounced file processing, and comment reconciliation.

---

### Commands

```bash
# Basic watch mode (suggests tasks, doesn't create)
ralph watch

# Watch specific directories
ralph watch src/ tests/

# Auto-create tasks without prompting
ralph watch --auto-queue

# Watch with custom patterns
ralph watch --patterns "*.rs,*.toml"

# Only detect TODO and FIXME
ralph watch --comments todo,fixme

# Enable desktop notifications
ralph watch --auto-queue --notify

# Auto-close tasks when comments are removed
ralph watch --auto-queue --close-removed

# Custom debounce interval
ralph watch --debounce-ms 1000

# Ignore additional patterns
ralph watch --ignore-patterns "vendor/,target/,node_modules/"
```

---

### Triggers

Watch mode triggers on file system events that indicate potential changes to source files.

**INTENDED BEHAVIOR**: React to all meaningful file modifications (create, modify, delete) that could contain actionable comments.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Uses the `notify` crate with `RecommendedWatcher` to monitor file events. The watcher operates in:

- **Recursive mode** for directories
- **Non-recursive mode** for individual files

**Event Types Processed:**

| Event | Action |
|-------|--------|
| File created | Scan for comments |
| File modified | Scan for comments |
| File deleted | Trigger reconciliation (with `--close-removed`) |

**Supported Comment Types:**

| Type | Pattern Example |
|------|-----------------|
| TODO | `// TODO: fix error handling` |
| FIXME | `// FIXME: this is broken` |
| HACK | `// HACK: temporary workaround` |
| XXX | `// XXX: review this before release` |

**Pattern Matching:**

Comments are matched case-insensitively with flexible separators:

```rust
// All these match TODO pattern:
// TODO: fix this
// TODO - fix this
// TODO; fix this
// TODO fix this
```

---

### Debouncing

Watch mode implements debouncing to batch rapid file changes and avoid redundant processing.

**INTENDED BEHAVIOR**: Wait for a quiet period after file changes before processing, to batch multiple rapid changes into a single scan.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Two-layer debouncing system:

1. **Event-level debouncing**: `WatchState` tracks pending files and only processes when `debounce_duration` has elapsed since the last event
2. **File-level debouncing**: Individual files can't be reprocessed within the debounce window

**Configuration:**

```bash
# Default debounce: 500ms
ralph watch

# Faster response (lower latency, more CPU)
ralph watch --debounce-ms 100

# Slower response (higher latency, less CPU)
ralph watch --debounce-ms 2000
```

**Implementation Details:**

```rust
// From crates/ralph/src/commands/watch/state.rs
pub struct WatchState {
    pub pending_files: HashSet<PathBuf>,
    pub last_event: Instant,
    pub debounce_duration: Duration,
}
```

The event loop:
1. Collects file paths from watch events
2. Adds to `pending_files` set (deduplicates)
3. Updates `last_event` timestamp
4. Processes when `now - last_event >= debounce_duration`

---

### Configuration

#### File Patterns

Default patterns: `*.rs,*.ts,*.js,*.py,*.go,*.java,*.md,*.toml,*.json`

**Custom patterns:**

```bash
# Only Rust and TOML files
ralph watch --patterns "*.rs,*.toml"

# Include YAML and config files
ralph watch --patterns "*.rs,*.yaml,*.yml,*.conf"

# Watch all source files
ralph watch --patterns "*"
```

**Pattern Syntax:**

Uses `globset` for pattern matching:

| Pattern | Matches |
|---------|---------|
| `*.rs` | All Rust files |
| `test_*.py` | Python test files |
| `file[0-9].txt` | file1.txt, file2.txt, etc. |
| `*.{js,ts}` | JavaScript and TypeScript files |

#### Ignore Patterns

Built-in ignores (always applied):
- `/target/`
- `/node_modules/`
- `/.git/`
- `/vendor/`
- `/.ralph/`

**Custom ignores:**

```bash
# Ignore generated files
ralph watch --ignore-patterns "generated/,dist/,build/"

# Ignore specific directories
ralph watch --ignore-patterns "legacy/,third_party/"
```

#### Comment Type Selection

```bash
# All comment types (default)
ralph watch --comments all

# Only TODO
ralph watch --comments todo

# TODO and FIXME only
ralph watch --comments todo,fixme

# Everything except XXX
ralph watch --comments todo,fixme,hack
```

---

### Deduplication

Watch mode uses fingerprint-based deduplication to prevent duplicate tasks.

**INTENDED BEHAVIOR**: Never create duplicate tasks for the same comment, even if the file is moved or line numbers change.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Two-tier deduplication system:

1. **Fingerprint-based** (primary): SHA256 hash of normalized comment content
2. **Location-based** (fallback): File path + line number matching

**Fingerprint Generation:**

```rust
// From crates/ralph/src/commands/watch/tasks.rs
pub fn generate_comment_fingerprint(content: &str) -> String {
    let normalized = content.to_lowercase().trim().to_string();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[..16].to_string()  // First 16 hex chars
}
```

**Deduplication Logic:**

1. Check if task with matching `watch.fingerprint` exists
2. Fallback to `watch.file` + `watch.line` match
3. Legacy fallback: Check task title/notes for file path references

**Stable Across:**
- File moves (fingerprint unchanged)
- Line number changes (fingerprint unchanged)
- Whitespace changes (normalization handles this)
- Case changes (normalization handles this)

---

### Comment Reconciliation

With `--close-removed`, watch mode automatically closes tasks when their originating comments are deleted.

**INTENDED BEHAVIOR**: Keep the task queue in sync with actual code comments.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Full reconciliation system that:

1. Tracks all detected comments across watched files
2. Compares against existing watch-tagged tasks
3. Marks tasks as `done` when comments are removed
4. Adds explanatory note with timestamp

**Reconciliation Process:**

```rust
// Pseudocode of reconciliation logic
for task in queue.tasks:
    if not task.tags.contains("watch"): continue
    if task.status in [Done, Rejected]: continue
    
    comment_exists = check_fingerprint(task) || check_location(task)
    
    if not comment_exists:
        task.status = Done
        task.completed_at = now()
        task.notes.push("Auto-closed: originating comment was removed")
```

**Example:**

```bash
# Terminal 1: Start watch with auto-close
ralph watch --auto-queue --close-removed

# Terminal 2: Work on code
# 1. Add "// TODO: refactor this" → Task auto-created
# 2. Complete the refactoring
# 3. Remove the TODO comment → Task auto-closed
```

---

### Task Metadata

Watch-created tasks include structured metadata in `custom_fields`:

```json
{
  "custom_fields": {
    "watch.file": "/absolute/path/to/file.rs",
    "watch.line": "42",
    "watch.comment_type": "todo",
    "watch.fingerprint": "a1b2c3d4e5f67890",
    "watch.version": "1"
  },
  "tags": ["watch", "todo"]
}
```

**Fields:**

| Field | Description |
|-------|-------------|
| `watch.file` | Absolute path to source file |
| `watch.line` | Line number of comment |
| `watch.comment_type` | Comment type (todo/fixme/hack/xxx) |
| `watch.fingerprint` | SHA256 prefix for deduplication |
| `watch.version` | Metadata format version |

---

### Use Cases

#### Development Workflow

```bash
# Terminal 1: Start watch with auto-queue and auto-close
ralph watch --auto-queue --close-removed

# Terminal 2: Regular development
# - Add TODO/FIXME comments as you code
# - Tasks automatically created
# - Complete work and remove comments
# - Tasks automatically closed
```

#### Code Review Cleanup

```bash
# After code review, scan for new action items
ralph watch src/ --auto-queue --patterns "*.rs"

# Clean up completed items
ralph watch src/ --close-removed --auto-queue
```

#### CI Integration

```bash
# Scan for existing TODOs in CI pipeline
ralph watch --auto-queue --patterns "*.rs" --comments todo,fixme

# Or run continuously in container
ralph watch --auto-queue --close-removed --patterns "*.rs"
```

#### Project Health Monitoring

```bash
# Generate report of all open watch tasks
ralph queue list --tag watch --format json | jq '.tasks | group_by(.custom_fields."watch.comment_type") | map({type: .[0].custom_fields."watch.comment_type", count: length})'
```

---

## Combined Usage Patterns

### Full Auto-Development Setup

Run both daemon and watch for fully automated task management:

```bash
# Terminal 1: Start daemon for task execution
ralph daemon start --notify-when-unblocked

# Terminal 2: Start watch for task detection
ralph watch --auto-queue --close-removed --notify

# Terminal 3: Regular development
# - Add TODOs → Tasks auto-created
# - Daemon executes tasks
# - Complete work → Tasks auto-closed
```

### Service-Based Setup

```bash
# Start daemon as user service
systemctl --user start ralph

# Run watch in tmux/screen session
tmux new-session -d -s ralph-watch "ralph watch --auto-queue --close-removed"

# Check on it later
tmux attach -t ralph-watch
```

---

## Troubleshooting

### Daemon Issues

| Issue | Solution |
|-------|----------|
| `Daemon is already running` | Run `ralph daemon status` to verify, then `ralph daemon stop` if needed |
| `Daemon failed to start` | Check `.ralph/logs/daemon.log` for errors |
| Stale state file | Run `ralph daemon status` to auto-clean, or manually remove `.ralph/cache/daemon.json` |
| Won't stop gracefully | Use `kill -9 <PID>` as last resort |

### Watch Issues

| Issue | Solution |
|-------|----------|
| Not detecting changes | Check `--patterns` match your files; verify file is in watched path |
| Too many tasks created | Enable deduplication by ensuring tasks have `watch.fingerprint` |
| High CPU usage | Increase `--debounce-ms` to reduce processing frequency |
| Missing comments | Check `--comments` includes the types you're using |

### Common Patterns

```bash
# Check daemon logs
tail -f .ralph/logs/daemon.log

# Verify watch is detecting files
ralph watch --patterns "*.rs"  # Run interactively to see output

# Clean up and restart
ralph daemon stop
rm -f .ralph/cache/daemon.json
rm -f .ralph/cache/stop_requested
ralph daemon start
```

---

## See Also

- [`docs/workflow.md`](../workflow.md) - General workflow documentation
- [`docs/cli.md`](../cli.md) - Complete CLI reference
- [`docs/queue-and-tasks.md`](../queue-and-tasks.md) - Task management details
