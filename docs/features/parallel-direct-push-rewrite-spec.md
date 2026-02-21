# Parallel Mode Direct-Push Rewrite Specification (Agent-Owned Git)

Status: Draft  
Last Updated: February 20, 2026  
Scope: `ralph run loop --parallel` only

---

## 1. Purpose

This document defines the target parallel architecture for Ralph with direct pushes to the current base branch.

This version replaces the previous ambiguous draft and is the authoritative source for implementation.

If any earlier note/doc conflicts with this file, this file wins.

---

## 2. Clarified Design Decision (Authoritative)

The agreed design is:

1. Parallel workers push directly to the base branch (`origin/<current-branch>`).
2. The **agent session(s)** perform git integration operations:
   1. `git fetch`
   2. `git rebase`
   3. conflict resolution edits
   4. `git add/commit` as needed
   5. `git push`
3. Ralph coordinator does orchestration and hard validation, but does not semantically integrate code.
4. Phase sessions remain separate per configured phase runner/model (no requirement for one continuous session).
5. Non-parallel behavior remains unchanged.

---

## 3. Scope and Non-Goals

### 3.1 In Scope

1. Rewrite parallel mode from PR-based merge orchestration to direct push.
2. Remove PR lifecycle state and merge-agent runtime dependency from parallel path.
3. Move integration actions to agent-driven sessions with strict runtime gates.
4. Keep coordinator minimal: schedule, monitor, persist state, retry blocked workspaces.

### 3.2 Non-Goals

1. No behavioral change to:
   1. `ralph run loop` without `--parallel`
   2. `ralph run one`
2. No PR fallback mode in parallel.
3. No compatibility shim keeping old PR merge flow alive.
4. No remote CI redesign.

---

## 4. Why Rewrite

Current PR/merge-agent parallel flow has too much deterministic coordinator logic and too many moving lifecycle parts.

Key pain points:

1. Worker -> PR -> merge-agent -> state reconciliation is complex and fragile.
2. Conflict handling can lose task intent context.
3. PR lifecycle state bookkeeping creates restart and reconciliation overhead.
4. Coordinator is doing too much semantic workflow management.

---

## 5. Target Architecture

### 5.1 Process Model

1. Coordinator acquires queue lock, selects tasks, creates worker workspaces, spawns workers.
2. Each worker executes configured phases in order.
3. After final phase, worker runs agent-driven integration loop until:
   1. success push, or
   2. retry budget exhausted (blocked), or
   3. terminal failure.
4. Coordinator records worker outcomes and handles cleanup/retention.

### 5.2 Ownership Matrix (Critical)

| Concern | Owner |
|---|---|
| Task scheduling | Ralph coordinator |
| Worker lifecycle | Ralph coordinator |
| Phase execution | Agent sessions (configured per phase) |
| `fetch/rebase/conflict resolution/commit/push` | Agent integration session(s) |
| Conflict semantic decisions | Agent integration session(s) |
| Hard correctness checks | Ralph runtime |
| State persistence | Ralph coordinator |

### 5.3 Explicit Constraint

In parallel mode, Ralph must not reintroduce semantic merge orchestration logic equivalent to old PR merge-agent behavior.

---

## 6. Worker Lifecycle

### 6.1 Phase Sessions (Unchanged Model)

Workers still honor configured phases/iterations and phase overrides.

Important:

1. Phase sessions are separate, as configured.
2. This spec does not require cross-phase session continuity.
3. Continuity is achieved through artifacts (task context, phase outputs, handoff packets), not a single persistent chat session.

### 6.2 Post-Phase Integration Loop (Agent-Owned Git)

After final phase succeeds, worker enters integration loop.

Loop intent:

1. Reconcile workspace with latest base branch.
2. Resolve conflicts via agent.
3. Ensure CI passes after integration.
4. Push to base branch.

Pseudo-flow:

1. Start attempt `N`.
2. Ask integration agent to run:
   1. `git fetch origin <base>`
   2. rebase onto `origin/<base>`
3. If conflicts:
   1. provide conflict handoff packet
   2. require agent to resolve and continue rebase
4. Run CI gate (if enabled).
5. If CI fails:
   1. provide CI failure handoff packet
   2. require agent to fix and rerun CI
6. Ask integration agent to push to `origin/<base>`.
7. If non-fast-forward or transient failure:
   1. retry with backoff
8. On success: worker completes.
9. On max retries: worker becomes `blocked_push` and workspace is retained.

---

## 7. Agent Prompt Contracts

### 7.1 Integration Session Prompt Requirements

Integration prompt must explicitly require the agent to perform git operations itself.

Required instructions:

1. You own fetch/rebase/conflict resolution/commit/push for this task.
2. Do not stop until either push succeeds or a hard stop condition is reached.
3. Preserve upstream functionality and task intent.

### 7.2 Conflict Resolution Prompt Requirements

When conflicts exist, prompt must include:

1. conflicted file list,
2. current task id/title/intent summary,
3. queue/done semantics,
4. explicit required commands to finish rebase.

### 7.3 CI Remediation Prompt Requirements

When CI fails, prompt must include:

1. exact command,
2. stdout/stderr,
3. explicit instruction to fix then rerun.

---

## 8. Deterministic Runtime Gates (Non-Negotiable)

Agent autonomy does not remove hard integrity checks.

Before worker can report success, Ralph must verify:

1. no unresolved conflicts (`diff-filter=U` empty),
2. queue/done parse and semantic validation pass,
3. CI passes (when enabled),
4. push to `origin/<base>` succeeded.

Prompt wording alone is not sufficient; runtime enforcement is required.

---

## 9. Coordinator Responsibilities (Minimal)

Coordinator responsibilities:

1. enforce parallel preflight checks,
2. select non-duplicate tasks,
3. spawn and monitor workers,
4. persist simplified worker state,
5. expose status and retry commands,
6. cleanup completed/failed workspaces,
7. retain blocked workspaces for retry.

Coordinator non-responsibilities:

1. PR creation,
2. PR merge management,
3. GitHub PR lifecycle reconciliation,
4. semantic conflict decision making.

---

## 10. State Model

Path remains:

1. `.ralph/cache/parallel/state.json`

Target schema:

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
      "started_at": "2026-02-20T00:00:00Z",
      "completed_at": null,
      "push_attempts": 0,
      "last_error": null
    }
  ]
}
```

Removed from runtime state:

1. PR records,
2. pending merge queue,
3. merge lifecycle state.

---

## 11. CLI Contract

### 11.1 Existing Entry

`ralph run loop --parallel N` remains the entrypoint.

### 11.2 Removed Parallel Runtime Dependency

`ralph run merge-agent` is not part of normal parallel runtime in this design.

### 11.3 Operational Commands

1. `ralph run parallel status [--json]`
2. `ralph run parallel retry --task <TASK_ID>`

---

## 12. Configuration Contract

### 12.1 Preserved

1. `parallel.workers`
2. `parallel.workspace_root`
3. `agent.phases`
4. `agent.iterations`
5. phase overrides
6. CI gate settings

### 12.2 Added

1. `parallel.max_push_attempts`
2. `parallel.push_backoff_ms`
3. `parallel.workspace_retention_hours`
4. `agent.runner_output_buffer_mb`

### 12.3 Removed

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

---

## 13. Migration

### 13.1 State Migration

1. Migrate old parallel state to worker-only state.
2. Drop PR/pending-merge fields.
3. Convert unresolved in-flight records to `failed` or `blocked_push` based on workspace inspection.

### 13.2 Config Migration

1. Remove obsolete PR/merge keys.
2. Apply defaults for new push/retry/retention keys.
3. Fail fast on invalid values.

---

## 14. Anti-Drift Rules

Implementation is rejected if any of the following remain true:

1. parallel worker success still triggers PR creation,
2. coordinator still performs PR merge lifecycle management,
3. merge-agent is required for normal parallel operation,
4. persisted parallel state still models PR lifecycle as core behavior.

Any deviation from this spec requires explicit maintainer approval.

---

## 15. Verification and Acceptance

### 15.1 Must-Pass Validation

1. Unit tests for push-loop, conflict remediation, CI remediation, and state migration.
2. Integration tests for multi-worker races and queue/done conflict merges.
3. `make agent-ci` passes.
4. `make ci` passes.

### 15.2 Acceptance Criteria

Accepted only when all are true:

1. Parallel mode uses direct push to base branch.
2. Agent sessions perform integration git operations.
3. Ralph enforces hard integrity gates before success.
4. Coordinator is minimal and non-semantic.
5. Non-parallel loop/run-one behavior is unchanged.

---

## 16. Implementation Map

Primary touched modules:

1. `crates/ralph/src/commands/run/parallel/orchestration.rs`
2. `crates/ralph/src/commands/run/parallel/state.rs`
3. `crates/ralph/src/commands/run/parallel/state_init.rs`
4. `crates/ralph/src/commands/run/parallel/worker.rs`
5. `crates/ralph/src/commands/run/supervision/parallel_worker.rs`
6. `crates/ralph/src/contracts/config/parallel.rs`
7. `crates/ralph/src/cli/run.rs`
8. `docs/cli.md`
9. `docs/configuration.md`
10. `docs/features/parallel.md`

Legacy removals:

1. `crates/ralph/src/commands/run/merge_agent.rs`
2. `crates/ralph/src/commands/run/parallel/merge_runner/*`

