# Parallel Mode Rewrite Specification

Status: Draft  
Last updated: 2026-02-17  
Owners: Ralph maintainers

## 1. Purpose

This document defines the target design for rewriting Ralph parallel mode so it is reliable, debuggable, and implementation-ready.

This is a specification, not an investigation brief. It defines required behavior, interfaces, state invariants, failure handling, and acceptance criteria for implementation.

## 2. Problem Statement

Current parallel mode depends on completion-signal files, a background merge-runner thread, and complex persisted state reconciliation. In practice this creates high manual intervention, fragile recovery paths, and hard-to-debug failure modes.

Primary pain points:
- Queue/done synchronization depends on completion-signal transport across workspaces.
- Merge behavior is constrained by an internal thread with limited control surface.
- Persisted parallel state has multiple blocker classes that can require manual cleanup.
- Recovery/debug often requires internal state inspection instead of explicit CLI workflows.

## 3. Goals

- Preserve parallel task throughput with isolated per-task workspaces.
- Make merge and queue-finalization behavior deterministic and inspectable.
- Remove completion-signal dependency from the architecture.
- Replace internal merge thread orchestration with explicit subprocess command boundaries.
- Keep worker execution behavior aligned with existing sequential task execution semantics.
- Reduce persisted coordination state to only what is needed for crash-safe orchestration.

## 4. Non-Goals

- No changes to sequential `ralph run one` behavior outside required parallel integration points.
- No redesign of phase prompts, phase semantics, or runner provider integrations.
- No redesign of task selection policy beyond what is necessary to support the new merge pipeline.
- No requirement to preserve legacy completion-signal state compatibility.

## 5. Scope

In scope:
- `ralph run loop --parallel` orchestration rewrite.
- New merge-agent command surface under `ralph run`.
- Parallel state model rewrite and migration behavior.
- Queue/done update path redesign.
- Tests and docs required to validate the new architecture.

Out of scope:
- Webhooks/notifications redesign.
- PR provider abstraction changes.
- Any remote CI workflow introduction (local `make ci` and `make macos-ci` remain the gates).

## 6. Terms

- Coordinator: The parent `ralph run loop --parallel` process.
- Worker: A subprocess executing one task in an isolated workspace clone.
- Merge Agent: A subprocess command that merges an already-created PR and finalizes task state in the coordinator repo.
- Workspace: A task-specific git clone under `parallel.workspace_root`.
- Canonical queue state: `.ralph/queue.json` and `.ralph/done.json` in the coordinator repo root.

## 7. Baseline Constraints (Current System Inputs)

The rewrite must honor these existing system boundaries unless explicitly changed in this spec:
- Parallel worker count and related settings still originate from CLI/config resolution used by `run_loop`.
- Workers execute task phases using existing run-one flow with `--parallel-worker`.
- Workspace isolation remains mandatory for worker execution (explicit worker CWD + coordinator path flags).
- Queue and done files are canonical in the coordinator repo, not in worker clones.

## 8. Target Architecture

### 8.1 High-Level Model

1. Coordinator selects and dispatches tasks to worker subprocesses up to configured concurrency.
2. Worker completes task execution and creates/updates PR using existing PR automation behavior.
3. Coordinator enqueues successful worker outputs for merge processing.
4. Coordinator invokes merge-agent subprocess per eligible PR.
5. Merge-agent performs merge + task finalization in coordinator repo context.
6. Coordinator records result, updates minimal state, and schedules next work.

No completion-signal file exchange is used in this target architecture.

### 8.2 Required Process Boundaries

- Worker subprocess:
  - Runs task execution only.
  - Does not perform canonical queue/done mutation in coordinator repo.
  - Must remain non-interactive in parallel mode.

- Merge-agent subprocess:
  - Accepts explicit task and PR identity as arguments.
  - Executes merge and canonical queue/done finalization.
  - Returns machine-parseable status to coordinator.

### 8.3 Coordinator Responsibilities

Coordinator MUST:
- Hold coordination lock semantics equivalent to current parallel safety guarantees.
- Track active workers and pending merge jobs.
- Stop scheduling new tasks when stop/abort conditions are reached.
- Continue draining in-flight merge-agent operations safely before exit (unless hard abort).
- Persist restart-safe minimal state.

Coordinator MUST NOT:
- Depend on completion-signal files for queue synchronization.
- Perform implicit queue reconciliation from worker workspace artifacts.

## 9. CLI Contract

### 9.1 New Command

Add a new explicit command:

```bash
ralph run merge-agent --task <TASK_ID> --pr <PR_NUMBER>
```

Required arguments:
- `--task <TASK_ID>`
- `--pr <PR_NUMBER>`

Expected behavior:
- Validates task/PR existence and merge eligibility.
- Performs merge according to configured merge policy.
- Finalizes task in canonical queue/done state (`ralph task done` semantics in coordinator repo context).
- Emits structured result to stdout.
- Writes user-facing diagnostics to stderr.

Exit codes:
- `0`: merge + task finalization successful.
- `1`: runtime/unexpected failure.
- `2`: usage/validation failure.
- `>=3`: documented domain-specific failure classes (if introduced).

### 9.2 Existing Commands

- `ralph run loop --parallel [N]` remains user entrypoint.
- `--resume` behavior in parallel mode remains unsupported unless explicitly added as part of this rewrite.
- Any CLI behavior change must be reflected in `docs/cli.md` and `docs/features/parallel.md`.

## 10. Configuration Contract

### 10.1 Preserve

Preserve existing parallel controls that remain meaningful:
- `workers`
- workspace root/prefix controls
- merge policy controls (`merge_when`, `merge_method`, conflict policy, retry limits)
- branch cleanup controls

### 10.2 Replace/Remove

Completion-signal-driven behavior is removed. Any config/state knobs that exist solely for completion-signal transport MUST be removed, along with runtime usage and tests.

If merge-agent-specific runner/model overrides are retained, they MUST be explicitly documented as applying to the merge-agent subprocess only.

### 10.3 Compatibility Policy

This rewrite does not preserve backward compatibility for removed completion-signal internals. Legacy state/config tied to that mechanism MAY be dropped or migrated forward one-way; no shim requirement exists.

## 11. State Model

### 11.1 File Location

Parallel state remains at:
- `.ralph/cache/parallel/state.json`

### 11.2 Required Contents (Minimal)

State MUST include only data required for safe restart/recovery:
- active task records (task id, workspace path, pid/process metadata, branch)
- pending merge records (task id, pr number, lifecycle marker)
- coordinator metadata required for compatibility checks (base branch, started_at, schema version)

State MUST NOT include:
- completion-signal bookkeeping
- synthetic queue/done payload snapshots
- blocker classes that can be recomputed deterministically from GitHub/PR/task status

### 11.3 Invariants

- One active worker per task id.
- One pending merge entry per task id.
- A task cannot be both active and merged.
- Persisted state is durable across coordinator crash/restart and recoverable without manual JSON edits in normal failure cases.

## 12. Control Flow Specification

### 12.1 Worker Lifecycle

1. Select eligible task.
2. Create/sync workspace.
3. Spawn worker subprocess with parallel-worker flags and repo-root isolation env.
4. Observe worker completion.
5. If worker success and PR exists, enqueue merge-agent job.
6. If worker failure, record failure and apply existing retry/abort policy.

### 12.2 Merge Lifecycle

1. Dequeue next merge job according to configured merge ordering policy.
2. Invoke `ralph run merge-agent --task ... --pr ...`.
3. Parse structured result.
4. On success: mark task merged/finalized and clean branch/workspace immediately.
5. On failure due to unresolved conflicts: leave PR open, persist retryable failure state, and continue the loop.
6. On other failures: persist actionable failure state and apply existing failure/abort policy.

### 12.3 Stop/Abort Semantics

- User stop signal:
  - Stop dispatching new workers.
  - Continue handling already-running workers and merge jobs unless configured for immediate hard stop.
- User-intent abort reasons (for example Ctrl+C, explicit revert intent) must be typed and handled before generic consecutive-failure escalation.
- Coordinator exit must leave state consistent for restart.

## 13. Failure Handling and Recovery

Required behaviors:
- Merge-agent failure leaves enough state to retry merge-agent manually or via coordinator retry.
- Idempotent task finalization: rerunning merge-agent after partial success must not duplicate done entries or corrupt queue ordering.
- Missing/invalid PR references are surfaced as typed validation errors.
- Workspace cleanup failures are non-fatal but logged with explicit retry path.
- Post-merge CI is not re-run by merge-agent; worker CI is the authoritative gate.

## 14. Data Consistency Rules

- Canonical queue mutation happens only in coordinator repo context.
- Worker workspace queue files, if present, are non-authoritative.
- Queue/done updates must be atomic at file level (write-temp + rename semantics).
- Any productivity/metrics side effects must be committed in the same logical transaction as task finalization or rolled back cleanly.

## 15. Observability Requirements

Coordinator and merge-agent logs must allow deterministic incident reconstruction:
- task id
- pr number
- workspace path
- subprocess command and exit code
- merge decision/result
- finalization decision/result

At minimum, one machine-readable log/event output path must exist for merge-agent results.

## 16. Security and Safety Requirements

- No secret values in logs.
- Merge-agent must run with explicit repo-root context; no implicit cwd fallback to unrelated repos.
- Parallel subprocesses remain non-interactive unless explicitly allowed by configuration.

## 17. Implementation Plan (Normative Sequence)

1. Introduce `ralph run merge-agent` command with help text, structured output, and exit codes.
2. Implement merge-agent execution path for merge + task finalization in coordinator repo.
3. Rewire parallel coordinator to use merge-agent subprocess jobs.
4. Remove completion-signal production/consumption paths.
5. Simplify parallel state schema and loader/reconciler logic.
6. Update tests across unit/integration boundaries.
7. Update docs (`docs/features/parallel.md`, `docs/cli.md`, related feature pages).

## 18. Verification Matrix

Minimum required verification before completion:
- Unit tests for:
  - merge-agent argument validation
  - merge-agent success/failure exit and output contracts
  - state transitions for worker->pending-merge->merged/failed
  - idempotent task finalization
- Integration tests for:
  - two-task parallel run with successful merges
  - merge conflict path with retry/escalation behavior
  - interrupted run recovery from persisted state
  - queue/done canonical updates without completion signals
- Full local gates:
  - `make ci`
  - `make macos-ci`

## 19. Acceptance Criteria

The rewrite is accepted when all conditions are true:
- `ralph run loop --parallel 2` completes representative multi-task flows without completion-signal artifacts.
- Queue/done finalization for merged tasks is automatic and deterministic.
- Merge operations are executable manually through `ralph run merge-agent`.
- No manual JSON state surgery is required for normal recoverable failure cases.
- Docs and tests reflect the new architecture and pass local CI gates.

## 20. Resolved Product Decisions

The following decisions are fixed for this rewrite:

1. Post-merge CI policy: rely on per-worker CI only; merge-agent does not run post-merge CI.

2. Unresolved conflict policy: leave PR open, persist retryable failure, and continue loop execution.

3. Workspace retention policy: delete workspace immediately after successful merge finalization.

## 21. Identified Issues in the Prior Briefing (Now Resolved in This Spec)

- The prior document mixed transcript, chronology, and implementation details without normative requirements.
- Critical contracts (CLI, state schema, failure semantics, acceptance criteria) were implicit or missing.
- Open questions were not tied to default behavior or explicit owner decisions.
- Migration/removal strategy for completion-signal architecture was not specified as a hard contract.
