# Ralph Webhooks System

Ralph's webhook system enables real-time HTTP notifications for task lifecycle events, allowing external systems (Slack, Discord, CI/CD pipelines, dashboards) to react to and monitor task execution.

## Overview

Webhooks complement desktop notifications by providing a machine-readable, integration-friendly event stream. When enabled, Ralph sends HTTP POST requests to your configured endpoint whenever subscribed events occur.

### Common Use Cases

- **Team Notifications**: Post task completions to Slack/Discord channels
- **CI/CD Integration**: Trigger downstream builds when tasks complete
- **Dashboard Updates**: Feed real-time task status to monitoring dashboards
- **Audit Logging**: Capture task history in external systems
- **Automation**: Trigger custom workflows based on task events

---

## Quick Start: Integration Examples

For copy-paste ready examples with Slack, Discord, and GitHub Actions, see **[Webhook Integration Examples](../guides/webhook-integrations.md)**.

---

## Configuration

Webhooks are configured via the `agent.webhook` section in your config file (`.ralph/config.jsonc` or `~/.config/ralph/config.jsonc`).

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | boolean | `false` | Master switch for webhook notifications |
| `url` | string | `null` | Webhook endpoint URL (required when enabled) |
| `allow_insecure_http` | boolean | `false` | Allow `http://` URLs (default HTTPS-only) |
| `allow_private_targets` | boolean | `false` | Allow loopback, link-local, and metadata-style hosts |
| `secret` | string | `null` | Secret key for HMAC-SHA256 signature generation |
| `events` | string[] | `null` | List of events to subscribe to (see [Event Filtering](#event-filtering)) |
| `timeout_secs` | number | `30` | HTTP request timeout (1-300 seconds) |
| `retry_count` | number | `3` | Retry attempts for failed deliveries (0-10) |
| `retry_backoff_ms` | number | `1000` | Base interval for exponential retry delays in ms (100-30000); delays include bounded jitter and cap at 30 seconds |
| `queue_capacity` | number | `500` | Maximum pending webhooks in queue (10-10000) |
| `parallel_queue_multiplier` | number | `2.0` | Parallel-mode queue capacity multiplier (1.0-10.0) |
| `queue_policy` | string | `"drop_oldest"` | Backpressure policy when queue is full |

When `enabled` is `true`, Ralph validates `url` before delivery: HTTPS is the default; `http://` needs `allow_insecure_http: true`. Loopback, link-local, and common metadata hostnames are blocked unless `allow_private_targets: true`.

### Queue Policy Options

- **`drop_oldest`**: Drop new webhooks when queue is full (preserves existing queue contents)
- **`drop_new`**: Drop the new webhook if the queue is full
- **`block_with_timeout`**: Block briefly (100ms), then drop if queue still full

### Basic Configuration Example

```json
{
  "version": 1,
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX",
      "secret": "my-webhook-secret",
      "events": ["task_completed", "task_failed"],
      "timeout_secs": 30,
      "retry_count": 3,
      "retry_backoff_ms": 1000,
      "queue_capacity": 100,
      "queue_policy": "drop_oldest"
    }
  }
}
```

### CI/Dashboard Integration Example

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://ci.example.com/webhooks/ralph",
      "events": ["loop_started", "phase_started", "phase_completed", "loop_stopped"],
      "queue_capacity": 500,
      "queue_policy": "block_with_timeout"
    }
  }
}
```

---

## Event Types

Ralph emits webhook events across three categories: **Task Events**, **Loop Events**, and **Phase Events**.

### Task Events (Enabled by Default)

These core task lifecycle events are always enabled when webhooks are on (unless explicitly filtered):

| Event | Description | When Emitted |
|-------|-------------|--------------|
| `task_created` | Task added to queue | After `ralph task \"...\"` / `ralph task build \"...\"` or scan creates a task |
| `task_started` | Task execution begins | When task status changes to `doing` |
| `task_completed` | Task finished successfully | When task status changes to `done` |
| `task_failed` | Task failed or rejected | When task status changes to `rejected` |
| `task_status_changed` | Generic status transition | Any status change not covered above |

### Loop Events (Opt-in)

Loop-level events track the execution loop lifecycle. These are **opt-in** and must be explicitly configured:

| Event | Description | When Emitted |
|-------|-------------|--------------|
| `loop_started` | Run loop initiated | When `ralph run loop` begins execution |
| `loop_stopped` | Run loop terminated | When loop exits (success, failure, or signal) |

**Note**: Loop events do not include `task_id` or `task_title` fields since they are not task-specific.

### Phase Events (Opt-in)

Phase events track individual phase execution within multi-phase tasks. These are **opt-in**:

| Event | Description | When Emitted |
|-------|-------------|--------------|
| `phase_started` | Phase execution begins | When a task phase starts running |
| `phase_completed` | Phase execution ends | When a task phase finishes (success or failure) |

Phase events include enriched context metadata (runner, model, git info, CI gate status).

### Queue Unblocked Event (Opt-in)

A special event emitted when a blocked queue becomes runnable:

| Event | Description | When Emitted |
|-------|-------------|--------------|
| `queue_unblocked` | Queue became runnable | When `--notify-when-unblocked` is used and blocked tasks become available |

The `queue_unblocked` event includes:
- `previous_status`: `"blocked"`
- `current_status`: `"runnable"`
- `note`: Summary counts (e.g., `"ready=2 blocked_deps=3 blocked_schedule=1"`)

---

## Event Filtering

Ralph uses an **opt-in model for new events** to maintain backward compatibility.

### Filtering Behavior

| `events` Configuration | Behavior |
|------------------------|----------|
| `null` or not specified | Only legacy task events (`task_*`) are delivered |
| `["*"]` | All events are delivered (legacy + new) |
| `["task_completed", "phase_started", ...]` | Only explicitly listed events are delivered |

### Opt-in Safety

New event types (`loop_*`, `phase_*`, `queue_unblocked`) are **not enabled by default**. This prevents unexpected payload formats from reaching existing integrations when Ralph adds new event types.

### Recommended Patterns

**Legacy-compatible (task events only)**:
```json
{ "events": null }
// or omit the events field entirely
```

**Subscribe to all current and future events**:
```json
{ "events": ["*"] }
```

**Explicit opt-in to specific events**:
```json
{ "events": ["task_completed", "task_failed", "phase_completed"] }
```

**Dashboard/monitoring (loop + phase tracking)**:
```json
{ "events": ["loop_started", "phase_started", "phase_completed", "loop_stopped"] }
```

---

## Payload Format

All webhooks are sent as HTTP POST requests with `Content-Type: application/json`.

### Base Payload Structure

```json
{
  "event": "task_completed",
  "timestamp": "2024-01-15T10:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Add webhook support",
  "previous_status": "doing",
  "current_status": "done",
  "note": null
}
```

### Field Descriptions

| Field | Type | Presence | Description |
|-------|------|----------|-------------|
| `event` | string | Always | Event type identifier |
| `timestamp` | string (RFC3339) | Always | When the event occurred |
| `task_id` | string | Task events only | Task identifier (e.g., `RQ-0001`) |
| `task_title` | string | Task events only | Human-readable task title |
| `previous_status` | string | Optional | Status before the change |
| `current_status` | string | Optional | New status after the change |
| `note` | string | Optional | Additional context or message |

### Task Event Payload Examples

**task_created**:
```json
{
  "event": "task_created",
  "timestamp": "2024-01-15T09:00:00Z",
  "task_id": "RQ-0001",
  "task_title": "Implement user authentication",
  "previous_status": null,
  "current_status": null,
  "note": null
}
```

**task_started**:
```json
{
  "event": "task_started",
  "timestamp": "2024-01-15T09:05:00Z",
  "task_id": "RQ-0001",
  "task_title": "Implement user authentication",
  "previous_status": "todo",
  "current_status": "doing",
  "note": null
}
```

**task_completed**:
```json
{
  "event": "task_completed",
  "timestamp": "2024-01-15T09:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Implement user authentication",
  "previous_status": "doing",
  "current_status": "done",
  "note": null
}
```

**task_failed**:
```json
{
  "event": "task_failed",
  "timestamp": "2024-01-15T09:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Implement user authentication",
  "previous_status": "doing",
  "current_status": "rejected",
  "note": "Runner returned non-zero exit code"
}
```

### Enriched Payloads (Phase Events)

Phase and loop events include additional context metadata:

```json
{
  "event": "phase_completed",
  "timestamp": "2024-01-15T10:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Add webhook support",
  "runner": "claude",
  "model": "sonnet",
  "phase": 2,
  "phase_count": 3,
  "duration_ms": 12500,
  "repo_root": "/home/user/project",
  "branch": "main",
  "commit": "abc123def456",
  "ci_gate": "passed"
}
```

### Context Fields Reference

Optional context fields are only present when applicable:

| Field | Type | Description |
|-------|------|-------------|
| `runner` | string | Runner used (e.g., `claude`, `codex`, `kimi`) |
| `model` | string | Model used for this phase |
| `phase` | number | Phase number (1, 2, or 3) |
| `phase_count` | number | Total configured phases |
| `duration_ms` | number | Execution duration in milliseconds |
| `repo_root` | string | Repository root path |
| `branch` | string | Current git branch |
| `commit` | string | Current git commit hash |
| `ci_gate` | string | CI gate outcome: `skipped`, `passed`, or `failed` |

### Loop Event Payloads

Loop events omit `task_id` and `task_title` since they are not task-specific:

```json
{
  "event": "loop_started",
  "timestamp": "2024-01-15T10:00:00Z",
  "repo_root": "/home/user/project",
  "branch": "main",
  "commit": "abc123def456"
}
```

```json
{
  "event": "loop_stopped",
  "timestamp": "2024-01-15T12:30:00Z",
  "repo_root": "/home/user/project",
  "branch": "main",
  "commit": "abc123def456",
  "note": "Completed 5 tasks, 0 failed"
}
```

---

## Security

Ralph provides HMAC-SHA256 signature verification to ensure webhook authenticity.

### Signature Header

When a `secret` is configured, Ralph includes an `X-Ralph-Signature` header:

```
X-Ralph-Signature: sha256=abc123def456...
```

The signature format is `sha256=` followed by the lowercase hex-encoded HMAC-SHA256 of the request body.

### Verification Examples

#### Python (Flask)

```python
import hmac
import hashlib
from flask import Flask, request, abort

app = Flask(__name__)
WEBHOOK_SECRET = b'my-webhook-secret'

@app.route('/webhook', methods=['POST'])
def handle_webhook():
    # Get signature from header
    signature = request.headers.get('X-Ralph-Signature', '')
    
    # Compute expected signature
    body = request.get_data()
    expected = 'sha256=' + hmac.new(
        WEBHOOK_SECRET, body, hashlib.sha256
    ).hexdigest()
    
    # Constant-time comparison to prevent timing attacks
    if not hmac.compare_digest(expected, signature):
        abort(401, 'Invalid signature')
    
    # Process the webhook
    payload = request.get_json()
    print(f"Received {payload['event']} for {payload.get('task_id', 'N/A')}")
    
    return '', 200
```

#### Node.js (Express)

```javascript
const express = require('express');
const crypto = require('crypto');

const app = express();
const WEBHOOK_SECRET = 'my-webhook-secret';

app.use(express.raw({ type: 'application/json' }));

app.post('/webhook', (req, res) => {
  const signature = req.headers['x-ralph-signature'];
  
  // Compute expected signature
  const expected = 'sha256=' + crypto
    .createHmac('sha256', WEBHOOK_SECRET)
    .update(req.body)
    .digest('hex');
  
  // Constant-time comparison
  if (!crypto.timingSafeEqual(
    Buffer.from(signature), 
    Buffer.from(expected)
  )) {
    return res.status(401).send('Invalid signature');
  }
  
  // Process the webhook
  const payload = JSON.parse(req.body);
  console.log(`Received ${payload.event} for ${payload.task_id || 'N/A'}`);
  
  res.sendStatus(200);
});
```

#### Ruby (Sinatra)

```ruby
require 'sinatra'
require 'openssl'

WEBHOOK_SECRET = 'my-webhook-secret'

post '/webhook' do
  request.body.rewind
  body = request.body.read
  
  signature = request.env['HTTP_X_RALPH_SIGNATURE']
  expected = 'sha256=' + OpenSSL::HMAC.hexdigest(
    OpenSSL::Digest.new('sha256'),
    WEBHOOK_SECRET,
    body
  )
  
  halt 401, 'Invalid signature' unless Rack::Utils.secure_compare(expected, signature)
  
  payload = JSON.parse(body)
  puts "Received #{payload['event']} for #{payload['task_id'] || 'N/A'}"
  
  status 200
end
```

### Security Best Practices

1. **Always verify signatures** in production environments
2. **Use HTTPS endpoints** to prevent MITM attacks
3. **Keep secrets secure** - use environment variables, not hardcoded values
4. **Use constant-time comparison** to prevent timing attacks
5. **Rotate secrets periodically** and revoke compromised secrets immediately

---

## Delivery Semantics

### Best-Effort Delivery

Ralph's webhook system follows **best-effort delivery semantics**:

- **Non-blocking**: Webhook calls return immediately after enqueueing
- **Async processing**: A background worker handles HTTP delivery
- **FIFO ordering**: Webhooks are delivered in order (within queue policy constraints)
- **Bounded failure persistence**: Final delivery failures are written to `.ralph/cache/webhooks/failures.json` (latest 200 records)

### Retry Behavior

Failed deliveries are automatically retried with exponential backoff:

1. Initial attempt (immediate)
2. Retry 1: after roughly `retry_backoff_ms`
3. Retry 2: after roughly `retry_backoff_ms * 2`
4. Retry 3: after roughly `retry_backoff_ms * 4`
- ...up to `retry_count` attempts

Retry timing includes bounded jitter to avoid synchronized retry storms and is capped at 30 seconds.

**Retryable failures**: Timeouts, connection errors, HTTP 5xx responses  
**Non-retryable failures**: HTTP 4xx responses (except 429), invalid URL

### Queue Policies and Backpressure

When the webhook queue reaches capacity, Ralph applies the configured `queue_policy`:

| Policy | Behavior | Use Case |
|--------|----------|----------|
| `drop_new` | Drop the new webhook | Prioritize queue depth over recency |
| `drop_oldest` | Drop new webhooks | Preserve existing queue (note: due to channel constraints, this behaves like `drop_new`) |
| `block_with_timeout` | Block 100ms then drop | Briefly wait for queue space |

> **Note on `drop_oldest`**: Due to crossbeam channel constraints, the sender cannot remove items from the front of the queue. In practice, `drop_oldest` behaves like `drop_new`. This is a known limitation.

### Worker Lifecycle

- **Starts**: On first webhook send after process startup
- **Runs**: Until the Ralph process exits
- **Cleanup**: Automatic thread cleanup on drop

### Idempotency Considerations

Webhook consumers should implement idempotency since:
- Retries may cause duplicate delivery
- Network issues may result in successful delivery but failed acknowledgment

**Recommended approach**: Use `task_id` + `event` + `timestamp` as a unique key for deduplication.

---

## Testing

Use the `ralph webhook test` command to verify your webhook configuration.

### Basic Testing

```bash
# Test with configured URL
ralph webhook test

# Test specific event type
ralph webhook test --event task_completed

# Test with custom URL (overrides config)
ralph webhook test --url https://example.com/webhook
```

### Testing Opt-in Events

```bash
# Test phase events
ralph webhook test --event phase_started
ralph webhook test --event phase_completed

# Test loop events
ralph webhook test --event loop_started
ralph webhook test --event loop_stopped
```

### Inspecting Payloads

```bash
# Print JSON payload without sending (useful for debugging)
ralph webhook test --event phase_completed --print-json

# Pretty-print the payload
ralph webhook test --event task_created --print-json --pretty

# Custom task ID and title for testing
ralph webhook test --event task_completed \
  --task-id "TEST-1234" \
  --task-title "Test webhook payload"
```

### Test Command Options

| Option | Description |
|--------|-------------|
| `--event` | Event type to send (default: `task_created`) |
| `--url` | Override webhook URL |
| `--task-id` | Custom task ID for payload (default: `TEST-0001`) |
| `--task-title` | Custom task title (default: `Test webhook notification`) |
| `--print-json` | Print payload without sending |
| `--pretty` | Pretty-print JSON (default: `true`) |

### Delivery Diagnostics and Replay

Use built-in diagnostics/replay commands to inspect delivery health and recover failed events:

```bash
# Inspect counters + recent failure records
ralph webhook status
ralph webhook status --format json

# Replay specific failures safely
ralph webhook replay --dry-run --id wf-1700000000-1
ralph webhook replay --event task_completed --limit 5
ralph webhook replay --task-id RQ-0814 --max-replay-attempts 3
```

Replay safety defaults:
- Replay requires explicit targeting (`--id`, `--event`, or `--task-id`).
- `--dry-run` previews candidates without enqueueing.
- Replay attempts are capped per failure record (`--max-replay-attempts`).
- Non-dry-run replay requires `agent.webhook.enabled=true` and a configured `agent.webhook.url`.

---

## Integration Examples

> **See the [Webhook Integration Guide](../guides/webhook-integrations.md) for complete, copy-paste ready examples with Slack, Discord, and GitHub Actions.**

The examples below show conceptual patterns. For production-ready code, use the integration guide.

### Slack Integration

Configure Slack Incoming Webhooks to receive task notifications:

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://hooks.slack.com/services/T00/B00/XXX",
      "secret": "slack-signing-secret",
      "events": ["task_completed", "task_failed"],
      "retry_count": 5
    }
  }
}
```

**Slack webhook handler** (using Slack's Block Kit):

```python
import hmac
import hashlib
import json
from flask import Flask, request

app = Flask(__name__)
SECRET = b'your-secret'

@app.route('/slack-webhook', methods=['POST'])
def slack_webhook():
    # Verify signature
    signature = request.headers.get('X-Ralph-Signature', '')
    body = request.get_data()
    expected = 'sha256=' + hmac.new(SECRET, body, hashlib.sha256).hexdigest()
    
    if not hmac.compare_digest(expected, signature):
        return 'Unauthorized', 401
    
    payload = request.get_json()
    
    # Format Slack message
    color = '#36a64f' if payload['event'] == 'task_completed' else '#ff0000'
    emoji = ':white_check_mark:' if payload['event'] == 'task_completed' else ':x:'
    
    slack_payload = {
        "blocks": [
            {
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": f"{emoji} Task {payload['current_status'].upper()}: {payload['task_id']}"
                }
            },
            {
                "type": "section",
                "fields": [
                    {"type": "mrkdwn", "text": f"*Task:*\n{payload['task_title']}"},
                    {"type": "mrkdwn", "text": f"*Event:*\n{payload['event']}"}
                ]
            }
        ]
    }
    
    # Forward to Slack
    import requests
    requests.post(
        'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK',
        json=slack_payload
    )
    
    return '', 200
```

### Discord Integration

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://discord.com/api/webhooks/000/xxx",
      "events": ["task_completed", "task_failed", "loop_started", "loop_stopped"]
    }
  }
}
```

**Discord webhook handler**:

```python
import hmac
import hashlib
from flask import Flask, request
import requests

app = Flask(__name__)
SECRET = b'your-secret'
DISCORD_WEBHOOK = 'https://discord.com/api/webhooks/YOUR/WEBHOOK'

@app.route('/discord-webhook', methods=['POST'])
def discord_webhook():
    # Verify signature
    signature = request.headers.get('X-Ralph-Signature', '')
    body = request.get_data()
    expected = 'sha256=' + hmac.new(SECRET, body, hashlib.sha256).hexdigest()
    
    if not hmac.compare_digest(expected, signature):
        return 'Unauthorized', 401
    
    payload = request.get_json()
    
    # Color based on event
    colors = {
        'task_completed': 0x00ff00,
        'task_failed': 0xff0000,
        'loop_started': 0x0099ff,
        'loop_stopped': 0x9900ff
    }
    
    embed = {
        "title": f"Ralph: {payload['event']}",
        "color": colors.get(payload['event'], 0x808080),
        "fields": [],
        "timestamp": payload['timestamp']
    }
    
    if payload.get('task_id'):
        embed['fields'].append({
            "name": "Task ID",
            "value": payload['task_id'],
            "inline": True
        })
        embed['fields'].append({
            "name": "Title",
            "value": payload.get('task_title', 'N/A'),
            "inline": True
        })
    
    requests.post(DISCORD_WEBHOOK, json={"embeds": [embed]})
    return '', 200
```

### Custom HTTP Endpoint

A complete example of a production-ready webhook receiver:

```python
#!/usr/bin/env python3
"""
Ralph Webhook Receiver Example
Production-ready handler with signature verification, idempotency, and logging.
"""

import hmac
import hashlib
import json
import logging
import sqlite3
from datetime import datetime
from functools import wraps
from flask import Flask, request, jsonify

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger('ralph-webhook')

app = Flask(__name__)

# Configuration
WEBHOOK_SECRET = b'your-webhook-secret-here'  # Use env var in production
DB_PATH = '/var/lib/ralph-webhooks/events.db'

# Initialize database
def init_db():
    conn = sqlite3.connect(DB_PATH)
    conn.execute('''
        CREATE TABLE IF NOT EXISTS webhook_events (
            id TEXT PRIMARY KEY,
            event_type TEXT,
            task_id TEXT,
            received_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            payload TEXT
        )
    ''')
    conn.commit()
    conn.close()

def verify_signature(f):
    """Decorator to verify webhook signatures."""
    @wraps(f)
    def decorated(*args, **kwargs):
        signature = request.headers.get('X-Ralph-Signature', '')
        body = request.get_data()
        
        expected = 'sha256=' + hmac.new(
            WEBHOOK_SECRET, body, hashlib.sha256
        ).hexdigest()
        
        if not hmac.compare_digest(expected, signature):
            logger.warning('Invalid signature received')
            return jsonify({'error': 'Invalid signature'}), 401
        
        return f(*args, **kwargs)
    return decorated

def is_duplicate(event_type, task_id, timestamp):
    """Check if this event has already been processed."""
    event_id = f"{task_id}:{event_type}:{timestamp}"
    
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.execute(
        'SELECT 1 FROM webhook_events WHERE id = ?',
        (event_id,)
    )
    exists = cursor.fetchone() is not None
    conn.close()
    
    return exists, event_id

def record_event(event_id, event_type, task_id, payload):
    """Record processed event for idempotency."""
    conn = sqlite3.connect(DB_PATH)
    conn.execute(
        'INSERT INTO webhook_events (id, event_type, task_id, payload) VALUES (?, ?, ?, ?)',
        (event_id, event_type, task_id, json.dumps(payload))
    )
    conn.commit()
    conn.close()

@app.route('/webhook', methods=['POST'])
@verify_signature
def handle_webhook():
    payload = request.get_json()
    
    event_type = payload['event']
    task_id = payload.get('task_id', 'N/A')
    timestamp = payload['timestamp']
    
    # Idempotency check
    is_dup, event_id = is_duplicate(event_type, task_id, timestamp)
    if is_dup:
        logger.info(f"Duplicate event ignored: {event_id}")
        return jsonify({'status': 'duplicate'}), 200
    
    # Record event
    record_event(event_id, event_type, task_id, payload)
    
    # Process based on event type
    handlers = {
        'task_created': handle_task_created,
        'task_started': handle_task_started,
        'task_completed': handle_task_completed,
        'task_failed': handle_task_failed,
        'phase_completed': handle_phase_completed,
    }
    
    handler = handlers.get(event_type, handle_generic)
    result = handler(payload)
    
    logger.info(f"Processed {event_type} for {task_id}")
    return jsonify(result), 200

def handle_task_created(payload):
    """Handle task_created event."""
    logger.info(f"New task created: {payload['task_title']}")
    return {'status': 'processed', 'action': 'logged'}

def handle_task_completed(payload):
    """Handle task_completed event."""
    logger.info(f"Task completed: {payload['task_id']}")
    # Trigger downstream CI, notify team, etc.
    return {'status': 'processed', 'action': 'notified'}

def handle_task_failed(payload):
    """Handle task_failed event."""
    logger.error(f"Task failed: {payload['task_id']} - {payload.get('note')}")
    # Alert on-call, create incident, etc.
    return {'status': 'processed', 'action': 'alerted'}

def handle_phase_completed(payload):
    """Handle phase_completed event."""
    context = payload.get('context', {})
    logger.info(
        f"Phase {context.get('phase')}/{context.get('phase_count')} completed "
        f"for {payload['task_id']} (CI: {context.get('ci_gate')})"
    )
    return {'status': 'processed', 'action': 'tracked'}

def handle_generic(payload):
    """Handle unknown event types."""
    logger.info(f"Generic event: {payload['event']}")
    return {'status': 'processed', 'action': 'logged'}

@app.route('/health', methods=['GET'])
def health():
    return jsonify({'status': 'healthy'}), 200

if __name__ == '__main__':
    init_db()
    app.run(host='0.0.0.0', port=5000)
```

### CI/CD Integration

Trigger downstream pipelines based on Ralph task completion:

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://ci.example.com/api/webhooks/ralph",
      "secret": "ci-webhook-secret",
      "events": ["task_completed", "phase_completed"],
      "timeout_secs": 60,
      "retry_count": 5
    }
  }
}
```

**GitLab CI trigger example**:

```python
import hmac
import hashlib
import os
from flask import Flask, request
import requests

app = Flask(__name__)
SECRET = os.environ['RALPH_WEBHOOK_SECRET'].encode()
GITLAB_TOKEN = os.environ['GITLAB_TOKEN']
GITLAB_PROJECT = 'group/project'

@app.route('/trigger-ci', methods=['POST'])
def trigger_ci():
    # Verify signature
    signature = request.headers.get('X-Ralph-Signature', '')
    body = request.get_data()
    expected = 'sha256=' + hmac.new(SECRET, body, hashlib.sha256).hexdigest()
    
    if not hmac.compare_digest(expected, signature):
        return 'Unauthorized', 401
    
    payload = request.get_json()
    
    # Only trigger on specific task completion
    if payload['event'] == 'task_completed':
        task_title = payload.get('task_title', '').lower()
        
        # Trigger different pipelines based on task type
        if 'deploy' in task_title:
            trigger_pipeline('deploy')
        elif 'test' in task_title:
            trigger_pipeline('test')
    
    return '', 200

def trigger_pipeline(pipeline_type):
    url = f'https://gitlab.com/api/v4/projects/{GITLAB_PROJECT}/trigger/pipeline'
    requests.post(url, data={
        'token': GITLAB_TOKEN,
        'ref': 'main',
        'variables[PIPELINE_TYPE]': pipeline_type
    })
```

---

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Webhooks not sending | `enabled: false` or no URL | Check config and ensure both are set |
| Events not received | Not in `events` list | Add event to config or use `["*"]` |
| Signature verification fails | Secret mismatch | Ensure secrets match between sender and receiver |
| Queue full warnings | Slow endpoint or high volume | Increase `queue_capacity` or optimize endpoint |
| Retries exhausted | Endpoint down or slow | Check endpoint health and timeout settings |

### Debug Logging

Enable debug logging to see webhook delivery details:

```bash
RALPH_LOG=debug ralph run loop
```

Look for log lines containing `Webhook`:
- `Webhook enqueued for delivery`
- `Webhook delivered successfully`
- `Webhook delivery failed`
- `Webhook retry attempt N`

### Testing Connectivity

```bash
# Test with curl
curl -X POST https://your-endpoint.com/webhook \
  -H "Content-Type: application/json" \
  -d '{"event":"test","timestamp":"2024-01-01T00:00:00Z"}'

# Test with ralph
ralph webhook test --url https://your-endpoint.com/webhook
```

---

## Reference

### Event Type Quick Reference

```
Task Events (default enabled):
  task_created, task_started, task_completed, task_failed, task_status_changed

Opt-in Events:
  loop_started, loop_stopped, phase_started, phase_completed, queue_unblocked

Subscribe to all: ["*"]
```

### HTTP Headers

| Header | Value | Condition |
|--------|-------|-----------|
| `Content-Type` | `application/json` | Always |
| `User-Agent` | `ralph/{version}` | Always |
| `X-Ralph-Signature` | `sha256={hex}` | When `secret` is configured |

### Response Handling

Ralph considers these HTTP status codes:

- **Success**: 2xx (delivery considered successful)
- **Retryable**: 5xx, timeouts, connection errors
- **Non-retryable**: 4xx (except 429)
