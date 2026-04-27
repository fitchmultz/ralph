# Advanced Workflows
Status: Active
Owner: Maintainers
Source of truth: this document for advanced workflow execution, parallel execution, and workflow optimization guidance
Parent: [Advanced Usage Guide](advanced.md)


Purpose: Deep-dive guidance for power users and teams tuning Ralph's multi-phase execution, parallel task processing, retry/session behavior, notifications, queue cleanup, and dependency ordering.

---

## Table of Contents

1. [Multi-Phase Workflows](#multi-phase-workflows)
2. [Parallel Execution](#parallel-execution)
3. [Workflow Optimization](#workflow-optimization)

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
  "version": 2,
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

# Different reasoning effort per phase (Codex or Pi)
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
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    },
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
    "max_push_attempts": 50,
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
      "notify_on_watch_new_tasks": true,
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
ralph productivity streak

# Check overall progress summary
ralph productivity summary
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
