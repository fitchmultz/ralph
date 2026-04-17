# Webhook Integration Examples

> Copy-paste ready configurations for Slack, Discord, and GitHub Actions.

This guide provides working examples for integrating Ralph webhooks with popular services.
For the complete webhook configuration reference, see [Webhooks](../features/webhooks.md).

**Last verified:** 2026-02-15

---

## Overview

There are two primary approaches to webhook integration:

1. **Direct webhook** (Ralph → Service URL) - Simple, works when the service accepts arbitrary JSON
2. **Transformation proxy** (Ralph → Proxy → Service) - Required when the service expects specific payload formats

Most third-party services (Slack, Discord, GitHub Actions) require a transformation proxy because they expect specific payload structures that differ from Ralph's standard format.

---

## Slack Integration

Slack's Incoming Webhooks API expects a specific [Block Kit](https://api.slack.com/block-kit) or attachment format. Ralph's standard payload won't render correctly without transformation.

### Prerequisites

1. Create a [Slack app](https://api.slack.com/apps) in your workspace
2. Enable **Incoming Webhooks** and create a webhook URL for your channel
3. Copy the webhook URL (looks like: `https://hooks.slack.com/services/T00/B00/XXX`)

### Slack Transformation Proxy (Python/Flask)

Save this as `slack_proxy.py`:

```python
#!/usr/bin/env python3
"""
Slack webhook proxy for Ralph.
Receives Ralph webhooks, transforms to Slack format, forwards to Slack.
"""

import hmac
import hashlib
import os
from flask import Flask, request, jsonify
import requests

app = Flask(__name__)

RALPH_SECRET = os.environ.get('RALPH_WEBHOOK_SECRET', '').encode()
SLACK_WEBHOOK_URL = os.environ.get('SLACK_WEBHOOK_URL')


def verify_ralph_signature(body: bytes, signature: str) -> bool:
    if not RALPH_SECRET:
        return True  # Skip verification if no secret configured
    expected = 'sha256=' + hmac.new(RALPH_SECRET, body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, signature)


def format_slack_message(payload: dict) -> dict:
    """Transform Ralph payload to Slack Block Kit format."""
    event = payload.get('event', 'unknown')
    task_id = payload.get('task_id', 'N/A')
    task_title = payload.get('task_title', 'Untitled')

    # Color and emoji based on event type
    if event == 'task_completed':
        color = '#36a64f'  # Green
        emoji = ':white_check_mark:'
        status_text = 'completed'
    elif event == 'task_failed':
        color = '#ff0000'  # Red
        emoji = ':x:'
        status_text = 'failed'
    elif event == 'task_started':
        color = '#0099ff'  # Blue
        emoji = ':rocket:'
        status_text = 'started'
    else:
        color = '#808080'  # Gray
        emoji = ':information_source:'
        status_text = event

    return {
        "blocks": [
            {
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": f"{emoji} Ralph Task {status_text.title()}"
                }
            },
            {
                "type": "section",
                "fields": [
                    {"type": "mrkdwn", "text": f"*Task ID:*\n`{task_id}`"},
                    {"type": "mrkdwn", "text": f"*Title:*\n{task_title}"}
                ]
            },
            {
                "type": "context",
                "elements": [
                    {"type": "mrkdwn", "text": f"Event: `{event}`"}
                ]
            }
        ],
        "attachments": [
            {"color": color, "blocks": []}
        ]
    }


@app.route('/webhook', methods=['POST'])
def handle_webhook():
    signature = request.headers.get('X-Ralph-Signature', '')
    body = request.get_data()

    if not verify_ralph_signature(body, signature):
        return jsonify({'error': 'Invalid signature'}), 401

    payload = request.get_json()

    # Transform and forward to Slack
    slack_message = format_slack_message(payload)
    response = requests.post(SLACK_WEBHOOK_URL, json=slack_message)

    if response.status_code != 200:
        return jsonify({'error': 'Slack delivery failed'}), 502

    return jsonify({'status': 'ok'}), 200


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=5000)
```

### Run the Proxy

```bash
# Install dependencies
pip install flask requests

# Set environment variables
export RALPH_WEBHOOK_SECRET="your-random-secret"
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/T00/B00/XXX"

# Run the proxy
python slack_proxy.py
```

### Ralph Configuration for Slack

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "http://localhost:5000/webhook",
      "allow_insecure_http": true,
      "allow_private_targets": true,
      "secret": "your-random-secret",
      "events": ["task_completed", "task_failed", "task_started"]
    }
  }
}
```

### Testing with curl

```bash
# Test the proxy locally
curl -X POST http://localhost:5000/webhook \
  -H "Content-Type: application/json" \
  -H "X-Ralph-Signature: sha256=$(echo -n '{"event":"task_completed","timestamp":"2026-02-15T12:00:00Z","task_id":"RQ-0001","task_title":"Test task"}' | openssl dgst -sha256 | sed 's/.* //')" \
  -d '{"event":"task_completed","timestamp":"2026-02-15T12:00:00Z","task_id":"RQ-0001","task_title":"Test task","previous_status":"doing","current_status":"done"}'
```

---

## Discord Integration

Discord webhooks support rich embeds and are more flexible than Slack's format, but a transformation proxy is still recommended for proper formatting and signature verification.

### Prerequisites

1. In your Discord server, go to **Server Settings** → **Integrations** → **Webhooks**
2. Click **New Webhook**, choose a channel, and copy the webhook URL
3. The URL format is: `https://discord.com/api/webhooks/{webhook_id}/{webhook_token}`

### Discord Transformation Proxy (Python/Flask)

Save this as `discord_proxy.py`:

```python
#!/usr/bin/env python3
"""
Discord webhook proxy for Ralph.
Transforms Ralph payloads to Discord embed format.
"""

import hmac
import hashlib
import os
from flask import Flask, request, jsonify
import requests

app = Flask(__name__)

RALPH_SECRET = os.environ.get('RALPH_WEBHOOK_SECRET', '').encode()
DISCORD_WEBHOOK_URL = os.environ.get('DISCORD_WEBHOOK_URL')


def verify_ralph_signature(body: bytes, signature: str) -> bool:
    if not RALPH_SECRET:
        return True
    expected = 'sha256=' + hmac.new(RALPH_SECRET, body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, signature)


def format_discord_embed(payload: dict) -> dict:
    """Transform Ralph payload to Discord embed format."""
    event = payload.get('event', 'unknown')
    task_id = payload.get('task_id', 'N/A')
    task_title = payload.get('task_title', 'Untitled')
    timestamp = payload.get('timestamp', '')
    note = payload.get('note', '')

    # Discord color integers (decimal, not hex)
    colors = {
        'task_completed': 0x00FF00,   # Green
        'task_failed': 0xFF0000,       # Red
        'task_started': 0x0099FF,      # Blue
        'task_created': 0xFFFF00,      # Yellow
        'loop_started': 0x9900FF,      # Purple
        'loop_stopped': 0xFF9900,      # Orange
    }

    embed = {
        "title": f"Ralph: {event.replace('_', ' ').title()}",
        "description": f"**{task_title}**",
        "color": colors.get(event, 0x808080),
        "timestamp": timestamp,
        "fields": [
            {"name": "Task ID", "value": f"`{task_id}`", "inline": True},
            {"name": "Event", "value": event, "inline": True},
        ],
        "footer": {"text": "Ralph Task Queue"},
    }

    # Add status change if present
    if payload.get('previous_status') and payload.get('current_status'):
        embed["fields"].append({
            "name": "Status Change",
            "value": f"{payload['previous_status']} → {payload['current_status']}",
            "inline": False
        })

    # Add note if present
    if note:
        embed["fields"].append({
            "name": "Note",
            "value": note,
            "inline": False
        })

    # Add context for phase events
    context = payload.get('context', {})
    if context:
        if context.get('phase'):
            embed["fields"].append({
                "name": "Phase",
                "value": f"{context['phase']}/{context.get('phase_count', '?')}",
                "inline": True
            })
        if context.get('ci_gate'):
            ci_emoji = '✅' if context['ci_gate'] == 'passed' else '❌'
            embed["fields"].append({
                "name": "CI Gate",
                "value": f"{ci_emoji} {context['ci_gate']}",
                "inline": True
            })

    return {"embeds": [embed]}


@app.route('/webhook', methods=['POST'])
def handle_webhook():
    signature = request.headers.get('X-Ralph-Signature', '')
    body = request.get_data()

    if not verify_ralph_signature(body, signature):
        return jsonify({'error': 'Invalid signature'}), 401

    payload = request.get_json()
    discord_message = format_discord_embed(payload)

    response = requests.post(DISCORD_WEBHOOK_URL, json=discord_message)

    if response.status_code not in (200, 204):
        return jsonify({'error': 'Discord delivery failed'}), 502

    return jsonify({'status': 'ok'}), 200


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=5001)
```

### Run the Proxy

```bash
# Install dependencies (if not already installed)
pip install flask requests

# Set environment variables
export RALPH_WEBHOOK_SECRET="your-random-secret"
export DISCORD_WEBHOOK_URL="https://discord.com/api/webhooks/000/xxx"

# Run the proxy
python discord_proxy.py
```

### Ralph Configuration for Discord

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "http://localhost:5001/webhook",
      "allow_insecure_http": true,
      "allow_private_targets": true,
      "secret": "your-random-secret",
      "events": ["task_completed", "task_failed", "task_started"]
    }
  }
}
```

### Testing with curl

```bash
curl -X POST http://localhost:5001/webhook \
  -H "Content-Type: application/json" \
  -H "X-Ralph-Signature: sha256=$(echo -n '{"event":"task_completed","timestamp":"2026-02-15T12:00:00Z","task_id":"RQ-0001","task_title":"Add webhook integration docs"}' | openssl dgst -sha256 | sed 's/.* //')" \
  -d '{"event":"task_completed","timestamp":"2026-02-15T12:00:00Z","task_id":"RQ-0001","task_title":"Add webhook integration docs","previous_status":"doing","current_status":"done"}'
```

---

## GitHub Actions Integration

GitHub Actions can receive webhooks via the `repository_dispatch` event, which allows triggering workflows from external sources. This requires a Personal Access Token (classic) with `repo` scope.

### Prerequisites

1. Create a [Personal Access Token (classic)](https://github.com/settings/tokens) with `repo` scope
2. Store it as a secret in your repository or organization
3. Create a workflow file that responds to `repository_dispatch` events

### Step 1: Create GitHub Actions Workflow

Create `.github/workflows/ralph-webhook.yml` in your repository:

```yaml
# .github/workflows/ralph-webhook.yml
# Receives Ralph webhooks and triggers conditional actions
name: Ralph Webhook Receiver

on:
  repository_dispatch:
    types: [ralph-task-completed, ralph-task-failed, ralph-event]

jobs:
  process-webhook:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Log event details
        run: |
          echo "Event: ${{ github.event.client_payload.event }}"
          echo "Task ID: ${{ github.event.client_payload.task_id }}"
          echo "Task Title: ${{ github.event.client_payload.task_title }}"
          echo "Status: ${{ github.event.client_payload.current_status }}"

      - name: Notify Slack on failure
        if: github.event.client_payload.event == 'task_failed'
        env:
          SLACK_WEBHOOK_URL: ${{ secrets.SLACK_WEBHOOK_URL }}
        run: |
          curl -X POST "$SLACK_WEBHOOK_URL" \
            -H "Content-Type: application/json" \
            -d '{
              "text": "Ralph task failed: ${{ github.event.client_payload.task_id }}",
              "blocks": [{
                "type": "section",
                "text": {"type": "mrkdwn", "text": ":x: *Task Failed*\n*${{ github.event.client_payload.task_title }}*"}
              }]
            }'

      - name: Trigger deployment
        if: |
          github.event.client_payload.event == 'task_completed' &&
          contains(github.event.client_payload.task_title, 'deploy')
        run: |
          echo "Triggering deployment workflow..."
          # Add deployment trigger logic here

      - name: Run tests on task completion
        if: github.event.client_payload.event == 'task_completed'
        run: |
          echo "Running post-task validation..."
          # Add your validation commands here
```

### Step 2: Create a Proxy to Forward Webhooks to GitHub

Save this as `github_proxy.py`:

```python
#!/usr/bin/env python3
"""
GitHub Actions webhook proxy for Ralph.
Forwards Ralph webhooks to GitHub repository_dispatch API.
"""

import hmac
import hashlib
import os
import requests
from flask import Flask, request, jsonify

app = Flask(__name__)

RALPH_SECRET = os.environ.get('RALPH_WEBHOOK_SECRET', '').encode()
GITHUB_TOKEN = os.environ.get('GITHUB_TOKEN')  # Personal access token with repo scope
GITHUB_REPO = os.environ.get('GITHUB_REPO')    # e.g., "owner/repo"


def verify_ralph_signature(body: bytes, signature: str) -> bool:
    if not RALPH_SECRET:
        return True
    expected = 'sha256=' + hmac.new(RALPH_SECRET, body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, signature)


def get_event_type(event: str) -> str:
    """Map Ralph events to GitHub repository_dispatch event types."""
    mapping = {
        'task_completed': 'ralph-task-completed',
        'task_failed': 'ralph-task-failed',
    }
    return mapping.get(event, 'ralph-event')


@app.route('/webhook', methods=['POST'])
def handle_webhook():
    signature = request.headers.get('X-Ralph-Signature', '')
    body = request.get_data()

    if not verify_ralph_signature(body, signature):
        return jsonify({'error': 'Invalid signature'}), 401

    payload = request.get_json()
    event_type = get_event_type(payload.get('event', 'unknown'))

    # Forward to GitHub repository_dispatch API
    github_url = f"https://api.github.com/repos/{GITHUB_REPO}/dispatches"
    headers = {
        "Authorization": f"token {GITHUB_TOKEN}",
        "Accept": "application/vnd.github.v3+json",
    }
    github_payload = {
        "event_type": event_type,
        "client_payload": payload
    }

    response = requests.post(github_url, headers=headers, json=github_payload)

    if response.status_code != 204:
        return jsonify({'error': 'GitHub dispatch failed', 'details': response.text}), 502

    return jsonify({'status': 'dispatched'}), 200


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=5002)
```

### Run the GitHub Actions Proxy

```bash
# Install dependencies (if not already installed)
pip install flask requests

# Set environment variables
export RALPH_WEBHOOK_SECRET="your-random-secret"
export GITHUB_TOKEN="ghp_xxxxxxxxxxxx"  # Your Personal Access Token
export GITHUB_REPO="yourusername/yourrepo"  # e.g., "acme-corp/my-project"

# Run the proxy
python github_proxy.py
```

### Ralph Configuration for GitHub Actions

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "http://localhost:5002/webhook",
      "allow_insecure_http": true,
      "allow_private_targets": true,
      "secret": "your-random-secret",
      "events": ["task_completed", "task_failed"]
    }
  }
}
```

---

## Quick Reference Table

| Service | Integration Type | Proxy Required? | Use Case |
|---------|-----------------|-----------------|----------|
| Slack | Proxy | Yes | Team notifications, formatted messages |
| Discord | Direct or Proxy | Optional | Team notifications with rich embeds |
| GitHub Actions | Proxy | Yes | CI/CD triggers, workflow automation |

---

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Webhook not firing | `enabled: false` or no URL | Check config and ensure both are set |
| Events not received | Not in `events` list | Add event to config or use `["*"]` |
| Signature verification fails | Secret mismatch | Ensure secrets match between Ralph and proxy |
| Proxy connection refused | Port already in use | Use a different port or kill existing process |
| Slack shows raw JSON | Direct Ralph → Slack without proxy | Use the transformation proxy |
| GitHub dispatch fails | Invalid token or repo format | Verify `GITHUB_TOKEN` has `repo` scope and `GITHUB_REPO` is `owner/repo` |

### Debug Logging

Enable debug logging in your Flask proxy:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

### Testing Without Ralph

Test your proxies independently before integrating with Ralph:

```bash
# Generic test payload
curl -X POST http://localhost:5000/webhook \
  -H "Content-Type: application/json" \
  -d '{
    "event": "task_completed",
    "timestamp": "2026-02-15T12:00:00Z",
    "task_id": "RQ-0001",
    "task_title": "Test task",
    "previous_status": "doing",
    "current_status": "done"
  }'
```

### Production Deployment

For production use:

1. **Use a proper WSGI server** (e.g., Gunicorn, uWSGI) instead of Flask's development server
2. **Enable HTTPS** using a reverse proxy (nginx, Traefik) or a service like ngrok for local testing
3. **Set up monitoring** for your proxy endpoints
4. **Use environment variables** for all secrets, never hardcode them
5. **Add rate limiting** to prevent abuse

Example with Gunicorn:

```bash
# Install gunicorn
pip install gunicorn

# Run with gunicorn
gunicorn -w 2 -b 0.0.0.0:5000 slack_proxy:app
```

---

## Further Reading

- [Webhooks Feature Documentation](../features/webhooks.md) - Complete webhook configuration reference
- [Ralph Configuration](../configuration.md) - Global and project-level configuration
- [Slack Block Kit Reference](https://api.slack.com/block-kit) - Building rich Slack messages
- [Discord Webhook Reference](https://discord.com/developers/docs/resources/webhook) - Discord webhook documentation
- [GitHub repository_dispatch](https://docs.github.com/en/actions/writing-workflows/choosing-when-your-workflow-runs/events-that-trigger-workflows#repository_dispatch) - Triggering GitHub Actions via API
