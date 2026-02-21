# Ralph's Phase System

Purpose: Document Ralph's multi-phase execution workflow for AI agent task processing.

## Overview

Ralph executes tasks using a **phase-based workflow** that separates planning, implementation, and review into distinct stages. This design enables:

- **Quality control**: Each phase has specific responsibilities and enforcement
- **Iterative refinement**: Plans can be reviewed before implementation
- **CI integration**: Automated validation gates between phases
- **Flexibility**: Choose 1, 2, or 3 phases based on task complexity
- **Crash recovery**: Per-phase session IDs enable resumption after interruptions

The phase system is Ralph's core execution model, designed to balance automation with human oversight for complex software engineering tasks.

## Phase Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Ralph Phase System                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Phase 1: Planning                                                      │
│  ├── Agent analyzes task and creates implementation plan                │
│  ├── Plan cached to .ralph/cache/plans/<TASK_ID>.md                     │
│  └── Enforcement: Plan-only (no code changes allowed)                   │
│                              ↓                                          │
│  Phase 2: Implementation + CI                                           │
│  ├── Agent implements the plan from Phase 1                             │
│  ├── CI gate runs (default: make ci)                                    │
│  └── Stops BEFORE completion (manual review opportunity)                │
│                              ↓                                          │
│  Phase 3: Review + Completion                                           │
│  ├── Agent reviews diff for quality/safety                              │
│  ├── Task must be marked done/rejected                                  │
│  ├── CI gate runs again                                                 │
│  └── Git commit/push if enabled                                         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Phase 1: Planning

### Purpose
Phase 1 is the **thinking phase**. The AI agent analyzes the task requirements and produces a detailed implementation plan without making any code changes.

### Detailed Behavior

**Input**: Task description from queue + context about the codebase

**Process**:
1. Load the Phase 1 planning prompt (`worker_phase1.md`)
2. Build a comprehensive prompt including:
   - Task description
   - Iteration context (if applicable)
   - RepoPrompt instructions (if enabled)
   - Policy reminders
3. Execute the runner with the planning prompt
4. Enforce plan-only constraints

**Output**: Plan cached at `.ralph/cache/plans/<TASK_ID>.md`

### Plan-Only Enforcement

Phase 1 is strictly **read-only** except for:
- `.ralph/queue.json` - Task status updates
- `.ralph/done.json` - Archive operations
- `.ralph/cache/plans/<TASK_ID>.md` - The plan cache itself

**INTENDED BEHAVIOR**: If Phase 1 makes any changes outside these allowed paths, Ralph should detect this and prompt the user for action.

**CURRENTLY IMPLEMENTED BEHAVIOR**: 
- After Phase 1 completes, Ralph checks if the repo is clean (ignoring allowed paths)
- If dirty files are detected, the `git_revert_mode` determines the response:
  - `ask`: Prompt user to revert, keep+continue, or continue with message
  - `enabled`: Automatically revert changes
  - `disabled`: Fail with error
- Baseline dirty paths (if `--force` was used) are tracked and allowed

### Plan Cache Format

The plan is stored as Markdown in `.ralph/cache/plans/<TASK_ID>.md`:

```markdown
# Implementation Plan for RQ-0001

## Analysis
[Agent's analysis of the task]

## Approach
[High-level approach]

## Implementation Steps
1. [Step 1]
2. [Step 2]
...

## Files to Modify
- `path/to/file1.rs` - [Description of changes]
- `path/to/file2.rs` - [Description of changes]

## Testing Strategy
[How to verify the implementation]

## Risks and Considerations
[Potential issues or edge cases]
```

### When Phase 1 is Skipped

- **Single-phase mode** (`--phases 1` or `--quick`): Planning is combined with implementation
- **Multi-iteration runs**: Only the first iteration runs Phase 1; subsequent iterations use the cached plan

## Phase 2: Implementation + CI

### Purpose
Phase 2 is the **doing phase**. The AI agent implements the plan from Phase 1 (or directly implements in single-phase mode).

### Detailed Behavior

**Input**: Task description + cached plan (from Phase 1 or task itself)

**Process**:
1. Load the appropriate prompt based on total phases:
   - **3-phase mode**: `worker_phase2_handoff.md` (includes handoff checklist)
   - **2-phase mode**: `worker_phase2.md` (includes completion checklist)
2. Build the implementation prompt with:
   - Task description
   - Plan text
   - Completion/iteration checklist
   - Phase 2 handoff checklist (3-phase only)
3. Execute the runner
4. Cache the final response for Phase 3 reference
5. Run CI gate
6. Stop BEFORE task completion (supervision point)

**No-Deferrals Policy (Phase 2)**:
- Phase 2 is responsible for fully executing the Phase 1 plan and closing any newly discovered follow-ups, inconsistencies, or test gaps it finds along the way.
- Only true blockers may remain at handoff (this should be rare). If blocked, Phase 2 must list explicit remediation steps (commands/files/expected outcome).

### CI Gate Integration

Phase 2 includes mandatory CI validation (unless disabled):

```bash
# Default CI command
make ci

# Configurable via:
# - CLI: (no direct flag, use config)
# - Config: agent.ci_gate_command
# - Config: agent.ci_gate_enabled (set to false to disable)
```

**CI Auto-Retry Behavior**:
- Ralph automatically retries up to **2 times** on CI failure
- Each retry sends a "strict compliance" message to the runner
- After 2 failures, behavior depends on `git_revert_mode`:
  - `ask`: Prompt user to revert, continue with message, or proceed
  - `enabled`: Automatically revert and retry
  - `disabled`: Fail with error

**INTENDED BEHAVIOR**: Provide automated recovery for transient CI failures while preventing infinite loops.

**CURRENTLY IMPLEMENTED BEHAVIOR**: 
- Exactly 2 automatic retries with strict compliance messaging
- Continue session allows the runner to fix CI errors
- Session ID preserved across retries for context continuity

### Stop-Before-Completion Design

In 3-phase mode, Phase 2 **intentionally stops before marking the task complete**:

1. Code changes are made
2. CI gate passes
3. Task remains in `doing` status
4. Human can review changes before Phase 3

This design provides a **supervision checkpoint** where you can:
- Review the diff manually
- Run additional tests
- Make manual adjustments
- Decide whether to proceed to review or revert

### Phase 2 Final Response Cache

The runner's final response is cached at `.ralph/cache/phase2_final/<TASK_ID>.md` for Phase 3 reference.

## Phase 3: Review + Completion

### Purpose
Phase 3 is the **validation phase**. The AI agent reviews the implemented changes for quality, safety, and completeness before finalizing the task.

### Detailed Behavior

**Input**: 
- Task description
- Code review prompt context (generated from git diff)
- Phase 2 final response (cached)
- Completion checklist

**Process**:
1. Generate code review context from git diff
2. Load `worker_phase3.md` prompt
3. Build Phase 3 prompt with:
   - Task description
   - Code review body
   - Phase 2 final response
   - Completion checklist
   - Phase 3 completion guidance
4. Execute the runner
5. Check for completion signal
6. Run CI gate
7. Finalize task (commit/push if enabled)

### Completion Requirements

Phase 3 **requires** the task to be marked with a terminal status:

```bash
# In Phase 3, the runner must execute:
ralph task done <TASK_ID> [--note "completion notes"]
# OR
ralph task reject <TASK_ID> [--note "rejection reason"]
```

**Enforcement**:
- Phase 3 loops until the task is archived to `done.json`
- If task is not done/rejected, user is prompted based on `git_revert_mode`
- With `git_commit_push_enabled=true`, rejected tasks allow dirty files in `.ralph/queue.{json,jsonc}`, `.ralph/done.{json,jsonc}`, `.ralph/config.{json,jsonc}`, and `.ralph/cache/`

### Code Review Context

Phase 3 generates a code review prompt that includes:
- Summary of changes (from git diff)
- Files modified
- Potential risks or suspicious patterns
- Questions for the reviewer

### Completion Signals

When the runner marks a task done/rejected in **sequential mode**, Ralph writes a completion signal:
- Location: `.ralph/cache/completions/<TASK_ID>.json`
- Contains: status, notes, runner_used, model_used
- Used for: Analytics, webhook events, custom fields patching

**Note:** In parallel mode, task finalization is handled by the worker integration loop (direct push to coordinator target branch). No merge-agent subprocess is involved.

## Single-Phase Mode

### When to Use

Use `--phases 1` (or `--quick`) for:
- **Quick fixes** (typo corrections, small refactors)
- **Simple tasks** (add a log line, update a comment)
- **Urgent patches** (skip planning overhead)
- **Exploratory work** (spike/prototype tasks)

### Behavior

Single-phase mode combines planning and implementation:

1. Uses `worker_single_phase.md` prompt
2. No separate planning phase
3. No Phase 2 stop-before-completion
4. CI gate runs after implementation
5. Task must still be marked done/rejected

### Example

```bash
# Quick fix for a typo
ralph run one --phases 1

# Equivalent to:
ralph run one --quick
```

## Two-Phase Mode

### When to Use

Use `--phases 2` for:
- **Medium complexity tasks** (new feature, moderate refactoring)
- **Tasks needing planning** but not separate review
- **Faster iteration** (skip Phase 3 review)
- **Trusted implementations** (CI gate is sufficient validation)

### Behavior

Two-phase mode includes planning and implementation without separate review:

1. Phase 1: Planning (plan cached)
2. Phase 2: Implementation with completion checklist
3. CI gate runs
4. Task completion happens in Phase 2
5. No Phase 3 review phase

### Comparison with 3-Phase

| Aspect | 2-Phase | 3-Phase |
|--------|---------|---------|
| Planning | Yes | Yes |
| Implementation | Yes | Yes |
| CI Gate | Yes (Phase 2) | Yes (Phase 2 & 3) |
| Review Phase | No | Yes |
| Stop-before-completion | No | Yes (Phase 2) |
| Completion | Phase 2 | Phase 3 |

## Configuration

### Setting Default Phases

Configure the default number of phases in `.ralph/config.json`:

```json
{
  "version": 1,
  "agent": {
    "phases": 3
  }
}
```

Valid values: `1`, `2`, or `3`

### Built-in Profiles

Ralph includes phase-optimized profiles:

```bash
# Quick profile: 1 phase, kimi runner
ralph run one --profile quick

# Thorough profile: 3 phases, claude/opus
ralph run one --profile thorough
```

Profile definitions (always available):
- `quick`: `phases=1`, `runner=kimi`, `model=kimi-for-coding`
- `thorough`: `phases=3`, `runner=claude`, `model=opus`

### Custom Profiles

Define your own phase configurations:

```json
{
  "version": 1,
  "profiles": {
    "fast-review": {
      "phases": 2,
      "runner": "codex",
      "model": "gpt-5.2-codex"
    },
    "deep-think": {
      "phases": 3,
      "runner": "claude",
      "model": "opus",
      "reasoning_effort": "high"
    }
  }
}
```

## Per-Phase Overrides

### CLI Flags

Override runner, model, or reasoning effort for specific phases:

```bash
# Phase 1: Use powerful model for planning
ralph run one --runner-phase1 codex --model-phase1 gpt-5.3-codex --effort-phase1 high

# Phase 2: Use fast model for implementation
ralph run one --runner-phase2 kimi --model-phase2 kimi-for-coding

# Phase 3: Use thorough model for review
ralph run one --runner-phase3 claude --model-phase3 opus --effort-phase3 high
```

### Configuration

Set per-phase overrides in `.ralph/config.json`:

```json
{
  "version": 1,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "phase_overrides": {
      "phase1": {
        "model": "gpt-5.2",
        "reasoning_effort": "high"
      },
      "phase2": {
        "runner": "kimi",
        "model": "kimi-code/kimi-for-coding"
      },
      "phase3": {
        "runner": "claude",
        "model": "claude-opus-4",
        "reasoning_effort": "high"
      }
    }
  }
}
```

### Override Precedence

Per-phase settings resolve in this order (highest to lowest):

1. **CLI phase flags** (`--runner-phase1`, `--model-phase1`, etc.)
2. **Config phase overrides** (`agent.phase_overrides.phaseN.*`)
3. **CLI global overrides** (`--runner`, `--model`, `--effort`)
4. **Task overrides** (`task.agent.*` in queue)
5. **Config defaults** (`agent.*`)
6. **Code defaults**

### Unused Override Warnings

Ralph warns when phase overrides won't be used:

```bash
# This will warn about unused phase3 overrides
ralph run one --phases 2 --runner-phase3 claude
# Warning: Phase 3 overrides specified but phases=2
```

## Session Management

### Session ID Format

Ralph generates unique session IDs for crash recovery:

```
Format: {task_id}-p{phase}-{timestamp}
Example: RQ-0001-p2-1704153600
```

- `task_id`: The task being executed (e.g., `RQ-0001`)
- `phase`: Phase number (`0` for single-phase, `1`, `2`, or `3`)
- `timestamp`: Unix epoch seconds when the phase started

### Why Per-Phase Sessions?

Each phase gets its own session ID for:
- **Isolation**: Prevents context leakage between planning, implementation, and review
- **Determinism**: Same session ID always resumes the same phase context
- **Debuggability**: Human-readable IDs trace session lifecycle
- **Crash recovery**: Resume exactly where the interruption occurred

### Runner Support

Session management is primarily for **Kimi** (which doesn't emit session IDs in output):

| Runner | Session Management |
|--------|-------------------|
| Kimi | Ralph-managed IDs |
| Others | Runner-managed or not supported |

### Crash Recovery Flow

```
1. Phase 2 starts → Generate session ID: RQ-0001-p2-1704153600
2. Runner executes with --session RQ-0001-p2-1704153600
3. CI fails → Continue session stored
4. Crash or interruption
5. User runs: ralph run resume
6. Ralph detects Phase 2 session, resumes with same ID
7. Runner continues from where it left off
```

### Continue Session State

Session state is persisted to `.ralph/cache/session.json`:

```json
{
  "task_id": "RQ-0001",
  "phase": 2,
  "session_id": "RQ-0001-p2-1704153600",
  "runner": "kimi",
  "model": "kimi-for-coding",
  "ci_failure_retry_count": 1
}
```

### Session Timeout

Sessions older than `session_timeout_hours` (default: 24) are considered stale:

```json
{
  "agent": {
    "session_timeout_hours": 72
  }
}
```

Stale sessions require explicit `--force` to resume.

## Phase Transitions

### How Ralph Detects Phase Changes

Ralph tracks phase progress internally through the `PhaseInvocation` context:

1. **Phase 1 → Phase 2**: 
   - Phase 1 returns plan text
   - Orchestrator calls Phase 2 with the plan
   - New session ID generated for Phase 2

2. **Phase 2 → Phase 3** (3-phase mode):
   - Phase 2 completes with CI gate passing
   - Task remains `doing`
   - Phase 3 starts with review context
   - New session ID generated for Phase 3

3. **Phase 2 Completion** (2-phase mode):
   - Phase 2 completes with CI gate passing
   - Task marked done/rejected
   - Post-run supervision finalizes

### Webhook Events

Phase transitions emit webhook events (opt-in):

```json
{
  "event": "phase_started",
  "task_id": "RQ-0001",
  "phase": 2,
  "phase_count": 3,
  "runner": "kimi",
  "model": "kimi-for-coding"
}
```

```json
{
  "event": "phase_completed",
  "task_id": "RQ-0001",
  "phase": 2,
  "phase_count": 3,
  "duration_ms": 12500,
  "ci_gate": "passed"
}
```

Enable in config:

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://example.com/webhook",
      "events": ["phase_started", "phase_completed"]
    }
  }
}
```

## Practical Examples

### Example 1: Quick Bug Fix (1 Phase)

```bash
# Fix a typo - no need for full 3-phase process
ralph run one --phases 1

# Or use the quick alias
ralph run one --quick
```

### Example 2: New Feature with Planning (2 Phases)

```bash
# Plan and implement, skip review phase
ralph run one --phases 2
```

### Example 3: Critical Change with Full Review (3 Phases)

```bash
# Full workflow with review
ralph run one --phases 3
```

### Example 4: Mixed Runner Workflow

```bash
# Use Codex for planning (strong reasoning)
# Use Kimi for implementation (fast)
# Use Claude for review (thorough analysis)
ralph run one \
  --runner-phase1 codex --model-phase1 gpt-5.3-codex --effort-phase1 high \
  --runner-phase2 kimi --model-phase2 kimi-for-coding \
  --runner-phase3 claude --model-phase3 opus
```

### Example 5: Per-Phase Reasoning Effort (Codex)

```bash
# High effort for planning, medium for implementation, high for review
ralph run one --runner codex \
  --effort-phase1 high \
  --effort-phase2 medium \
  --effort-phase3 high
```

### Example 6: Configuration-Based Workflow

```json
// .ralph/config.json
{
  "version": 1,
  "agent": {
    "phases": 3,
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "phase_overrides": {
      "phase1": {
        "reasoning_effort": "high"
      },
      "phase3": {
        "runner": "claude",
        "model": "sonnet"
      }
    }
  }
}
```

Then simply run:

```bash
ralph run one  # Uses 3-phase with overrides from config
```

## Prompt Overrides

Each phase uses specific prompt templates that can be overridden:

| Phase | Default Prompt | Override Path |
|-------|---------------|---------------|
| 1 | `worker_phase1.md` | `.ralph/prompts/worker_phase1.md` |
| 2 (2-phase) | `worker_phase2.md` | `.ralph/prompts/worker_phase2.md` |
| 2 (3-phase) | `worker_phase2_handoff.md` | `.ralph/prompts/worker_phase2_handoff.md` |
| 3 | `worker_phase3.md` | `.ralph/prompts/worker_phase3.md` |
| Single | `worker_single_phase.md` | `.ralph/prompts/worker_single_phase.md` |

Override prompts must preserve required placeholders (e.g., `{{USER_REQUEST}}`).

## Troubleshooting

### Phase 1 Violations

If Phase 1 makes unexpected changes:

```
Error: Phase 1 violated plan-only contract
```

**Solutions**:
- Check if the runner is configured correctly
- Review the runner output for unintended edits
- Use `--git-revert-mode ask` to manually decide
- Add `--force` with `--allow-dirty` if baseline dirty paths are expected

### Phase 3 Not Completing

If Phase 3 loops indefinitely:

```
Phase 3 incomplete: task RQ-0001 is not archived with a terminal status
```

**Solutions**:
- Ensure the runner executes `ralph task done` or `ralph task reject`
- Check if the runner has proper queue file permissions
- Use `--git-revert-mode ask` to manually intervene

### Session Mismatch

If resuming fails with session errors:

```
ralph run resume --force
```

The `--force` flag bypasses stale session checks.

### CI Gate Failures

If CI repeatedly fails in Phase 2 or 3:

1. Check CI output: `make ci` (or your configured command)
2. Ralph will auto-retry twice with strict compliance messaging
3. After 2 failures, choose to:
   - Revert and try again
   - Continue with a message to the runner
   - Proceed without fixing (not recommended)

## See Also

- [Workflow](../workflow.md) - High-level workflow documentation
- [Configuration](../configuration.md) - Full configuration reference
- [Queue and Tasks](../queue-and-tasks.md) - Task lifecycle documentation
