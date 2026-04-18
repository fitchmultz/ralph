# Security Features

![Security](../assets/images/2026-02-07-11-32-24-security.png)

Purpose: Comprehensive guide to Ralph's security features, including secrets redaction, safeguard dumps, debug logging, git safety, CI gate validation, plugin security, approval modes, and webhook security.

---

## Overview

Security is critical when running AI agents that have access to your codebase, environment variables, and can execute commands. Ralph includes multiple layers of security features designed to protect sensitive information while providing visibility and control over agent operations.

### Security Philosophy

1. **Defense in Depth**: Multiple overlapping security controls
2. **Secure by Default**: Conservative defaults that prioritize safety
3. **Explicit Opt-in**: Dangerous operations require explicit user consent
4. **Auditability**: Clear logging and tracking of all actions
5. **Fail Safe**: Errors default to safe states

---

## Secrets Redaction

Ralph includes comprehensive pattern-based redaction to prevent accidental exposure of sensitive data in logs, error messages, and console output.

### RedactedString Type

The `RedactedString` wrapper type ensures sensitive data is automatically redacted when displayed:

```rust
use ralph::redaction::RedactedString;

// Create a redacted string
let sensitive = RedactedString::from("API_KEY=sk-secret123");

// Display automatically redacts
println!("{}", sensitive);  // Output: API_KEY=[REDACTED]
```

Key properties:
- **Automatic redaction**: Redaction happens at display time via `Display` and `Debug` traits
- **Immutable**: The original content is preserved internally but masked on output
- **Used in error types**: `RunnerError` stores stdout/stderr as `RedactedString`

### Pattern-Based Redaction

Ralph applies multiple redaction patterns in sequence:

| Pattern | Description | Example |
|---------|-------------|---------|
| **Key-Value Pairs** | Keys matching sensitive labels | `API_KEY=secret` → `API_KEY=[REDACTED]` |
| **Bearer Tokens** | Content after `Bearer ` | `Authorization: Bearer token123` → `Authorization: Bearer [REDACTED]` |
| **AWS Keys** | AKIA-prefixed access keys | `AKIAIOSFODNN7EXAMPLE` → `[REDACTED]` |
| **AWS Secrets** | 40-char base64-like strings that are not pure hex | Pure 40-char git SHAs stay readable; AWS secret access keys still match |
| **SSH Keys** | PEM-encoded keys | `-----BEGIN...END-----` → `[REDACTED]` |
| **Hex Tokens** | Very long hex runs (≥96 chars), or ≥32 chars when preceded by sensitive words (for example `token`, `secret`, `signature`) within ~80 characters; alphanumeric word boundaries apply | Standalone git SHAs/hashes typically stay readable |
| **Env Variables** | Values of sensitive env vars | `MY_SECRET=value` → `[REDACTED]` |

### Sensitive Environment Variables

Ralph automatically detects and redacts values of environment variables with keys matching these patterns:

- `*KEY*` (e.g., `API_KEY`, `SECRET_KEY`)
- `*SECRET*` (e.g., `AWS_SECRET`, `AUTH_SECRET`)
- `*TOKEN*` (e.g., `API_TOKEN`, `ACCESS_TOKEN`)
- `*PASSWORD*` (e.g., `DB_PASSWORD`, `USER_PASSWORD`)
- Numbered variants (e.g., `SECRET1`, `TOKEN_2`)

Exclusions (not redacted):
- Path-like variables: `PATH`, `HOME`, `CWD`, `PWD`, `TEMP`, `TMP`, `TMPDIR`
- Values shorter than 6 characters

### Using Redaction in Code

```rust
// Redact text directly
use ralph::redaction::redact_text;

let output = redact_text("password=secret123");
assert_eq!(output, "password=[REDACTED]");

// Use redaction macros for logging
use ralph::{rinfo, rwarn, rerror, rdebug, rtrace};

rinfo!("Connecting with API_KEY={}", api_key);  // Redacted automatically
```

### RedactedLogger

Wrap any logger to automatically redact all log messages:

```rust
use ralph::redaction::RedactedLogger;
use log::LevelFilter;

// Wrap an existing logger
let inner = Box::new(env_logger::builder().build());
RedactedLogger::init(inner, LevelFilter::Info)?;

// All subsequent log messages are automatically redacted
log::info!("Using API_KEY={}", secret);  // Redacted in output
```

### Limitations of Pattern-Based Redaction

**Important**: Redaction is pattern-based and best-effort. It may miss secrets in unexpected formats.

Known limitations:

1. **Encoding variations**: Base64-encoded secrets, URL-encoded tokens, or JSON-escaped values may not match patterns
2. **Novel formats**: Custom secret formats not matching known patterns
3. **Split secrets**: Secrets broken across multiple lines or chunks
4. **Contextual sensitivity**: Cannot detect secrets based on context alone

**Best Practice**: Always review output before sharing, even when redaction is applied.

---

## Safeguard Dumps

Safeguard dumps are temporary files written for troubleshooting and error recovery. They capture context when operations fail (runner errors, validation failures).

### Redacted Dumps (Default)

By default, safeguard dumps apply redaction to all sensitive content:

```rust
use ralph::fsutil::safeguard_text_dump_redacted;

// Write redacted dump (safe default)
let path = safeguard_text_dump_redacted("error_context", &content)?;
println!("Dump written to: {}", path.display());
```

Redacted dumps are written to:
- Location: System temp directory under `ralph/` (e.g., `/tmp/ralph/error_context_XXXXXX/`)
- File: `output.txt`
- Persistence: Survives process exit via `TempDir::keep()`

### Raw Dumps (Opt-in Required)

Raw dumps write unredacted content and require explicit opt-in:

```bash
# Enable via environment variable
export RALPH_RAW_DUMP=1
ralph run one

# Or use --debug flag (implies raw dumps)
ralph run one --debug
```

Programmatic usage:

```rust
use ralph::fsutil::safeguard_text_dump;

// Requires RALPH_RAW_DUMP=1 or debug_mode=true
let path = safeguard_text_dump("debug_context", &raw_content, is_debug_mode)?;
```

**Security Warning**: Raw dumps may contain secrets. Only use when necessary for debugging and keep them secure. Never commit raw dumps to version control.

### Cleaning Up Dumps

Redacted dumps persist via `TempDir::keep()`. Clean them up periodically:

```bash
# List Ralph temp directories
ls -la /tmp/ralph/

# Clean up old dumps
rm -rf /tmp/ralph/error_context_*
```

---

## Debug Logging

Debug logging captures detailed runtime information for troubleshooting. It has important security implications.

### Enabling Debug Mode

```bash
# Enable debug logging
ralph run one --debug

# Or set environment variable
export RALPH_DEBUG=1
```

### What Gets Logged

When `--debug` is enabled, `.ralph/logs/debug.log` contains:

1. **Log records** (raw, unredacted)
2. **Runner stdout/stderr** (raw streams)
3. **Internal state transitions**

### Security Implications

| Output | Redaction | Safe to Share |
|--------|-----------|---------------|
| Console output (terminal) | ✅ Redacted via `RedactedLogger` | Yes (review first) |
| Debug log file | ❌ Raw, unredacted | **No - treat as sensitive** |
| Safeguard dumps (default) | ✅ Redacted | Yes (review first) |
| Safeguard dumps (raw) | ❌ Unredacted | **No** |

**Important**: Debug logs capture raw runner output before redaction is applied. Even when console output appears clean, debug logs may contain secrets.

### Best Practices for Debug Logs

1. **Use sparingly**: Only enable `--debug` when necessary for troubleshooting

2. **Treat as sensitive**: `.ralph/logs/debug.log` contains unredacted data

3. **Clean up after use**:
   ```bash
   rm -rf .ralph/logs/
   ```

4. **Never commit**: Ensure `.ralph/logs/` is in `.gitignore`:
   ```gitignore
   # .gitignore
   .ralph/logs/
   ```

5. **Review before sharing**: If you must share debug logs, manually review and sanitize them first

### Console vs Debug Log Differences

```rust
// Console output: REDACTED
// Log record goes through RedactedLogger before display
log::info!("API_KEY={}", secret);  // Console: API_KEY=[REDACTED]

// Debug log: RAW
// Written directly via debuglog::write_log_record()
// Contains: API_KEY=actual_secret_value
```

---

## Runner Output Handling

Ralph handles runner output differently depending on the destination:

### Console Output Flow

```
Runner stdout/stderr
    ↓
[Captured raw]
    ↓
[Written to debug.log] (if --debug)
    ↓
[Parsed for JSON items]
    ↓
[Displayed via RedactedLogger] ← Redacted for console
```

### Key Differences

| Aspect | Console Output | Debug Log |
|--------|---------------|-----------|
| Redaction | ✅ Yes, via `RedactedString` | ❌ No, raw capture |
| Purpose | User visibility | Troubleshooting |
| Persistence | No | Yes (file on disk) |
| Safe to share | Review first | **Never** |

### Example: Error Display

```rust
// RunnerError stores output as RedactedString
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("runner exited non-zero (code={code})\nstdout: {stdout}\nstderr: {stderr}")]
    NonZeroExit {
        code: i32,
        stdout: RedactedString,  // Auto-redacted on display
        stderr: RedactedString,  // Auto-redacted on display
        session_id: Option<String>,
    },
}

// When formatted for display, secrets are redacted
let err = RunnerError::NonZeroExit {
    code: 1,
    stdout: "API_KEY=secret123".into(),
    stderr: "bearer token".into(),
    session_id: None,
};
println!("{}", err);  // Secrets are redacted
```

---

## Git Safety

Ralph includes git safety features to prevent unintended changes and provide recovery options.

### Clean Repository Checks

Before operations, Ralph can verify the repository is in a clean state:

```rust
use ralph::git::clean::require_clean_repo_ignoring_paths;

// Check repo is clean (ignoring Ralph's own files)
require_clean_repo_ignoring_paths(
    repo_root,
    force,  // Bypass check if true
    &[
        ".ralph/queue.jsonc",
        ".ralph/queue.jsonc",
        ".ralph/done.jsonc",
        ".ralph/done.jsonc",
        ".ralph/config.jsonc",
        ".ralph/config.jsonc",
        ".ralph/cache/",
    ],
)?;
```

Allowed dirty paths (Ralph's own files):
- `.ralph/queue.jsonc` / `.ralph/queue.jsonc` - Active task queue
- `.ralph/done.jsonc` / `.ralph/done.jsonc` - Completed task archive
- `.ralph/config.jsonc` / `.ralph/config.jsonc` - Project configuration
- `.ralph/cache/` - Cache directory

### Revert Modes

Configure automatic git revert behavior when errors occur:

```json
{
  "agent": {
    "git_revert_mode": "ask"
  }
}
```

| Mode | Behavior |
|------|----------|
| `ask` | **Default**: Prompt user for action when plan-only violations detected |
| `enabled` | Automatically revert changes on runner/supervision errors |
| `disabled` | Never automatically revert changes |

The revert mode controls behavior during:
- Plan-only violations in Phase 1
- Runner execution errors
- Supervision failures

### Using Force Flag

Bypass clean repository checks with `--force`:

```bash
# Bypass clean repo check
ralph run one --force

# Useful when:
# - You have intentional uncommitted changes
# - You're in the middle of a task chain
# - You understand the risks
```

---

## CI Gate

The CI gate is a validation step that runs before commits are finalized, ensuring code quality and preventing broken changes.

### How It Works

```
Phase 2 (Implementation)
    ↓
[Apply changes]
    ↓
[Run CI gate command]
    ↓
Success → Continue to Phase 3
Failure → Stop and report errors
```

### Configuration

```json
{
  "agent": {
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    }
  }
}
```

| Option | Default | Description |
|--------|---------|-------------|
| `ci_gate.enabled` | `true` | Enable/disable CI gate |
| `ci_gate.argv` | `["make", "ci"]` | Direct argv command to run for validation |

### CI Gate Execution

The CI gate runs:
1. After Phase 2 implementation (before Phase 3)
2. After Phase 3 review (before completion)

When the CI gate fails:
- Task stops for review
- Changes are NOT automatically reverted
- User can fix issues and retry

### Local CI Gate

The `make ci` target typically includes:

```makefile
# Makefile
ci: check-env-safety check-backup-artifacts deps format-check type-check lint test build generate install-verify
```

This ensures:
- No `.env` file accidentally committed
- No backup artifacts in source
- Code passes all checks
- Tests pass
- Build succeeds

### Security Benefits

1. **Prevents broken code**: Validates before commit/push
2. **Enforces standards**: Runs linting, formatting, type checking
3. **Catches secrets**: Some CI gates include secret scanning
4. **Ensures tests pass**: Prevents regressions

---

## Plugin Security

Ralph supports plugins for extending functionality with custom runners and task processors.

### Important: Plugins Are NOT Sandboxed

**Critical Security Warning**: Plugin executables run with the same privileges as Ralph. They have:

- Full access to the filesystem
- Access to environment variables
- Ability to execute arbitrary commands
- Same network access as Ralph

### Plugin Discovery

Plugins are discovered from:

1. **Global plugins**: `~/.config/ralph/plugins/<plugin_id>/plugin.json`
2. **Project plugins**: `.ralph/plugins/<plugin_id>/plugin.json`

Project plugins override global plugins of the same ID.

### Enabling Plugins

Plugins are **disabled by default** and must be explicitly enabled:

```json
{
  "plugins": {
    "plugins": {
      "my.custom.plugin": {
        "enabled": true,
        "config": {
          "custom_setting": "value"
        }
      }
    }
  }
}
```

### Security Best Practices for Plugins

1. **Only enable trusted plugins**: Enabling a plugin is equivalent to trusting it
2. **Review plugin code**: Understand what a plugin does before enabling
3. **Keep plugin binaries plugin-local**: Manifest `runner.bin` / `processors.bin` must stay relative to the plugin directory and remain inside it after canonical path resolution (symlink escapes are rejected)
4. **Limit plugin scope**: Use project-specific plugins over global when possible
5. **Monitor plugin activity**: Review plugin output in logs

### Plugin Manifest Structure

```json
{
  "api_version": 1,
  "id": "my.custom.plugin",
  "name": "My Custom Plugin",
  "version": "1.0.0",
  "runner": {
    "bin": "my-runner"
  },
  "processors": []
}
```

---

## Approval Modes

Approval modes control how much autonomy AI runners have when making changes.

### Available Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `safe` | **Warning**: May cause interactive prompts/hangs | Maximum safety (not recommended) |
| `auto_edits` | Auto-approve file edits only | Balanced safety/convenience |
| `yolo` | **Default**: Bypass all approvals | Fully automated operation |
| `default` | Use runner's default behavior | Runner-dependent |

### Configuration

Via config file:

```json
{
  "agent": {
    "runner_cli": {
      "defaults": {
        "approval_mode": "auto_edits"
      },
      "runners": {
        "claude": {
          "approval_mode": "yolo"
        }
      }
    }
  }
}
```

Via CLI:

```bash
# Set approval mode for a run
ralph run one --approval-mode=auto_edits
```

### Claude-Specific: Permission Modes

Claude has a legacy `claude_permission_mode` setting:

```json
{
  "agent": {
    "claude_permission_mode": "accept_edits"
  }
}
```

| Mode | Behavior |
|------|----------|
| `accept_edits` | Auto-approve file edits |
| `bypass_permissions` | Skip all permission prompts (YOLO mode) |

**Warning**: `bypass_permissions` grants the runner full autonomy. Use with caution.

### Runner Support Matrix

| Runner | `safe` | `auto_edits` | `yolo` |
|--------|--------|--------------|--------|
| Claude | ❌ Not implemented | ✅ Supported | ✅ Supported (default) |
| Gemini | ❌ Not implemented | ✅ Supported | ✅ Supported |
| Codex | ❌ Not implemented | ❌ Not supported | ✅ Supported (default) |
| Cursor | ❌ Not implemented | ❌ Not supported | ✅ Supported (default) |
| Kimi | ❌ Not implemented | ❌ Not supported | ✅ Supported (default) |
| Plugins | ❌ Not implemented | ✅ Supported | ✅ Supported |

### Safe Mode Warning

The `safe` approval mode is **not consistently implemented** across runners and may cause:
- Interactive prompts that hang indefinitely
- Unexpected behavior
- Task failures

Use `auto_edits` or `yolo` for automated workflows.

---

## Webhook Security

Ralph can emit webhook events for external integrations (Slack, Discord, CI systems, dashboards).

### Destination URL policy

When `agent.webhook.enabled` is true, Ralph validates `agent.webhook.url` before delivery:

- Only `http://` and `https://` schemes are accepted; other schemes are rejected.
- `http://` is rejected unless `agent.webhook.allow_insecure_http` is `true`.
- Loopback, IPv4 link-local (`169.254.0.0/16`), IPv6 link-local, `localhost` / `*.localhost`, and `metadata.google.internal` are rejected unless `agent.webhook.allow_private_targets` is `true`.

### HMAC-SHA256 Signatures

When a webhook secret is configured, Ralph signs all webhook payloads with HMAC-SHA256:

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://example.com/webhook",
      "secret": "your-webhook-secret-key"
    }
  }
}
```

### Signature Verification

Webhook receivers should verify the signature to ensure authenticity:

```python
import hmac
import hashlib

def verify_webhook(payload_body: bytes, signature: str, secret: str) -> bool:
    """
    Verify webhook signature from X-Ralph-Signature header.
    
    Args:
        payload_body: Raw request body bytes
        signature: Value from X-Ralph-Signature header (format: "sha256=<hex>")
        secret: Shared secret key
    
    Returns:
        True if signature is valid
    """
    expected = hmac.new(
        secret.encode(),
        payload_body,
        hashlib.sha256
    ).hexdigest()
    
    expected_sig = f"sha256={expected}"
    return hmac.compare_digest(expected_sig, signature)
```

### Webhook Headers

| Header | Description |
|--------|-------------|
| `Content-Type` | `application/json` |
| `User-Agent` | `ralph/X.Y.Z` |
| `X-Ralph-Signature` | `sha256=<hex>` (when secret configured) |

### Security Best Practices

1. **Always use HTTPS**: Never send webhooks over unencrypted HTTP
2. **Use strong secrets**: Generate cryptographically random secrets (32+ bytes)
3. **Verify signatures**: Always validate the HMAC signature
4. **Use fixed-time comparison**: Use `hmac.compare_digest()` to prevent timing attacks
5. **Rotate secrets periodically**: Change webhook secrets regularly
6. **IP allowlisting**: Restrict webhook endpoints to known Ralph IPs if possible

### Example: Secure Webhook Handler

```python
from flask import Flask, request, abort
import hmac
import hashlib
import json

app = Flask(__name__)
WEBHOOK_SECRET = "your-secret-here"

@app.route('/webhook', methods=['POST'])
def webhook():
    # Get signature from header
    signature = request.headers.get('X-Ralph-Signature')
    if not signature:
        abort(401, "Missing signature")
    
    # Verify signature
    expected = hmac.new(
        WEBHOOK_SECRET.encode(),
        request.data,
        hashlib.sha256
    ).hexdigest()
    
    if not hmac.compare_digest(f"sha256={expected}", signature):
        abort(401, "Invalid signature")
    
    # Process verified webhook
    payload = request.get_json()
    event_type = payload['event']
    
    if event_type == 'task_completed':
        handle_task_completed(payload)
    
    return '', 200

def handle_task_completed(payload):
    print(f"Task {payload['task_id']} completed: {payload['task_title']}")
```

### Webhook Event Types

Events are categorized by sensitivity:

**Default Events** (always sent if webhooks enabled):
- `task_created` - Task added to queue
- `task_started` - Task execution begins
- `task_completed` - Task finished successfully
- `task_failed` - Task failed or was rejected
- `task_status_changed` - Generic status transition

**Opt-in Events** (must be explicitly configured):
- `loop_started` - Run loop begins
- `loop_stopped` - Run loop ends
- `phase_started` - Phase execution begins
- `phase_completed` - Phase execution ends
- `queue_unblocked` - Queue became runnable after being blocked

Configure events:

```json
{
  "agent": {
    "webhook": {
      "events": ["task_completed", "task_failed", "loop_started"]
    }
  }
}
```

Use `["*"]` to subscribe to all events.

---

## Security Checklist

### Before Running Ralph

- [ ] Repository is in a known good state
- [ ] `.env` file is in `.gitignore` and not tracked
- [ ] Sensitive environment variables are necessary for the task
- [ ] Approval mode is appropriate for the task risk level

### During Operation

- [ ] Monitor runner output for unexpected behavior
- [ ] Review changes before committing (unless using `yolo` mode)
- [ ] Check that CI gate passes before finalizing

### Before Sharing Output

- [ ] Review for any missed secrets or sensitive data
- [ ] Use redacted safeguard dumps, not raw dumps
- [ ] Sanitize task notes if they contain sensitive context
- [ ] Never share debug logs without review

### Regular Maintenance

- [ ] Clean up `.ralph/logs/` directory periodically
- [ ] Remove old safeguard dumps from `/tmp/ralph/`
- [ ] Rotate webhook secrets
- [ ] Review and update plugin trust decisions
- [ ] Keep Ralph updated: `cargo install ralph-agent-loop --force`

---

## Reporting Security Issues

If you discover a security vulnerability in Ralph:

1. **Do not open a public issue**
2. Preferred: use GitHub private vulnerability reporting in the repository Security tab (`Security` → `Report a vulnerability`)
3. If private reporting is unavailable, contact the maintainer via <https://github.com/mitchfultz>
4. Include:
   - Clear description of the vulnerability
   - Steps to reproduce
   - Potential impact assessment
4. Allow reasonable time for remediation before public disclosure

---

## Related Documentation

- [Configuration](../configuration.md) - Full config reference
- [Workflow](../workflow.md) - Runtime layout and phases
- [Queue and Tasks](../queue-and-tasks.md) - Task management
- [SECURITY.md](../../SECURITY.md) - Security policy and vulnerability reporting
