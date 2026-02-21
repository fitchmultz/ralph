# Advanced Usage Guide

Purpose: Deep-dive guidance for power users and teams looking to maximize Ralph's capabilities through complex workflows, optimization, and custom integrations.

---

## Table of Contents

1. [Multi-Phase Workflows](#multi-phase-workflows)
2. [Parallel Execution](#parallel-execution)
3. [Custom Profiles](#custom-profiles)
4. [Plugin Development](#plugin-development)
5. [Advanced Configuration](#advanced-configuration)
6. [Automation](#automation)
7. [Workflow Optimization](#workflow-optimization)
8. [Troubleshooting Complex Issues](#troubleshooting-complex-issues)

---

## Multi-Phase Workflows

### Understanding Phase Selection

Ralph's phase system allows you to tailor execution depth to task complexity:

| Task Type | Recommended Phases | Rationale |
|-----------|-------------------|-----------|
| Typo fixes, small refactors | 1 phase | Minimal overhead for trivial changes |
| Feature implementation | 2-3 phases | Planning catches edge cases; review ensures quality |
| Security audits | 3 phases | Critical review stage for sensitive code |
| Architecture changes | 3 phases | Multi-phase reasoning prevents costly mistakes |
| Quick prototypes | 1 phase | Speed over thoroughness |

### Per-Phase Runner Optimization

Use different runners/models for each phase to optimize cost and quality:

```json
{
  "version": 1,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "phase_overrides": {
      "phase1": {
        "model": "gpt-5.3-codex",
        "reasoning_effort": "high"
      },
      "phase2": {
        "runner": "kimi",
        "model": "kimi-for-coding"
      },
      "phase3": {
        "runner": "claude",
        "model": "opus",
        "reasoning_effort": "high"
      }
    }
  }
}
```

**Rationale:**
- **Phase 1 (Planning)**: Use powerful model for thorough analysis
- **Phase 2 (Implementation)**: Use fast, cost-effective model for code generation
- **Phase 3 (Review)**: Use thorough model for quality assurance

### Dynamic Phase Overrides via CLI

Override phases on a per-run basis without editing config:

```bash
# Use cheap model for planning, expensive for implementation
ralph run one \
  --runner-phase1 kimi --model-phase1 kimi-for-coding \
  --runner-phase2 claude --model-phase2 opus

# Different reasoning effort per phase (Codex)
ralph run one --runner codex \
  --effort-phase1 high \
  --effort-phase2 medium \
  --effort-phase3 high
```

### Phase 2 Supervision Checkpoint

In 3-phase mode, Phase 2 intentionally stops before completion. Use this checkpoint to:

```bash
# After Phase 2 completes, review changes
ralph run one --phases 3

# Check what changed
git diff --stat

# Run additional tests not in CI gate
make integration-tests

# If satisfied, continue to Phase 3 (review)
# If not, fix manually or revert
```

### CI Gate Retry Loop

Ralph automatically retries CI failures up to 2 times. To customize this behavior:

```json
{
  "agent": {
    "ci_gate_command": "make ci",
    "ci_gate_enabled": true,
    "git_revert_mode": "ask"
  }
}
```

**Auto-retry behavior:**
1. First CI failure → Automatic retry with strict compliance message
2. Second CI failure → Automatic retry with stricter message
3. Third CI failure → Prompt user (revert/continue/proceed)

---

## Parallel Execution

### Architecture Overview

Parallel execution runs tasks in isolated git workspace clones:

```
┌─────────────────────────────────────────────────────────┐
│                    Parallel Coordinator                  │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │  Worker 1   │  │  Worker 2   │  │    Worker N     │ │
│  │  RQ-0001    │  │  RQ-0002    │  │    RQ-000N      │ │
│  │ (workspace) │  │ (workspace) │  │  (workspace)    │ │
│  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ │
│         │                │                   │          │
│         ▼                ▼                   ▼          │
│  ┌─────────────────────────────────────────────────────┐│
│  │      Agent-Owned Integration Loop (per worker)       ││
│  │   fetch/rebase/conflict-fix/commit/push to base      ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

### Configuration for Different Workflows

**Continuous Integration Workflow:**
```json
{
  "parallel": {
    "workers": 4,
    "max_push_attempts": 5,
    "push_backoff_ms": [500, 2000, 5000, 10000],
    "workspace_retention_hours": 24
  }
}
```

**Conservative Direct-Push Workflow:**
```json
{
  "parallel": {
    "workers": 3,
    "max_push_attempts": 8,
    "push_backoff_ms": [1000, 3000, 7000, 15000],
    "workspace_retention_hours": 48
  }
}
```

### Monitoring Parallel Runs

```bash
# Check state during run
watch -n 2 'ralph run parallel status'

# JSON status output for scripting
watch -n 2 'ralph run parallel status --json'

# Retry a blocked worker
ralph run parallel retry --task RQ-0001
```

### Workspace Root Configuration

Store workspaces outside the repo for cleaner git status:

```json
{
  "parallel": {
    "workspace_root": "/tmp/ralph-workspaces/myrepo"
  }
}
```

Or if inside repo, ensure gitignore:
```bash
# .gitignore
.workspaces/
```

### Handling Integration Conflicts

Parallel workers resolve rebase conflicts inside the integration loop. If a worker is blocked after retry exhaustion:

```bash
# Inspect worker lifecycle and failure reason
ralph run parallel status

# Retry that worker (reuses retained workspace/state)
ralph run parallel retry --task RQ-0001
```

### Parallel State Recovery

If parallel run crashes:

```bash
# Check current state
jq '.' .ralph/cache/parallel/state.json

# Inspect with Ralph's status command
ralph run parallel status

# Or start fresh (removes all state)
# Only do this when no active workers are running.
rm .ralph/cache/parallel/state.json
```

---

## Custom Profiles

### Team Workflow Profiles

Define standardized profiles for team consistency:

```json
{
  "version": 1,
  "profiles": {
    "quick-fix": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1
    },
    "standard-feature": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2
    },
    "critical-security": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 3,
      "reasoning_effort": "high"
    },
    "code-review": {
      "runner": "claude",
      "model": "opus",
      "phases": 1,
      "instruction_files": ["~/.codex/REVIEW_GUIDELINES.md"]
    },
    "hotfix": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1,
      "git_revert_mode": "enabled",
      "ci_gate_enabled": false
    }
  }
}
```

### Environment-Specific Profiles

```json
{
  "profiles": {
    "ci-safe": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1,
      "git_revert_mode": "enabled",
      "git_commit_push_enabled": false,
      "ci_gate_enabled": false
    },
    "local-dev": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2,
      "git_revert_mode": "ask",
      "claude_permission_mode": "bypass_permissions"
    }
  }
}
```

### Profile Chaining Patterns

Use base profile + CLI overrides for flexibility:

```bash
# Start with thorough profile, override for speed
ralph run one --profile thorough --phases 2 --runner kimi

# Use CI-safe profile but enable auto-push for this run
ralph run loop --profile ci-safe --git-commit-push-on
```

### Profile Inheritance Visualization

```
Base Config
    │
    ├── Profile: quick-fix
    │      ├── overrides: runner=kimi, phases=1
    │      └── inherits: model from base
    │
    ├── Profile: standard-feature
    │      ├── overrides: runner=claude, phases=2
    │      └── inherits: reasoning_effort from base
    │
    └── Profile: critical-security
           └── overrides: runner=codex, phases=3, effort=high
```

---

## Plugin Development

### Creating a Custom Runner

**Step 1: Scaffold the plugin**
```bash
ralph plugin init mycompany.custom-llm --with-runner --scope global
```

**Step 2: Implement the runner**

```python
#!/usr/bin/env python3
# ~/.config/ralph/plugins/mycompany.custom-llm/runner.py

import json
import sys
import os
import urllib.request

def main():
    command = sys.argv[1]
    
    # Parse arguments
    args = {}
    i = 2
    while i < len(sys.argv):
        if sys.argv[i].startswith('--'):
            key = sys.argv[i][2:].replace('-', '_')
            if i + 1 < len(sys.argv) and not sys.argv[i + 1].startswith('--'):
                args[key] = sys.argv[i + 1]
                i += 2
            else:
                args[key] = True
                i += 1
        else:
            i += 1
    
    # Read prompt from stdin
    prompt = sys.stdin.read()
    
    # Get config from environment
    config = json.loads(os.environ.get('RALPH_PLUGIN_CONFIG_JSON', '{}'))
    
    if command == 'run':
        response = call_custom_api(
            config.get('api_url'),
            config.get('api_key'),
            prompt,
            args.get('model', 'default')
        )
        
        # Output NDJSON format
        print(json.dumps({"type": "text", "content": response}))
        print(json.dumps({"type": "finish", "session_id": None}))
        
    elif command == 'resume':
        # Implement resume logic if supported
        message = sys.argv[-1]  # Last argument is the message
        print(json.dumps({"type": "text", "content": f"Resumed: {message}"}))
        print(json.dumps({"type": "finish", "session_id": None}))

def call_custom_api(url, key, prompt, model):
    # Your API integration here
    headers = {
        'Authorization': f'Bearer {key}',
        'Content-Type': 'application/json'
    }
    data = json.dumps({'prompt': prompt, 'model': model}).encode()
    
    req = urllib.request.Request(url, data=data, headers=headers, method='POST')
    
    try:
        with urllib.request.urlopen(req) as resp:
            result = json.loads(resp.read().decode())
            return result.get('completion', '')
    except Exception as e:
        return f'Error: {e}'

if __name__ == '__main__':
    main()
```

**Step 3: Configure and enable**
```bash
chmod +x ~/.config/ralph/plugins/mycompany.custom-llm/runner.py
```

```json
{
  "plugins": {
    "plugins": {
      "mycompany.custom-llm": {
        "enabled": true,
        "config": {
          "api_url": "https://api.mycompany.com/v1/complete",
          "api_key": "${CUSTOM_LLM_API_KEY}"
        }
      }
    }
  }
}
```

### Creating a Processor Plugin

**Task Validator Example:**

```python
#!/usr/bin/env python3
# .ralph/plugins/acme.task-validator/validator.py

import json
import sys
import os

def main():
    hook = sys.argv[1]
    task_id = sys.argv[2]
    filepath = sys.argv[3]
    
    if hook != "validate_task":
        sys.exit(0)
    
    with open(filepath) as f:
        task = json.load(f)
    
    config = json.loads(os.environ.get('RALPH_PLUGIN_CONFIG_JSON', '{}'))
    required_tags = config.get('required_tags', [])
    
    errors = []
    
    # Policy: High priority tasks must have evidence
    if task.get('priority') == 'high' and not task.get('evidence'):
        errors.append("High priority tasks must have evidence")
    
    # Policy: Tasks must have at least one required tag
    task_tags = set(task.get('tags', []))
    if required_tags and not any(tag in task_tags for tag in required_tags):
        errors.append(f"Task must have one of: {', '.join(required_tags)}")
    
    # Policy: Task titles under 80 characters
    if len(task.get('title', '')) > 80:
        errors.append("Task title exceeds 80 characters")
    
    if errors:
        print(f"Validation failed for {task_id}:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        sys.exit(1)
    
    print(f"Validation passed for {task_id}")

if __name__ == '__main__':
    main()
```

**Enable with config:**
```json
{
  "plugins": {
    "plugins": {
      "acme.task-validator": {
        "enabled": true,
        "config": {
          "required_tags": ["feature", "bugfix", "refactor"]
        }
      }
    }
  }
}
```

### Plugin Debugging

```bash
# Test runner directly
echo "Test prompt" | ~/.config/ralph/plugins/my.plugin/runner.sh run --model test

# Test processor hook
~/.config/ralph/plugins/my.plugin/processor.sh validate_task RQ-0001 /tmp/test-task.json

# Debug environment variables
RALPH_LOG=debug ralph run one --id RQ-0001

# Check plugin discovery
ralph plugin list
ralph plugin validate --id my.plugin
```

---

## Advanced Configuration

### JSONC (JSON with Comments)

Use comments in config for documentation:

```jsonc
{
  // Schema version - must be 1
  "version": 1,
  "agent": {
    /* Runner configuration
       Choose from: codex, opencode, gemini, claude, cursor */
    "runner": "claude",
    "phases": 3, // 1 = single-pass, 2 = plan+implement, 3 = full workflow
    
    // CI gate settings
    "ci_gate_enabled": true,
    "ci_gate_command": "make ci",
    
    // Safety settings
    "git_revert_mode": "ask",
    "git_commit_push_enabled": true
  },
  "parallel": {
    // Workspace isolation
    "workspace_root": "/tmp/ralph-workspaces",
    "workers": 3
  }
}
```

### Layered Configuration Strategy

**Global config** (`~/.config/ralph/config.json`):
```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "git_revert_mode": "ask"
  },
  "profiles": {
    "personal-default": {
      "runner": "kimi",
      "model": "kimi-for-coding"
    }
  }
}
```

**Project config** (`.ralph/config.json`):
```json
{
  "version": 1,
  "agent": {
    "ci_gate_command": "cargo test && cargo clippy",
    "phases": 2
  },
  "profiles": {
    "team-standard": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2
    }
  }
}
```

**Resolution order:** CLI flags → Task overrides → Profile → Project config → Global config → Schema defaults

### Per-Task Agent Overrides

```json
{
  "id": "RQ-0001",
  "title": "Implement complex algorithm",
  "status": "todo",
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "model_effort": "high",
    "iterations": 2,
    "followup_reasoning_effort": "medium"
  }
}
```

### Runner CLI Normalization

Configure consistent behavior across runners:

```json
{
  "agent": {
    "runner_cli": {
      "defaults": {
        "output_format": "stream_json",
        "approval_mode": "auto_edits",
        "sandbox": "enabled",
        "unsupported_option_policy": "warn"
      },
      "runners": {
        "codex": {
          "sandbox": "disabled"
        },
        "claude": {
          "verbosity": "verbose",
          "approval_mode": "bypass_permissions"
        }
      }
    }
  }
}
```

### Instruction Files Injection

Inject custom instructions at the top of every prompt:

```json
{
  "agent": {
    "instruction_files": [
      "~/.codex/GLOBAL_GUIDELINES.md",
      "AGENTS.md",
      ".ralph/custom-instructions.md"
    ]
  }
}
```

---

## Automation

### Daemon Mode Setup

**Basic daemon management:**
```bash
# Start daemon
ralph daemon start

# Check status
ralph daemon status

# View daemon logs
ralph daemon logs

# Stop daemon
ralph daemon stop
```

**systemd Service (Linux):**

Create `~/.config/systemd/user/ralph.service`:
```ini
[Unit]
Description=Ralph Daemon
After=network.target

[Service]
Type=simple
WorkingDirectory=/path/to/repo
ExecStart=/home/username/.local/bin/ralph daemon serve \
  --empty-poll-ms 10000 \
  --wait-poll-ms 1000 \
  --notify-when-unblocked
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
journalctl --user -u ralph -f
```

**launchd Service (macOS):**

Create `~/Library/LaunchAgents/com.ralph.daemon.plist`:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.ralph.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/username/.local/bin/ralph</string>
        <string>daemon</string>
        <string>serve</string>
        <string>--notify-when-unblocked</string>
    </array>
    <key>WorkingDirectory</key>
    <string>/path/to/repo</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Load and start:
```bash
launchctl load ~/Library/LaunchAgents/com.ralph.daemon.plist
launchctl start com.ralph.daemon
```

### Watch Mode Integration

**Auto-capture TODOs:**
```bash
# Terminal 1: Start daemon for execution
ralph daemon start

# Terminal 2: Watch for TODO/FIXME comments
ralph watch --auto-queue --close-removed --notify

# Terminal 3: Regular development
# Add "// TODO: refactor this" → Task auto-created
# Remove TODO comment → Task auto-closed
```

### CI/CD Integration

**GitHub Actions Example:**
```yaml
name: Ralph Task Execution

on:
  schedule:
    - cron: '0 2 * * *'  # Daily at 2 AM
  workflow_dispatch:

jobs:
  ralph:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Ralph
        run: |
          curl -fsSL https://ralph.dev/install.sh | sh
          ralph doctor --auto-fix
      
      - name: Run Tasks
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          ralph run loop \
            --non-interactive \
            --profile ci-safe \
            --max-tasks 5
      
      - name: Upload Logs
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: ralph-logs
          path: .ralph/logs/
```

**Pre-commit Hook:**
```bash
#!/bin/bash
# .git/hooks/pre-commit

# Validate queue before commit
if ! ralph queue validate --quiet; then
    echo "Queue validation failed. Run 'ralph queue validate' for details."
    exit 1
fi

# Run quick check on critical tasks
ralph run loop --profile ci-check --max-tasks 1 --non-interactive
```

### Webhook Automation

**Slack notifications for task completion:**
```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://hooks.slack.com/services/...",
      "secret": "${SLACK_WEBHOOK_SECRET}",
      "events": ["task_completed", "task_failed", "loop_stopped"],
      "timeout_secs": 30,
      "retry_count": 3
    }
  }
}
```

**Dashboard integration:**
```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://dashboard.example.com/webhooks/ralph",
      "events": ["*"],
      "queue_capacity": 500,
      "queue_policy": "block_with_timeout"
    }
  }
}
```

---

## Workflow Optimization

### Session Timeout Tuning

Configure based on your workflow:

```json
{
  "agent": {
    "session_timeout_hours": 72  // For weekend-long tasks
  }
}
```

| Use Case | Recommended Timeout |
|----------|---------------------|
| Daily development | 24 hours (default) |
| Weekend tasks | 72 hours |
| Long analysis | 168 hours (1 week) |
| CI environments | 1 hour or null |

### Runner Retry Configuration

Tune retry behavior for transient failures:

```json
{
  "agent": {
    "runner_retry": {
      "max_attempts": 5,
      "base_backoff_ms": 2000,
      "multiplier": 2.0,
      "max_backoff_ms": 60000,
      "jitter_ratio": 0.2
    }
  }
}
```

### Notification Optimization

```json
{
  "agent": {
    "notification": {
      "notify_on_complete": true,
      "notify_on_fail": true,
      "notify_on_loop_complete": false,
      "suppress_when_active": true,
      "sound_enabled": false,
      "timeout_ms": 5000
    }
  }
}
```

### Queue Auto-Archive

Keep queue clean automatically:

```json
{
  "queue": {
    "auto_archive_terminal_after_days": 7
  }
}
```

### Aging Thresholds

Configure stale task detection:

```json
{
  "queue": {
    "aging_thresholds": {
      "warning_days": 5,
      "stale_days": 10,
      "rotten_days": 20
    }
  }
}
```

### Performance Monitoring

```bash
# Track task completion time
ralph productivity velocity

# View streaks
ralph productivity streaks

# Check milestone progress
ralph productivity milestones
```

### Dependency Chain Optimization

```bash
# View critical path
ralph queue graph --critical

# Check what blocks a task
ralph queue graph --task RQ-0001 --reverse

# Optimize order: start with critical path tasks
ralph queue list --sort priority | head -10
```

---

## Troubleshooting Complex Issues

### Session Recovery

**Problem:** Session resume fails
```bash
# Check session state
jq '.' .ralph/cache/session.json

# Force fresh start
ralph run loop --force

# Or clear session manually
rm .ralph/cache/session.json
```

### Parallel Run Issues

**Problem:** "workspace_root not gitignored"
```bash
# Add to .gitignore
echo ".workspaces/" >> .gitignore
# Or use .git/info/exclude for local-only
echo ".workspaces/" >> .git/info/exclude
```

**Problem:** Base branch mismatch
```bash
# Check current branch
git branch --show-current

# View state file target branch
jq '.target_branch' .ralph/cache/parallel/state.json

# If no in-flight tasks, auto-heal by running
# Otherwise, checkout original base branch
```

**Problem:** Worker blocked in parallel integration
```bash
# Inspect worker lifecycle + error context
ralph run parallel status --json | jq '.workers[] | select(.lifecycle == "blocked_push")'

# Retry a blocked worker explicitly
ralph run parallel retry --task RQ-0001
```

### Queue Lock Issues

**Problem:** Stale queue lock
```bash
# Check lock status
ls -la .ralph/lock/

# Safe unlock (verifies PID not running)
ralph queue unlock

# Force with caution
ralph run one --force
```

### Plugin Debugging

**Problem:** Plugin not executing
```bash
# Verify plugin discovered
ralph plugin list

# Check validation
ralph plugin validate --id my.plugin

# Test runner directly
echo "test" | ~/.config/ralph/plugins/my.plugin/runner.sh run --model test

# Check environment
env | grep RALPH_
```

### Phase Violations

**Problem:** Phase 1 made code changes
```bash
# Check what changed
git status
git diff

# With git_revert_mode: ask
# Choose: revert, keep+continue, or continue with message

# Force proceed (if you know changes are acceptable)
ralph run one --force --allow-dirty
```

### CI Gate Failures

**Problem:** CI repeatedly fails
```bash
# Run CI manually to see output
make ci

# Check CI command config
ralph config show | grep ci_gate

# Temporarily disable (not recommended for production)
ralph run one --no-ci-gate
```

### Memory and Resource Issues

**Problem:** High memory usage during parallel runs
```json
{
  "parallel": {
    "workers": 2  // Reduce from default
  }
}
```

**Problem:** Slow task processing
```bash
# Use quick profile for simple tasks
ralph run one --profile quick

# Skip phases when appropriate
ralph run one --phases 1
```

### Webhook Delivery Issues

**Problem:** Webhooks not sending
```bash
# Test webhook directly
ralph webhook test --url https://your-endpoint.com/webhook

# Check config
ralph config show | grep -A 10 webhook

# Debug with logs
RUST_LOG=debug ralph run one 2>&1 | grep -i webhook
```

### Dependency Resolution

**Problem:** Task stuck waiting for dependencies
```bash
# Check dependency graph
ralph queue graph --task RQ-0001

# View blocking tasks
ralph queue list --status doing

# Check done.json for completed dependencies
jq '.tasks[] | select(.id == "RQ-0000")' .ralph/done.json
```

### Recovery Patterns

**Complete reset procedure:**
```bash
# 1. Stop any running daemon
ralph daemon stop

# 2. Clear all state
rm -f .ralph/cache/session.json
rm -f .ralph/cache/parallel/state.json
rm -f .ralph/cache/daemon.json
rm -f .ralph/cache/stop_requested

# 3. Clear locks (if safe)
ralph queue unlock

# 4. Validate queue
ralph queue validate

# 5. Restart daemon if needed
ralph daemon start
```

**Debug mode for troubleshooting:**
```bash
# Enable debug logging
ralph --debug run one --id RQ-0001

# View debug logs
tail -f .ralph/logs/debug.log

# Clean up after
cat .ralph/logs/debug.log  # Review for secrets
rm -rf .ralph/logs/        # Secure deletion
```

---

## Quick Reference

### Common Command Patterns

```bash
# Quick single task
ralph run one --profile quick

# Full workflow with review
ralph run one --profile thorough

# Parallel execution
ralph run loop --parallel 4 --max-tasks 10

# Dry-run to check what would run
ralph run loop --dry-run

# Non-interactive CI mode
ralph run loop --non-interactive --max-tasks 5

# Resume interrupted session
ralph run loop --resume

# Wait for dependencies
ralph run loop --wait-when-blocked --wait-timeout-seconds 3600
```

### Config Quick Reference

| Setting | Config Path | CLI Override |
|---------|-------------|--------------|
| Runner | `agent.runner` | `--runner` |
| Model | `agent.model` | `--model` |
| Phases | `agent.phases` | `--phases` |
| Profile | N/A | `--profile` |
| Parallel workers | `parallel.workers` | `--parallel` |
| CI gate | `agent.ci_gate_enabled` | `--ci-gate-on/off` |
| Git push | `agent.git_commit_push_enabled` | `--git-commit-push-on/off` |

### File Locations

| File | Default Location |
|------|------------------|
| Queue | `.ralph/queue.json` |
| Done archive | `.ralph/done.json` |
| Project config | `.ralph/config.json` |
| Global config | `~/.config/ralph/config.json` |
| Session state | `.ralph/cache/session.json` |
| Parallel state | `.ralph/cache/parallel/state.json` |
| Daemon logs | `.ralph/logs/daemon.log` |
| Debug logs | `.ralph/logs/debug.log` |
| Prompt overrides | `.ralph/prompts/*.md` |
| Plugins (project) | `.ralph/plugins/<id>/` |
| Plugins (global) | `~/.config/ralph/plugins/<id>/` |
