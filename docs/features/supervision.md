# Supervision System

![Supervision](../assets/images/2026-02-07-11-32-24-supervision.png)

Ralph's supervision system provides human-in-the-loop oversight for CI gate enforcement, git operations, and queue state management during task execution. It ensures code quality through automated checks while providing flexible recovery options when things go wrong.

---

## Overview

The supervision system orchestrates the post-execution workflow after an AI runner completes a task. It serves as the quality gate between task implementation and task completion, handling:

**Core Responsibilities:**
- **CI Gate Enforcement**: Running the configured CI command (`make ci` by default) to validate changes
- **Git State Management**: Committing, pushing, and reverting changes based on outcomes
- **Queue Updates**: Marking tasks as done, archiving completed work, and managing task lifecycle
- **Session Resumption**: Coordinating continue/resume cycles for CI failure recovery
- **Notifications**: Triggering desktop and webhook notifications on completion

**Key Design Principles:**
- **Safety First**: Default configurations favor manual approval over automatic actions
- **Recoverability**: CI failures auto-retry with strict compliance messaging before human intervention
- **Transparency**: All supervision decisions are logged with clear reasoning
- **Flexibility**: Multiple git revert modes accommodate different workflow preferences

---

## CI Gate

The CI gate is Ralph's primary quality enforcement mechanism. It runs after task implementation to ensure all changes meet project standards before completion.

### Configuration

```json
{
  "version": 1,
  "agent": {
    "ci_gate_enabled": true,
    "ci_gate_command": "make ci"
  }
}
```

| Setting | Default | Description |
|---------|---------|-------------|
| `ci_gate_enabled` | `true` | Enable/disable the CI gate entirely |
| `ci_gate_command` | `"make ci"` | Command to run for validation |

### Command Execution

The CI gate command:
1. Runs in the repository root with inherited stdin/stdout/stderr
2. Must exit with code 0 to pass
3. Failure triggers the revert mode policy and/or auto-retry logic

**Example Commands:**
```bash
# Default (most projects)
"make ci"

# Rust projects
"cargo test && cargo clippy"

# Node.js projects  
"npm test && npm run lint"

# Custom scripts
"./scripts/validate.sh"
```

### Auto-Retry Behavior

When CI fails during Phase 2, Phase 3, or single-phase execution, Ralph automatically retries with a strict compliance message:

1. **First 2 failures**: Auto-send continue message to the runner requesting fixes
2. **Third failure**: Apply `git_revert_mode` policy (prompt user, auto-revert, or skip)

The auto-retry message emphasizes:
> "Compliance is mandatory. No hacky fixes allowed e.g. skipping tests, half-assed patches, etc. Implement fixes your mother would be proud of."

### Failure Handling

When the CI gate fails after exhausting retries:

| Mode | Behavior |
|------|----------|
| `ask` | Prompt user to keep changes, revert, or continue with message |
| `enabled` | Automatically revert uncommitted changes |
| `disabled` | Leave changes in place, fail the task |

---

## Git Operations

The supervision system manages git state throughout the task lifecycle.

### Commit and Push

When `git_commit_push_enabled` is true and a task completes successfully:

1. **Commit**: All changes are committed with a formatted message:
   ```
   RQ-0001: Add user authentication feature
   ```

2. **Push**: Commits are pushed to the configured upstream

3. **Verification**: Repository must be clean after operations (except for allowed paths)

### Configuration

```json
{
  "version": 1,
  "agent": {
    "git_commit_push_enabled": true
  }
}
```

**Safety Warning**: When enabled, Ralph automatically pushes changes to the remote repository. This action is irreversible. Ralph prompts for confirmation when enabling this setting.

### Push Policies

| Policy | Behavior |
|--------|----------|
| `RequireUpstream` | Skip push if no upstream is configured |
| `AllowCreateUpstream` | Create upstream branch if missing (`git push -u origin HEAD`) |

### LFS Validation

Git LFS files are validated before commit when `--lfs-check` is enabled:

- Detects modified LFS files in working tree
- Validates LFS filter configuration
- Warns about potential data loss issues

```bash
# Strict LFS validation
ralph run one --lfs-check
```

---

## Git Revert Modes

The `git_revert_mode` setting controls how Ralph handles uncommitted changes when errors occur.

### Mode: `ask` (Default)

Interactive prompt allowing user choice:

```
CI failure: action? [1=keep (default), 2=revert, 3=other]: 
```

**Options:**
- **1 / keep**: Leave changes in place (default on Enter)
- **2 / revert**: Run `git checkout -- .` to discard uncommitted changes
- **3 / other**: Send a custom message to the runner to continue
- **4 / keep+continue** (when allowed): Proceed without sending message

**Non-TTY Behavior**: When stdin is not a terminal, defaults to "keep changes" to prevent hanging in automated environments.

### Mode: `enabled`

Automatically revert uncommitted changes on any error:

```json
{
  "agent": {
    "git_revert_mode": "enabled"
  }
}
```

**Use Case**: CI environments where failed attempts should always be discarded.

### Mode: `disabled`

Never revert changes automatically:

```json
{
  "agent": {
    "git_revert_mode": "disabled"
  }
}
```

**Use Case**: Debugging sessions where you want to inspect failed changes.

### Comparison Table

| Scenario | `ask` | `enabled` | `disabled` |
|----------|-------|-----------|------------|
| CI failure (auto-retry exhausted) | Prompt user | Auto-revert | Keep changes |
| Phase 1 plan-only violations | Prompt with proceed option | Auto-revert | Keep changes |
| Task inconsistency detected | Prompt user | Auto-revert | Error only |
| Non-TTY environment | Keep changes | Auto-revert | Keep changes |

---

## Auto Commit/Push

The `git_commit_push_enabled` setting controls whether Ralph automatically commits and pushes changes after successful task completion.

### Enabled (Default)

```json
{
  "agent": {
    "git_commit_push_enabled": true
  }
}
```

**Behavior:**
- Commits all changes with task ID prefix
- Pushes to upstream if ahead
- Requires clean repo state for Phase 3 completion

### Disabled

```json
{
  "agent": {
    "git_commit_push_enabled": false
  }
}
```

**Behavior:**
- Leaves repository dirty after queue updates
- Still marks tasks as done in queue
- User must manually commit and push

**Use Case**: Code review workflows where human review is required before committing.

### Parallel Mode Implications

When `git_commit_push_enabled` is disabled:
- Parallel workers still run agent-owned integration (fetch/rebase/conflict-fix).
- Final commit/push remains disabled; worker exits with dirty repo state for manual follow-up.

---

## Queue Operations

The supervision system manages task lifecycle state transitions and queue file maintenance.

### Task Status Transitions

```
Todo → Doing → Done (or Rejected)
```

**Automatic Transitions:**
- `Todo` → `Doing`: When task execution begins
- `Doing` → `Done`: When supervision completes successfully
- Terminal tasks → `done.json`: When archived

### Completion Signals

Phase 3 writes a completion signal to `.ralph/cache/completions/{TASK_ID}.json`:

```json
{
  "task_id": "RQ-0001",
  "status": "done",
  "notes": ["Reviewed and approved"],
  "runner_used": "claude",
  "model_used": "sonnet"
}
```

**Signal Requirements:**
- Status must be `done` or `rejected` (terminal states)
- Used for analytics and webhook events
- Staged and committed with task changes

**Note:** In parallel mode, workers finalize directly to the coordinator base branch (`origin/<target_branch>`) through the integration loop. No merge-agent subprocess is involved.

### Queue Maintenance

Supervision performs automatic queue maintenance:

1. **Backfill `completed_at`**: Stamps missing completion timestamps for terminal tasks
2. **Validate queue set**: Ensures ID uniqueness and dependency integrity
3. **Archive terminal tasks**: Moves done/rejected tasks to `done.json`

### Dirty Repo Handling

When the repository has uncommitted changes at completion:

1. Mark task as Done in queue
2. Archive to done file
3. Commit all changes (if `git_commit_push_enabled`)
4. Push to upstream

### Clean Repo Handling

When the repository is clean at completion (e.g., documentation-only review):

1. Ensure task is marked Done
2. Archive to done file
3. Push if ahead (for any previous commits)

---

## Completion Checklist

The Phase 2 → Phase 3 handoff includes a completion checklist that ensures implementation quality before review.

### Checklist Items

The Phase 2 handoff checklist (`.ralph/prompts/phase2_handoff_checklist.md`) typically includes:

- **CI Gate**: `make ci` passes with no warnings
- **No deferrals**: Phase 2 closes follow-ups it discovers; only true blockers may remain, with explicit remediation steps
- **Documentation**: Module docs updated for changed files
- **Tests**: New behavior covered (success + failure modes)
- **Feature Parity**: CLI and macOS app behavior consistent (when applicable)
- **Help Text**: User-facing commands have `--help` examples
- **Secrets**: No credentials committed or logged

### Enforcement

The runner is prompted to verify checklist items before signaling completion. The checklist is advisory—the supervision system performs its own validation.

### Customization

Override the default checklist by creating `.ralph/prompts/phase2_handoff_checklist.md`:

```markdown
# Phase 2 Handoff Checklist

- [ ] All new code has unit tests
- [ ] API changes documented in CHANGELOG.md
- [ ] Performance benchmarks added
- [ ] Security review completed
```

---

## Notification Integration

Supervision triggers notifications at task completion to alert users of results.

### Desktop Notifications

Configured via `agent.notification`:

```json
{
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "suppress_when_active": true,
      "sound_enabled": false,
      "timeout_ms": 8000
    }
  }
}
```

**Platform Support:**
- **macOS**: NotificationCenter
- **Linux**: D-Bus/notify-send
- **Windows**: Toast notifications

### Webhook Events

Supervision emits webhook events for external integrations:

| Event | Description |
|-------|-------------|
| `task_completed` | Task finished successfully |
| `task_failed` | Task failed or was rejected |
| `phase_completed` | Phase finished (includes CI gate outcome) |

**Phase Event Payload:**
```json
{
  "event": "phase_completed",
  "timestamp": "2024-01-15T10:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Add feature",
  "phase": 3,
  "phase_count": 3,
  "ci_gate": "passed",
  "duration_ms": 12500
}
```

See [Webhooks](./webhooks.md) for full configuration.

---

## Supervision Flow

### Sequential Task Flow

```
┌─────────────────┐
│  Task Complete  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│ Check Git Status│────▶│   Is Dirty?     │
└─────────────────┘     └────────┬────────┘
                                 │
                    ┌────────────┴────────────┐
                    │ Yes                      │ No
                    ▼                          ▼
         ┌─────────────────┐       ┌─────────────────┐
         │ Run CI Gate     │       │ Task Already    │
         │ (with auto-     │       │ Done?           │
         │  retry)         │       └────────┬────────┘
         └────────┬────────┘                │
                  │              ┌──────────┴──────────┐
                  │           Yes │                    │ No
                  │              ▼                     ▼
         ┌────────┴────────┐    ┌──────────────┐  ┌─────────────────┐
         │   CI Pass?      │    │ Push if Ahead│  │ Mark Task Done  │
         └────────┬────────┘    └──────────────┘  └────────┬────────┘
                  │                                         │
       ┌─────────┴──────────┐                              ▼
       │                    │                     ┌─────────────────┐
    Yes │                 No │                     │ Archive Task    │
       ▼                    ▼                     └────────┬────────┘
┌─────────────────┐   ┌─────────────────┐                 │
│ Mark Task Done  │   │ Apply Revert    │                 ▼
└────────┬────────┘   │ Mode Policy     │        ┌─────────────────┐
         │            └─────────────────┘        │ Commit & Push   │
         │                                       │ (if enabled)    │
         ▼                                       └─────────────────┘
┌─────────────────┐
│ Archive Task    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Commit & Push   │
│ (if enabled)    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Notify User   │
└─────────────────┘
```

### Parallel Worker Flow

Parallel workers have a simplified supervision flow:

1. **Run CI Gate** (if dirty)
2. **Restore Bookkeeping**: Reset queue/done/productivity files to HEAD
3. **Finalize Git**: Commit and push changes (if enabled)

Workers update coordinator-authoritative queue/done paths during integration conflict resolution and must preserve other workers' entries exactly. Task finalization is part of the worker integration loop; no merge-agent subprocess is required.

### CI Failure Recovery Flow

```
CI Failure Detected
        │
        ▼
┌─────────────────┐
│ Retry Count < 2?│
└────────┬────────┘
         │
    ┌────┴────┐
   Yes        No
    │          │
    ▼          ▼
┌─────────────────┐   ┌─────────────────┐
│ Send Continue   │   │ Apply Revert    │
│ with strict     │   │ Mode Policy     │
│ compliance msg  │   └────────┬────────┘
└────────┬────────┘            │
         │                     │
         ▼                     ▼
┌─────────────────┐   ┌─────────────────┐
│ Runner Fixes    │   │ User Choice:    │
│ Issues          │   │ - Revert        │
└────────┬────────┘   │ - Keep          │
         │           │ - Continue      │
         └───────────┴─────────────────┘
```

---

## Configuration Examples

### Conservative (Review Required)

```json
{
  "version": 1,
  "agent": {
    "git_commit_push_enabled": false,
    "git_revert_mode": "ask",
    "ci_gate_enabled": true,
    "ci_gate_command": "make ci"
  }
}
```

**Use Case**: Human review required before every commit.

### Automated CI Pipeline

```json
{
  "version": 1,
  "agent": {
    "git_commit_push_enabled": true,
    "git_revert_mode": "enabled",
    "ci_gate_enabled": true,
    "ci_gate_command": "make ci"
  }
}
```

**Use Case**: Unattended automation where failures should be discarded.

### Local Development

```json
{
  "version": 1,
  "agent": {
    "git_commit_push_enabled": false,
    "git_revert_mode": "disabled",
    "ci_gate_enabled": true
  }
}
```

**Use Case**: Iterative development with manual git operations.

---

## CLI Overrides

Supervision behavior can be overridden per-run:

```bash
# Disable git operations for this run
ralph run one --no-git-commit-push

# Force git operations
ralph run one --git-commit-push-on

# Skip CI gate
ralph run one --no-ci-gate

# Enable strict LFS checking
ralph run one --lfs-check

# Disable notifications
ralph run one --no-notify
```

---

## Troubleshooting

### CI Gate Fails After Runner Completes

**Symptom**: Task implementation succeeds but CI gate fails.

**Resolution**:
1. Check CI output for specific failures
2. Runner will auto-retry up to 2 times with compliance messaging
3. On third failure, choose based on revert mode:
   - `ask`: Review changes, choose to fix, revert, or keep
   - `enabled`: Changes auto-reverted, re-run task
   - `disabled`: Fix issues manually, then `ralph task done {ID}`

### Task Marked Done but Not Committed

**Symptom**: Task in done file but repo has uncommitted changes.

**Cause**: `git_commit_push_enabled` is false or push failed.

**Resolution**:
```bash
# Manually commit and push
git add -A
git commit -m "RQ-0001: Task title"
git push
```

### Integration Loop Blocked (Parallel Mode)

**Symptom**: Parallel worker completes but task remains in queue.

**Resolution**:
```bash
# Inspect worker status/details
ralph run parallel status

# Retry blocked worker integration
ralph run parallel retry --task RQ-0001
```

### Push Fails with No Upstream

**Symptom**: "No upstream configured" warning.

**Resolution**:
```bash
# Set upstream manually
git push -u origin HEAD

# Or use AllowCreateUpstream policy in code
```

---

## See Also

- [Phases](./phases.md) — Multi-phase execution workflow
- [Session Management](./session-management.md) — Crash recovery and resumption
- [Configuration](../configuration.md) — Full configuration reference
- [Workflow](../workflow.md) — High-level workflow documentation
- [Parallel](./parallel.md) — Parallel execution supervision
