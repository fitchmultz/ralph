# Ralph Notification System

![Notification System](../assets/images/2026-02-07-11-32-24-notifications.png)

Ralph provides a cross-platform desktop notification system that alerts you when tasks complete, fail, or when loop execution finishes. Notifications help you stay informed about long-running tasks without constantly monitoring the terminal.

---

## Overview

The notification system delivers desktop alerts for key task lifecycle events:

- **Task Completion**: Success notifications when a task finishes
- **Task Failure**: Alert notifications when a task fails or is rejected
- **Loop Completion**: Summary notifications when batch task execution completes
- **Watch Mode**: Notifications when new tasks are detected from code comments

Notifications are designed to be:
- **Non-intrusive**: Failures are logged but never interrupt execution
- **UI-aware**: Automatically suppressed when the macOS app is active (configurable)
- **Cross-platform**: Native notifications on macOS, Linux, and Windows
- **Optional sound**: Audible alerts can accompany visual notifications

---

## Configuration

Notifications are configured via the `agent.notification` section in your config file (`.ralph/config.jsonc` or `~/.config/ralph/config.jsonc`).

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `true` | Legacy master switch for notifications (backward compatibility) |
| `notify_on_complete` | boolean | `true` | Enable notifications when tasks complete successfully |
| `notify_on_fail` | boolean | `true` | Enable notifications when tasks fail |
| `notify_on_loop_complete` | boolean | `true` | Enable notifications when loop mode finishes |
| `suppress_when_active` | boolean | `true` | Suppress notifications when the macOS app is active |
| `sound_enabled` | boolean | `false` | Play sound with notifications |
| `sound_path` | string | `null` | Custom sound file path (platform-specific) |
| `timeout_ms` | number | `8000` | Notification display duration (1000-60000 ms) |

### Configuration Precedence

Settings are resolved in this order (highest to lowest):

1. **CLI flags** (`--notify`, `--no-notify`, etc.)
2. **Task-level overrides** (if supported in future versions)
3. **Project config** (`.ralph/config.jsonc`)
4. **Global config** (`~/.config/ralph/config.jsonc`)
5. **Built-in defaults**

### Basic Configuration Example

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "notify_on_loop_complete": true,
      "suppress_when_active": true,
      "sound_enabled": false,
      "timeout_ms": 8000
    }
  }
}
```

### Advanced Configuration with Custom Sound

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "notify_on_loop_complete": true,
      "suppress_when_active": true,
      "sound_enabled": true,
      "sound_path": "/Users/me/sounds/complete.aiff",
      "timeout_ms": 10000
    }
  }
}
```

### Minimal Configuration (Failures Only)

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "notify_on_complete": false,
      "notify_on_fail": true,
      "sound_enabled": true
    }
  }
}
```

---

## Platform Support

Ralph uses the `notify-rust` crate for cross-platform notification delivery, with platform-specific adaptations for sound playback.

### macOS

**Notification Delivery:**
- Uses NotificationCenter framework
- Native macOS notification appearance
- Respects system Do Not Disturb settings
- Clicking notifications brings Ralph to foreground

**Sound Support:**
- Default sound: `/System/Library/Sounds/Glass.aiff`
- Custom sounds via `afplay` command
- Supported formats: AIFF, WAV, and other formats supported by `afplay`
- System sounds available in `/System/Library/Sounds/`

**Example macOS Configuration:**
```json
{
  "agent": {
    "notification": {
      "sound_enabled": true,
      "sound_path": "/System/Library/Sounds/Ping.aiff"
    }
  }
}
```

### Linux

**Notification Delivery:**
- Uses D-Bus notification specification
- Compatible with GNOME, KDE, XFCE, and other desktop environments
- Falls back gracefully if notification daemon unavailable
- Respects desktop environment notification settings

**Sound Support:**
- Default: Uses `canberra-gtk-play` for theme sounds (if available)
- Custom sounds: Supports `paplay` (PulseAudio) with `aplay` (ALSA) fallback
- Supported formats: WAV, OGG, and other formats supported by PulseAudio/ALSA
- Graceful degradation if audio subsystem unavailable

**Linux Dependencies:**
```bash
# For default notification sounds (optional)
sudo apt install libcanberra-gtk-play    # Debian/Ubuntu
sudo dnf install libcanberra-gtk3        # Fedora

# For custom sound playback (usually pre-installed)
pulseaudio-utils    # For paplay
alsa-utils          # For aplay (fallback)
```

**Example Linux Configuration:**
```json
{
  "agent": {
    "notification": {
      "sound_enabled": true,
      "sound_path": "/home/user/sounds/notification.wav"
    }
  }
}
```

### Windows

**Notification Delivery:**
- Uses native Windows toast notifications
- Integrated with Windows Action Center
- Respects Focus Assist settings

**Sound Support:**
- Default: Windows toast notifications play system default sound
- Custom sounds: Uses `winmm.dll` `PlaySoundA` for `.wav` files
- Fallback to PowerShell `MediaPlayer` for other formats
- Supported formats: WAV (native), MP3 and others via PowerShell

**Example Windows Configuration:**
```json
{
  "agent": {
    "notification": {
      "sound_enabled": true,
      "sound_path": "C:\\Users\\Me\\Sounds\\complete.wav"
    }
  }
}
```

### Other Platforms

On platforms not explicitly supported, notifications are silently skipped with a debug log entry. The build can disable the `notifications` feature to compile without notification support entirely.

---

## Sound Support

Sound alerts provide an additional sensory channel for notifications, useful when you're away from your desk or the notification appears on a different monitor.

### Enabling Sound

```json
{
  "agent": {
    "notification": {
      "sound_enabled": true
    }
  }
}
```

### Custom Sound Paths

Platform-specific paths should be used:

| Platform | Example Path |
|----------|--------------|
| macOS | `/Users/name/Sounds/alert.aiff` or `/System/Library/Sounds/Glass.aiff` |
| Linux | `/home/name/sounds/alert.wav` or `/usr/share/sounds/freedesktop/stereo/complete.oga` |
| Windows | `C:\Users\name\Sounds\alert.wav` |

### Default Sounds by Platform

| Platform | Default Sound |
|----------|---------------|
| macOS | `/System/Library/Sounds/Glass.aiff` |
| Linux | Theme sound via `canberra-gtk-play --id=message` |
| Windows | System toast notification sound |

### Platform-Specific Sound Behavior

**macOS:**
- Uses `afplay` command for playback
- Sound plays synchronously (blocks until complete)
- Missing files are logged but don't fail the notification

**Linux:**
- Custom sounds: Attempts `paplay` first, falls back to `aplay`
- Default sounds: Uses `canberra-gtk-play` for theme integration
- If all methods fail, notification still displays silently

**Windows:**
- `.wav` files: Uses native `winmm.dll` `PlaySoundA`
- Other formats: Falls back to PowerShell `MediaPlayer`
- Silent fallback if both methods fail

### INTENDED vs CURRENT BEHAVIOR: Sound Failure Handling

**INTENDED BEHAVIOR:** Sound failures should be logged at debug level but never fail the notification or the calling operation.

**CURRENTLY IMPLEMENTED BEHAVIOR:** Sound failures are logged at debug level using `log::debug!()` and do not affect notification display or task execution. This matches the intended behavior.

---

## CLI Overrides

Notification settings can be overridden per-invocation via CLI flags. These take precedence over all config file settings.

### Available Flags

| Flag | Description |
|------|-------------|
| `--notify` | Enable notification on task completion |
| `--no-notify` | Disable notification on task completion |
| `--notify-fail` | Enable notification on task failure |
| `--no-notify-fail` | Disable notification on task failure |
| `--notify-sound` | Enable sound for this run |

### CLI Override Examples

**Enable notifications for a single run:**
```bash
ralph run one --notify
```

**Disable completion notifications but keep failure alerts:**
```bash
ralph run one --no-notify
```

**Enable sound for this run only:**
```bash
ralph run one --notify --notify-sound
```

**Run loop with notifications but no sound:**
```bash
ralph run loop --notify --no-notify-fail
```

**Completely silent operation (no notifications):**
```bash
ralph run one --no-notify --no-notify-fail
```

### Flag Conflicts

The following flags conflict with each other and cannot be used together:
- `--notify` and `--no-notify`
- `--notify-fail` and `--no-notify-fail`

If you need different settings for completion vs failure, use the specific flags:
```bash
# Enable completion notifications, disable failure notifications
ralph run one --notify --no-notify-fail
```

---

## Webhook Notifications

Desktop notifications can be complemented with webhook notifications for team collaboration and external integrations. See [Webhooks](./webhooks.md) for complete webhook documentation.

### Integration Points

Desktop notifications and webhooks fire for the same events but serve different purposes:

| Event | Desktop Notification | Webhook |
|-------|---------------------|---------|
| Task Complete | ✅ Popup + sound | ✅ HTTP POST to endpoint |
| Task Failed | ✅ Popup + sound | ✅ HTTP POST to endpoint |
| Loop Complete | ✅ Summary popup | ✅ HTTP POST with stats |
| Watch New Task | ✅ Popup (if enabled) | ✅ `task_created` event |

### Combined Configuration Example

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "sound_enabled": true
    },
    "webhook": {
      "enabled": true,
      "url": "https://hooks.slack.com/services/T00/B00/XXX",
      "events": ["task_completed", "task_failed"],
      "secret": "webhook-signing-secret"
    }
  }
}
```

### Wait Mode Integration

When using `--wait-when-blocked`, you can receive a notification when the queue becomes unblocked:

```bash
ralph run loop --wait-when-blocked --notify-when-unblocked
```

This sends both desktop and webhook notifications when a previously blocked task becomes ready to run.

---

## When Notifications Fire

Understanding the exact timing of notifications helps you configure them appropriately for your workflow.

### Task Completion Notifications

**When it fires:**
- After a task successfully completes all phases
- After the task status is updated to `done`
- After webhook notifications are sent
- Before celebration effects (if enabled)

**When it does NOT fire:**
- If `notify_on_complete` is `false`
- If `enabled` is `false` (legacy switch)
- If the macOS app is active and `suppress_when_active` is `true`
- If the `--no-notify` CLI flag is used
- If the notification feature is not compiled in

### Task Failure Notifications

**When it fires:**
- When a task fails during execution (runner error, CI failure, etc.)
- When a task is rejected via supervision
- Before git revert operations (if configured)

**When it does NOT fire:**
- If `notify_on_fail` is `false`
- If `enabled` is `false`
- If `--no-notify-fail` CLI flag is used

**Note:** Task failure notifications use a separate code path that includes error details in the notification body.

### Loop Completion Notifications

**When it fires:**
- When `ralph run loop` finishes (naturally or via signal)
- Includes statistics: total tasks, succeeded count, failed count

**When it does NOT fire:**
- If `notify_on_loop_complete` is `false`
- If `enabled` is `false`
- For single task runs (`ralph run one`)

### Watch Mode Notifications

**When it fires:**
- When `ralph watch` detects new tasks from code comments
- Only if `--notify` flag is passed to `ralph watch`

**Format:**
- "1 new task detected from code comments" (single)
- "N new tasks detected from code comments" (multiple)

### UI Suppression Behavior (macOS app)

When `suppress_when_active` is `true`, desktop notifications are suppressed while the macOS app is active.

---

## Practical Examples

### Example 1: Focus Mode (Failures Only)

Stay focused during development, only get interrupted for failures:

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "notify_on_complete": false,
      "notify_on_fail": true,
      "sound_enabled": true
    }
  }
}
```

### Example 2: Long-Running Tasks

For tasks that take hours, enable all notifications with sound:

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "notify_on_loop_complete": true,
      "sound_enabled": true,
      "timeout_ms": 15000
    }
  }
}
```

Then run with:
```bash
ralph run one RQ-0001 --notify-sound
```

### Example 3: CI/Server Environment

Disable desktop notifications (no GUI available):

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": false
    },
    "webhook": {
      "enabled": true,
      "url": "https://ci.example.com/webhooks/ralph"
    }
  }
}
```

### Example 4: Different Sounds for Different Projects

Use different sounds for work vs personal projects:

**Work project** (`.ralph/config.jsonc`):
```json
{
  "agent": {
    "notification": {
      "sound_enabled": true,
      "sound_path": "/System/Library/Sounds/Glass.aiff"
    }
  }
}
```

**Personal project** (`.ralph/config.jsonc`):
```json
{
  "agent": {
    "notification": {
      "sound_enabled": true,
      "sound_path": "/System/Library/Sounds/Ping.aiff"
    }
  }
}
```

### Example 5: Quiet Hours Script

Use per-run CLI overrides during focus time:
```bash
# Morning deep work session
ralph run loop --no-notify --no-notify-fail

# Afternoon collaborative work
ralph run loop --notify --notify-fail
```

---

## Troubleshooting

### Notifications Not Appearing

1. **Check configuration:**
   ```bash
   ralph config show --format json | jq '.agent.notification'
   ```

2. **Verify notification permissions:**
   - **macOS**: System Preferences → Notifications → Terminal/ralph
   - **Linux**: Check notification daemon is running (`dunst`, `mako`, etc.)
   - **Windows**: Settings → System → Notifications

3. **Check debug logs:**
   ```bash
   RUST_LOG=debug ralph run one 2>&1 | grep -i notification
   ```

### Sound Not Playing

1. **Verify sound file exists:**
   ```bash
   ls -la /path/to/sound.wav
   ```

2. **Test platform sound playback:**
   - **macOS**: `afplay /System/Library/Sounds/Glass.aiff`
   - **Linux**: `paplay /path/to/sound.wav` or `aplay /path/to/sound.wav`
   - **Windows**: Test in Sound settings

3. **Check volume/mute settings** at OS level

4. **Verify sound_enabled is true** in config or via `--notify-sound`

### UI Suppression Not Working (macOS app)

If you see notifications while the macOS app is active:
- Check that `suppress_when_active` is not explicitly set to `false`
- Check debug logs for notification/suppression details

### Build Without Notifications

To compile Ralph without notification support (reduces dependencies):

```bash
cargo build --no-default-features
```

This disables the `notifications` feature flag, making all notification functions no-ops.

---

## Related Documentation

- [Configuration](../configuration.md) - Complete configuration reference
- [Webhooks](./webhooks.md) - HTTP webhook notifications
- [App (macOS)](./app.md) - App entry point and workflow
- [Queue and Tasks](../queue-and-tasks.md) - Task lifecycle and events
