# Session Management

Ralph's session management system provides **crash recovery** and **resume capability** for long-running agent tasks. When a task is interrupted—whether by system crash, network failure, or manual termination—session state is persisted to enable seamless resumption without losing progress.

---

## Overview

### Purpose

Session management serves two primary purposes:

1. **Crash Recovery**: Automatically detect and resume interrupted tasks after unexpected failures
2. **CI Gate Retry**: Continue a runner session when CI failures require additional iterations

### Key Features

| Feature | Description |
|---------|-------------|
| **Automatic Recovery** | Detect incomplete sessions on startup and offer to resume |
| **Per-Phase Isolation** | Each phase gets its own session ID to prevent context leakage |
| **Configurable Timeout** | Sessions older than `session_timeout_hours` require explicit confirmation |
| **Runner Integration** | Native session support for Kimi via explicit `--session` flags |
| **Atomic Persistence** | Session state is written atomically to prevent corruption |

---

## Session State File

### Location

Session state is persisted to:

```
.ralph/cache/session.json
```

This file is created when a task starts and cleared when it completes successfully or is explicitly rejected.

### Structure

```json
{
  "version": 1,
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "task_id": "RQ-0001",
  "run_started_at": "2026-02-07T10:00:00.000000000Z",
  "last_updated_at": "2026-02-07T10:30:00.000000000Z",
  "iterations_planned": 2,
  "iterations_completed": 1,
  "current_phase": 2,
  "runner": "claude",
  "model": "sonnet",
  "tasks_completed_in_loop": 0,
  "max_tasks": 10,
  "git_head_commit": "abc123def456",
  "phase1_settings": {
    "runner": "claude",
    "model": "sonnet",
    "reasoning_effort": null
  },
  "phase2_settings": {
    "runner": "codex",
    "model": "o3-mini",
    "reasoning_effort": "high"
  },
  "phase3_settings": {
    "runner": "claude",
    "model": "haiku",
    "reasoning_effort": null
  }
}
```

### Field Descriptions

| Field | Type | Description |
|-------|------|-------------|
| `version` | `u32` | Schema version for forward compatibility |
| `session_id` | `string` | Unique UUID v4 for this run session |
| `task_id` | `string` | The task being executed (e.g., `RQ-0001`) |
| `run_started_at` | `string` | When the session began (RFC3339 UTC) |
| `last_updated_at` | `string` | When the session was last updated (RFC3339 UTC) |
| `iterations_planned` | `u8` | Total iterations configured for the task |
| `iterations_completed` | `u8` | Iterations finished so far |
| `current_phase` | `u8` | Active phase (1, 2, or 3) |
| `runner` | `string` | Primary runner for this session |
| `model` | `string` | Primary model for this session |
| `tasks_completed_in_loop` | `u32` | Tasks finished in current loop session |
| `max_tasks` | `u32` | Maximum tasks to run (0 = unlimited) |
| `git_head_commit` | `string?` | Git HEAD at session start (for validation) |
| `phase1_settings` | `object?` | Phase 1 runner/model (display/logging only) |
| `phase2_settings` | `object?` | Phase 2 runner/model (display/logging only) |
| `phase3_settings` | `object?` | Phase 3 runner/model (display/logging only) |

> **Note**: Per-phase settings are **informational only**. Crash recovery recomputes settings from CLI flags, config, and task overrides to ensure consistency.

---

## Session ID Format

### Run Session ID

The main session state uses a UUID v4 for uniqueness:

```
550e8400-e29b-41d4-a716-446655440000
```

### Runner Session ID (Per-Phase)

For runners that support session resumption (notably **Kimi**), Ralph generates deterministic session IDs:

**Format:**
```
{task_id}-p{phase}-{timestamp}
```

**Example:**
```
RQ-0001-p2-1704153600
```

| Component | Description |
|-----------|-------------|
| `task_id` | Task identifier (e.g., `RQ-0001`) |
| `p{phase}` | Phase number (`p1`, `p2`, `p3`) |
| `timestamp` | Unix epoch seconds at phase start |

> **Design Note**: The timestamp ensures uniqueness even if the same task is run multiple times, while the human-readable prefix makes debugging easier.

---

## Per-Phase Sessions

### Why Each Phase Gets Its Own Session

Ralph generates a new session ID for each phase to ensure **context isolation**:

| Phase | Purpose | Session Isolation Benefit |
|-------|---------|---------------------------|
| **Phase 1** | Planning | Prevents implementation details from polluting plan context |
| **Phase 2** | Implementation | Fresh context for code changes, CI feedback loop |
| **Phase 3** | Review | Clean slate for objective code review |

### Benefits

1. **Deterministic**: Same ID always resumes the same session
2. **Reliable**: No dependency on parsing JSON output or runner-specific `last_session_id` tracking
3. **Debuggable**: Human-readable IDs make it easy to trace session lifecycle
4. **Isolated**: Context from one phase cannot leak into another

### Example Session IDs for a Task

```
# Phase 1 (Planning)
RQ-0001-p1-1704153600

# Phase 2 (Implementation)  
RQ-0001-p2-1704157200

# Phase 3 (Review)
RQ-0001-p3-1704160800
```

---

## Crash Recovery Flow

### 1. Session Detection

When Ralph starts (`ralph run loop` or `ralph run one`), it checks for an existing session:

```rust
match session::check_session(&cache_dir, &queue_file, session_timeout_hours)? {
    SessionValidationResult::NoSession => // Start fresh
    SessionValidationResult::Valid(session) => // Offer to resume
    SessionValidationResult::Stale { reason } => // Clear and start fresh
    SessionValidationResult::Timeout { hours, session } => // Warn and prompt
}
```

### 2. Validation Checks

Before offering to resume, Ralph validates the session:

| Check | Failure Result |
|-------|----------------|
| Task exists in queue | `Stale` - Task no longer exists |
| Task status is `Doing` | `Stale` - Task not in progress |
| Session age < timeout | `Timeout` - Session too old |

### 3. Recovery Prompt

For valid sessions, Ralph displays:

```
╔══════════════════════════════════════════════════════════════╗
║  Incomplete session detected                                 ║
╠══════════════════════════════════════════════════════════════╣
║  Task:        RQ-0001                                        ║
║  Started:     2026-02-07T10:00:00.000000000Z                ║
║  Iterations:  1/2                                            ║
║  Phase:       2                                              ║
╠══════════════════════════════════════════════════════════════╣
║  Phase Settings:                                             ║
║    Phase 1:   Claude/sonnet                                  ║
║    Phase 2:   Codex/gpt-5.2-codex, effort=High               ║
║    Phase 3:   Claude/sonnet                                  ║
╚══════════════════════════════════════════════════════════════╝

Resume this session? [Y/n]:
```

### 4. Resume Execution

If confirmed:

1. **Lock Handling**: Clear any stale queue lock from previous crash
2. **Task Restoration**: Resume from saved `task_id` and `current_phase`
3. **Progress Continuation**: Maintain `tasks_completed_in_loop` counter
4. **Runner Resumption**: Use saved session ID for runner continue operations

---

## Session Timeout

### Default Behavior

Sessions older than **24 hours** are considered stale by default and require explicit confirmation to resume.

### Configuration

Configure the timeout in `.ralph/config.jsonc`:

```json
{
  "agent": {
    "session_timeout_hours": 72
  }
}
```

Or use the default (24 hours) by omitting the field or setting to `null`.

### Timeout Warning

When a session exceeds the timeout threshold:

```
╔══════════════════════════════════════════════════════════════╗
║  STALE session detected (48 hours old)                       ║
╠══════════════════════════════════════════════════════════════╣
║  Task:        RQ-0001                                        ║
║  Started:     2026-02-05T10:00:00.000000000Z                ║
║  Last update: 2026-02-05T10:30:00.000000000Z                ║
║  Iterations:  1/2                                            ║
╚══════════════════════════════════════════════════════════════╝

Warning: This session is older than 24 hours.
Resume anyway? [y/N]:
```

> **Safety Note**: The default `N` (no) protects against accidentally resuming very old sessions where repository state may have changed significantly.

### Disabling Timeout

To disable timeout checking (not recommended for production):

```json
{
  "agent": {
    "session_timeout_hours": null
  }
}
```

---

## Resume Behavior

### Automatic Resume

Use the `--resume` flag to auto-resume without prompting:

```bash
# Auto-resume interrupted session
ralph run loop --resume

# Resume and target a specific task
ralph run one --id RQ-0001 --resume
```

When `--resume` is specified:
- Valid sessions are resumed immediately
- Stale/timeout sessions are still cleared (safety measure)

### Manual Resume

Without `--resume`, Ralph prompts interactively:

| Session State | Behavior |
|---------------|----------|
| **Valid** | Prompt: "Resume this session? [Y/n]" |
| **Stale** | Log info and clear session; start fresh |
| **Timeout** | Prompt: "Resume anyway? [y/N]" |

### Lock Handling During Resume

When resuming a session, Ralph preemptively clears stale queue locks:

```rust
if resume_task_id.is_some() {
    clear_stale_queue_lock_for_resume(&resolved.repo_root)?;
}
```

This handles cases where a previous Ralph process crashed and left behind a lock file.

---

## Non-Interactive Mode

### Behavior

In CI environments or when `--non-interactive` is specified:

| Session State | Result |
|---------------|--------|
| **Valid** | Returns `false` (do not resume) - safe default |
| **Timeout** | Returns `false` (do not resume) - safe default |

This prevents CI jobs from hanging on interactive prompts.

### Recommended CI Patterns

```bash
# Option 1: Don't resume in CI (safest)
ralph run loop --non-interactive

# Option 2: Auto-resume with explicit flag
ralph run loop --non-interactive --resume

# Option 3: Force fresh start
ralph run loop --non-interactive --force
```

### Scripting

For scripts that need to handle sessions:

```bash
# Check for existing session without prompting
if ralph run loop --non-interactive 2>&1 | grep -q "Incomplete session"; then
    echo "Previous session detected - handling manually"
    # Custom logic here
fi
```

---

## Runner-Specific Handling

### Kimi Session Support

Ralph provides first-class session support for **Kimi** via explicit `--session` flags:

**Initial Invocation:**
```bash
kimi --print --output-format stream-json --model kimi-for-coding \
  --session RQ-0001-p2-1704153600 \
  --prompt "Implement the plan..."
```

**Continue (CI Failure Retry):**
```bash
kimi --print --output-format stream-json --model kimi-for-coding \
  --session RQ-0001-p2-1704153600 \
  --prompt "Fix the CI errors..."
```

### Runner Support Matrix

| Runner | Session Support | Mechanism |
|--------|----------------|-----------|
| **Kimi** | ✅ Full | `--session {id}` flag |
| **Claude** | ❌ None | N/A |
| **Codex** | ❌ None | N/A |
| **OpenCode** | ❌ None | N/A |
| **Gemini** | ❌ None | N/A |
| **Cursor** | ❌ None | N/A |

### Implementation Details

Kimi session IDs are generated in `crates/ralph/src/commands/run/phases/mod.rs`:

```rust
pub(crate) fn generate_phase_session_id(task_id: &str, phase: u8) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}-p{}-{}", task_id, phase, timestamp)
}
```

The session ID is:
1. Generated at phase start
2. Reused for all continue operations within that phase
3. Stored in `ContinueSession` for CI gate retry loops

### Risk Acknowledgment

> **Important**: If Kimi crashes and the session becomes corrupted, Ralph will attempt to resume with the same ID. The user accepts this risk by confirming session resumption.

---

## Best Practices

### For Interactive Use

1. **Review before resuming**: Always check the session details before confirming
2. **Consider timeout**: For long-running tasks, increase `session_timeout_hours`
3. **Check git state**: Ensure the repository is in a reasonable state before resuming

### For CI/CD

1. **Use `--non-interactive`**: Prevent hanging on prompts
2. **Decide resume policy**: Either always use `--resume` or never resume in CI
3. **Clean workspace**: Ensure fresh state for CI runs

### Configuration Recommendations

```json
{
  "agent": {
    "session_timeout_hours": 24
  }
}
```

| Use Case | Recommended Timeout |
|----------|---------------------|
| Daily development | 24 hours (default) |
| Weekend tasks | 72 hours |
| Long-running analysis | 168 hours (1 week) |
| CI/CD environments | 1 hour or disable |

---

## Troubleshooting

### Session Not Resuming

**Symptom**: Session is detected but not offered for resume

**Causes**:
1. Task status changed from `Doing` to `Todo` or `Done`
2. Session timed out and wasn't confirmed
3. Non-interactive mode defaults to not resuming

**Solutions**:
```bash
# Check task status
ralph queue list

# Force start fresh
ralph run loop --force

# Or clear session manually
rm .ralph/cache/session.json
```

### Stale Session Warning

**Symptom**: "Stale session cleared: Task RQ-0001 is not in Doing status"

**Cause**: The task was marked done/rejected while Ralph wasn't running

**Solution**: This is normal behavior. Start fresh with the task in `Todo` status if needed.

### Session Timeout in CI

**Symptom**: CI jobs fail with timeout warnings

**Solution**:
```bash
# Either auto-resume
ralph run loop --non-interactive --resume

# Or use shorter timeout
ralph run loop --non-interactive  # with session_timeout_hours: 1 in config
```

---

## See Also

- [Workflow](../workflow.md) - High-level execution flow
- [Configuration](../configuration.md) - Session timeout configuration
- [Phases](./phases.md) - Phase execution details
- [Runners](./runners.md) - Runner-specific behavior
