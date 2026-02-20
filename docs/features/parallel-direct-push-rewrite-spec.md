# Parallel Direct-Push Rewrite Specification

Status: Draft  
Last updated: February 20, 2026  
Owners: Ralph maintainers

## 1. Purpose

This document defines a full rewrite of Ralph parallel mode to remove PR-driven orchestration and move to direct pushes onto the user's current base branch.

This specification is self-contained. It assumes no prior knowledge of Ralph, no prior architecture context, and no prior discussion history.

## 2. Product Philosophy (Normative)

The product direction for this rewrite is:

1. Maximize AI-agent autonomy during execution and integration.
2. Minimize deterministic coordinator logic that attempts to "reason" about complex, changing code states.
3. Keep deterministic logic only where strict correctness/safety boundaries are required.

Practical interpretation:

1. The coordinator is a scheduler/tracker, not a merge decision engine.
2. Agents resolve integration complexity through targeted prompts and contextual handoff data.
3. Deterministic checks remain mandatory only for:
   1. unresolved git conflicts,
   2. queue/done semantic validity,
   3. CI gate success,
   4. push success/failure classification.

## 2.1 Vision Lock (Non-Negotiable)

This rewrite is explicitly direct-push and agent-first.

Required outcomes:

1. Parallel integration path is direct push to the current base branch.
2. PR creation/merge is removed from parallel mode runtime behavior.
3. Coordinator remains non-semantic and minimal.
4. Agent-led remediation is primary for conflicts and CI repair.

Forbidden outcomes:

1. Reintroducing PR automation in parallel mode.
2. Reintroducing merge-agent orchestration semantics under a new name.
3. Adding compatibility shims that keep old PR path alive behind flags.
4. Moving queue/done finalization back into coordinator-only merge logic.

## 3. System Context

Ralph is a Rust CLI that executes queued tasks from `.ralph/queue.json` using AI agents through configurable multi-phase runs.

Core concepts:

1. Tasks are selected from queue state.
2. Work is performed via configured phases and iterations.
3. Task completion mutates queue/done state.
4. Git commit/push finalization happens after successful phase execution.

## 4. Current Architecture (As-Is)

Current parallel mode is PR-based and coordinator-heavy.

### 4.1 Current High-Level Flow

1. Coordinator spawns isolated worker processes.
2. Worker runs `run one --parallel-worker` in a workspace clone.
3. Worker pushes a task branch.
4. Worker creates PR.
5. Coordinator tracks PR state and pending merge jobs.
6. Coordinator invokes `run merge-agent` subprocess to merge/finalize.

### 4.2 Current Boundaries

1. Workers avoid canonical queue/done mutation in coordinator context.
2. Merge-agent finalizes canonical queue/done in coordinator repo.
3. Coordinator reconciles PR lifecycle on restart.

### 4.3 Current Pain Points

1. Multiple lifecycle layers (worker, PR, merge-agent, pending-merges) increase fragility.
2. Heavy persisted state reconciliation logic creates many failure paths.
3. GitHub PR state dependence introduces external coupling and recoverability complexity.
4. Conflict resolution context is split across sessions/processes.
5. Coordinator contains substantial deterministic merge behavior better handled by agents.

## 5. Goals

1. Remove PR creation/merge from parallel execution path.
2. Push all completed worker output directly to the current base branch.
3. Let agents resolve rebase/conflict/CI fix loops with explicit handoff context.
4. Reduce parallel state model to worker lifecycle only.
5. Preserve strong correctness gates at deterministic boundaries.

## 6. Non-Goals

1. No attempt to preserve old PR workflow in parallel mode.
2. No compatibility shim where both PR and direct-push run simultaneously.
3. No redesign of task data model or phase semantics beyond what direct-push requires.
4. No introduction of remote CI orchestration (local CI gates remain authoritative).
5. No behavioral changes to non-parallel run execution (`ralph run loop` without `--parallel`, and `ralph run one`) except strictly necessary shared refactors with zero behavior change.

## 6.1 Explicitly Forbidden Implementation Patterns

The implementing agent MUST NOT:

1. Add dual-path logic such as `if pr_mode { ... } else { ... }`.
2. Keep deprecated merge modules as active fallback paths.
3. Preserve obsolete PR/merge config keys as silent compatibility behavior.
4. Treat this rewrite as incremental migration that leaves old merge lifecycle running.
5. Add hidden safety behavior that functionally restores coordinator merge decisions.

## 7. Target Architecture (To-Be)

### 7.1 Core Model

Parallel mode becomes "direct integration workers":

1. Coordinator selects tasks and spawns workers.
2. Each worker executes configured phases.
3. After phase completion, each worker enters integration loop:
   1. fetch base branch,
   2. rebase,
   3. resolve conflicts through agent session(s),
   4. run CI/fix loop,
   5. push to base branch.
4. Coordinator records worker outcomes only.

### 7.2 Session Model (Critical)

Each phase still runs as configured in project/global config.

Important clarification:

1. "Worker owns lifecycle" means worker process owns lifecycle end-to-end.
2. It does **not** mean one continuous agent session.
3. Each phase agent remains its own session per configured phase runner/model.
4. Conflict-resolution and CI-remediation steps may spawn additional sessions, using an explicit handoff packet.

### 7.3 Coordinator Role

Coordinator responsibilities:

1. queue lock and task selection,
2. worker spawn/monitoring,
3. state persistence,
4. status/retry command support,
5. workspace cleanup policy.

Coordinator explicitly does **not**:

1. create PRs,
2. merge PRs,
3. reconcile GitHub PR lifecycle,
4. make semantic merge decisions.

## 8. Canonical Data Ownership

In the new model, canonical queue/done updates are committed and pushed by workers directly to base branch.

Therefore:

1. queue/done conflicts become normal git conflicts in worker rebase flow.
2. worker must resolve queue/done semantically, not textually.
3. deterministic validation must reject invalid queue/done post-resolution.

## 9. Worker Execution Contract

### 9.1 Worker Precondition

Worker starts from an isolated workspace clone rooted at the same commit/branch lineage as coordinator base branch.

### 9.2 Phase Execution

Worker executes configured phase count/iterations with existing phase override mechanics.

### 9.3 Post-Phase Integration Loop

After final configured phase success:

1. commit task changes,
2. enter bounded integration loop,
3. produce terminal outcome:
   1. `Completed`,
   2. `BlockedPush`,
   3. `Failed`.

## 10. Integration Loop (Normative)

### 10.1 Retry Parameters

Default values:

1. `max_attempts = 5`
2. `backoff_ms = [500, 2000, 5000, 10000]`

### 10.2 Loop Steps

For each attempt:

1. `git fetch origin <base_branch>`
2. compute divergence vs `origin/<base_branch>`
3. if behind:
   1. `git rebase origin/<base_branch>`
   2. if conflict:
      1. generate handoff packet,
      2. run agent conflict-remediation session(s),
      3. require zero unresolved conflicts,
      4. `git rebase --continue`
4. run CI gate (if enabled)
5. on CI failure:
   1. generate CI-failure handoff,
   2. run remediation agent session,
   3. repeat CI until pass or policy stop
6. `git push origin <base_branch>`
7. if non-fast-forward push failure:
   1. classify retryable,
   2. next attempt
8. if non-retryable failure:
   1. fail immediately as `BlockedPush` or `Failed` per classification

### 10.3 Terminal Conditions

1. Push succeeds: `Completed`.
2. Attempts exhausted: `BlockedPush` with workspace retained.
3. unrecoverable runtime/config error: `Failed`.

### 10.4 Mandatory Gate Before Worker Exit

A worker is not allowed to report success unless all are true:

1. no unresolved merge conflicts,
2. queue/done validate semantically,
3. CI gate passes when enabled,
4. push to `origin/<base_branch>` succeeds.

Prompt wording is not sufficient by itself. Runtime checks must enforce this.

## 11. Agent Remediation Contract

### 11.1 Required Handoff Packet

Before any remediation session, worker must write a structured handoff packet containing:

1. task id/title,
2. base branch,
3. phase outputs summary,
4. original task intent snapshot,
5. list of conflict files,
6. current git status,
7. queue/done semantic rules,
8. CI command + last failing output (for CI remediation).

Suggested location:

1. `.ralph/cache/parallel/handoffs/<task-id>/<attempt>.json`

### 11.2 Prompt Requirements

Prompt must explicitly instruct:

1. do not stop until all listed conflicts are resolved,
2. preserve upstream/base-branch changes,
3. preserve this task intent,
4. for queue/done:
   1. remove completed task from queue,
   2. ensure completed task appears in done,
   3. preserve other tasks from upstream.

### 11.3 Mandatory Compliance Checks (Deterministic)

Worker must enforce after remediation session:

1. `git diff --name-only --diff-filter=U` is empty,
2. queue/done parse and validate,
3. CI gate passes (if enabled).

If any check fails, worker must continue remediation loop or transition to terminal blocked/failed state per retry policy.

## 12. State Model Rewrite

### 12.1 State File

Path remains:

1. `.ralph/cache/parallel/state.json`

### 12.2 New Schema

State tracks worker lifecycle only.

```json
{
  "schema_version": 3,
  "started_at": "2026-02-20T00:00:00Z",
  "target_branch": "main",
  "workers": [
    {
      "task_id": "RQ-0001",
      "workspace_path": "/abs/path",
      "lifecycle": "running|integrating|completed|failed|blocked_push",
      "started_at": "...",
      "completed_at": null,
      "push_attempts": 0,
      "last_error": null
    }
  ]
}
```

### 12.3 Removed Concepts

1. PR records,
2. pending merge queue,
3. merge-agent lifecycle,
4. PR reconciliation state.

### 12.4 Invariants

1. one active worker per task id,
2. worker lifecycle monotonic toward terminal states,
3. blocked workspaces always retained unless retention expiry cleanup applies.

## 13. CLI Contract

### 13.1 Existing Entry Point

`ralph run loop --parallel N` remains parallel entry point.

### 13.2 Removed Command

`ralph run merge-agent` is removed.

### 13.3 New Operational Commands

1. `ralph run parallel status [--json]`
2. `ralph run parallel retry --task <TASK_ID>`

Behavior:

1. `status` shows active/completed/failed/blocked workers.
2. `retry` resumes integration loop for blocked worker from retained workspace.

## 14. Configuration Contract

### 14.1 Preserved

1. `parallel.workers`
2. `parallel.workspace_root`
3. `agent.phases`
4. `agent.iterations`
5. phase overrides
6. CI gate settings

### 14.2 Added

1. `parallel.max_push_attempts`
2. `parallel.push_backoff_ms`
3. `parallel.workspace_retention_hours`
4. `agent.runner_output_buffer_mb`

### 14.3 Removed

1. `parallel.auto_pr`
2. `parallel.auto_merge`
3. `parallel.merge_when`
4. `parallel.merge_method`
5. `parallel.merge_retries`
6. `parallel.draft_on_failure`
7. `parallel.conflict_policy`
8. `parallel.branch_prefix`
9. `parallel.delete_branch_on_merge`
10. `parallel.merge_runner`

## 15. Migration Strategy

### 15.1 State Migration

1. migrate schema v2 -> v3 by dropping PR/merge fields,
2. map in-flight entries to worker entries,
3. set unresolved in-flight workers to `failed` or `blocked_push` via workspace inspection.

### 15.2 Config Migration

1. remove obsolete PR/merge keys,
2. apply defaults for new push/retention settings,
3. fail fast on invalid values.

## 16. Deterministic Safety Rails (Required)

This rewrite is agent-first, but the following rails are non-negotiable:

1. unresolved conflict check before push,
2. queue/done schema + semantic validation,
3. CI gate enforcement (if enabled),
4. bounded retry and explicit terminal state,
5. crash-safe state persistence.

These are not "business logic theology"; they are integrity gates.

## 16.1 Minimal Deterministic Boundary (Design Rule)

Deterministic code is allowed only for:

1. orchestration lifecycle bookkeeping,
2. hard validation and guard conditions,
3. bounded retries and failure classification.

Deterministic code is not allowed to make semantic integration choices that belong to agents.

## 17. Failure and Recovery

### 17.1 Retryable

1. non-fast-forward push rejection,
2. transient fetch/push network failures,
3. conflict requiring additional remediation pass.

### 17.2 Non-Retryable

1. invalid configuration,
2. irreparable queue/done validation failures after retry budget,
3. persistent CI failure after retry policy exhaustion.

### 17.3 Blocked Workspace Handling

1. blocked workspace retained for inspection/retry,
2. retention cleanup runs on:
   1. parallel start,
   2. parallel end,
   3. `parallel status`.

## 18. Security and Operational Considerations

1. Never log secrets from CI output or environment.
2. Sanitize control/NUL bytes in logs before persistence.
3. Direct push requires write access to target base branch.
4. Protected branch policies may force blocked outcomes; this is expected and surfaced clearly.

## 19. Test Plan

### 19.1 Unit Tests

1. push loop success and retry paths,
2. conflict-remediation compliance checks,
3. queue/done conflict semantic merge validation,
4. state migration v2->v3,
5. status/retry command behavior,
6. log sanitization,
7. webhook outcome typing,
8. runner buffer config loading.

### 19.2 Integration Tests

1. two workers no conflict,
2. code conflict resolved then push,
3. queue/done conflict resolved correctly,
4. three-worker push race,
5. CI fail -> agent fix -> CI pass -> push,
6. blocked worker retry success,
7. interrupted run recovery from persisted state.

### 19.3 Verification Commands

1. `make agent-ci`
2. `make ci`

## 20. Implementation Plan

### Phase A: Reliability fixes already identified

1. fix bookkeeping cleanup ordering,
2. replace line-based bookkeeping parsing with porcelain `-z` parser,
3. add debug log sanitization,
4. replace webhook boolean outcome with typed enum,
5. add runner output buffer config.

### Phase B: Direct-push core

1. rewrite worker post-run supervision to stop restoring queue/done from HEAD,
2. implement worker integration loop,
3. implement remediation handoff packet + prompts,
4. implement deterministic compliance checks,
5. integrate retry policy and blocked-state transitions.

### Phase C: Coordinator simplification

1. remove PR creation from parallel orchestration,
2. remove merge-agent subprocess invocations,
3. simplify state model and initialization,
4. remove stale PR reconciliation logic.

### Phase D: Deletions and CLI updates

1. delete `run merge-agent` command,
2. delete deprecated merge-runner module,
3. remove PR-related config/schema/docs,
4. add `parallel status` and `parallel retry` commands.

### Phase E: Docs and release

1. update `docs/cli.md`, `docs/configuration.md`, `docs/features/parallel.md`,
2. add migration notes,
3. run full local CI gates.

## 20.1 Agent Execution Checklist (Must Be Satisfied)

Before implementation is considered complete, the implementing agent must verify:

1. no active parallel PR creation flow remains,
2. no active parallel merge-agent invocation flow remains,
3. obsolete PR/merge config keys are removed from contracts/schema/docs,
4. state schema no longer tracks PR lifecycle and pending merge queues,
5. integration tests cover direct-push conflict and queue/done merge scenarios,
6. `make agent-ci` and `make ci` pass.

If any checklist item fails, the rewrite is incomplete.

## 21. Acceptance Criteria

This rewrite is accepted when all are true:

1. Parallel mode performs no PR creation/merge operations.
2. Workers push directly to base branch with successful multi-worker runs.
3. Queue/done remain valid and semantically correct under conflict scenarios.
4. Coordinator restart and blocked-workspace retry are reliable.
5. Deprecated PR/merge state and commands are removed.
6. Local CI gates pass.
7. Non-parallel loop and run-one behavior remain unchanged.

## 21.1 Anti-Drift Acceptance Gate

This rewrite is rejected if any remain true:

1. parallel worker success still triggers PR creation,
2. coordinator still performs PR merge lifecycle management,
3. merge-agent command remains required for normal parallel operation,
4. persisted parallel state still models PR lifecycle as core runtime behavior.

## 22. Explicit Design Decisions

1. Direct push to base branch is the canonical parallel integration strategy.
2. Worker lifecycle spans phase sessions plus post-phase remediation loop.
3. Phase sessions remain separate, per configured phase settings.
4. Coordinator is intentionally minimized and non-semantic.
5. Agent autonomy is primary, bounded by deterministic integrity checks.

## 22.1 Change Control and Deviation Protocol

Any implementation deviation from sections 2, 2.1, 6.1, 10, 16, 20.1, and 21.1 requires explicit maintainer approval before merge.

"Reasonable interpretation" is not sufficient for deviations. Approval must be explicit and documented.

## 23. Appendix: Current Module Touchpoints (Implementation Map)

Primary files impacted:

1. `crates/ralph/src/commands/run/parallel/orchestration.rs`
2. `crates/ralph/src/commands/run/parallel/state.rs`
3. `crates/ralph/src/commands/run/parallel/state_init.rs`
4. `crates/ralph/src/commands/run/parallel/worker.rs`
5. `crates/ralph/src/commands/run/supervision/parallel_worker.rs`
6. `crates/ralph/src/commands/run/merge_agent.rs` (delete)
7. `crates/ralph/src/commands/run/parallel/merge_runner/*` (delete)
8. `crates/ralph/src/contracts/config/parallel.rs`
9. `crates/ralph/src/cli/run.rs`
10. `docs/cli.md`
11. `docs/configuration.md`
12. `docs/features/parallel.md`

This map is informative; the normative behavior is defined by sections 1-22.
