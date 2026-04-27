# Ralph Daemon Mode
Status: Active
Owner: Maintainers
Source of truth: this document for daemon-mode commands, behavior, and service setup
Parent: [Daemon and Watch](../daemon-and-watch.md)

This guide covers Ralph daemon mode: background task execution, lifecycle commands, process management, service setup, continuous waiting, and graceful shutdown.

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

## See Also

- [Daemon and Watch overview](../daemon-and-watch.md)
- [CLI Reference](../../cli.md)
- [Queue and Tasks](../../queue-and-tasks.md)
- [Watch Mode](./watch.md)
- [Operations](./operations.md)
- [Troubleshooting](./troubleshooting.md)
