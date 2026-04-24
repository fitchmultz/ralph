# Ralph Task Decompose
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](../index.md)


## Introduction

Ralph currently supports several task creation and task-structuring workflows, but none of them make recursive decomposition a first-class experience.

- `ralph task build` creates one strong task from a freeform request.
- `ralph task split` manually breaks one existing task into a fixed number of child tasks.
- `ralph prd create` converts a document into one or more tasks.
- `ralph scan` discovers opportunities and adds them to the queue.

This leaves a product gap for users who start from a high-level engineering goal and want Ralph to propose a structured tree of executable subtasks before any work runs.

The proposed `ralph task decompose` command fills that gap. It brings a dedicated decomposition workflow into Ralph while preserving Ralph’s core architecture:

- Queue-backed, durable state in `.ralph/queue.jsonc`
- Existing hierarchy support via `parent_id`
- Preview-first workflow before mutating queue state
- Separation between planning/decomposition and execution

The feature should feel native to Ralph rather than bolted on. Users should be able to decompose a new goal or decompose an existing task using the same safety, validation, and undo expectations as the rest of the task surface.

## Goals

- Introduce `ralph task decompose` as the dedicated workflow for recursive task decomposition.
- Let users decompose either a freeform request or an existing queue task.
- Generate durable Ralph tasks, not ephemeral planner-only output.
- Reuse existing task hierarchy, queue validation, ID generation, and undo mechanisms.
- Keep the workflow preview-first so users can inspect the proposed tree before writing.
- Preserve clean separation between decomposition and execution.

## Non-Goals

- Automatically executing decomposed tasks as part of the same command.
- Replacing `ralph task build`, `ralph task split`, `ralph prd create`, or `ralph scan`.
- Introducing a new persistent queue field for “atomic” vs “composite” task kind.
- Automatically inferring dependencies outside the generated sibling group.
- Merging or rewriting unrelated existing hierarchy automatically.
- Building a GUI-only experience before the CLI workflow is stable.

## User Stories

### US-001: Decompose a New Goal into a Task Tree

As a user starting from a high-level engineering goal,
I want to run `ralph task decompose "..."` and preview a structured task tree,
so that I can turn an abstract goal into reviewable, executable queue entries.

#### Acceptance Criteria

- Running `ralph task decompose "Build OAuth login with GitHub and Google"` produces a preview by default.
- The preview includes a hierarchy tree, node counts, and warnings when limits or heuristics affect the result.
- The preview does not modify `.ralph/queue.jsonc` unless `--write` is explicitly provided.
- When `--write` is provided, Ralph creates a root task and descendant tasks with unique IDs and valid timestamps.
- Created tasks use `parent_id` to represent hierarchy.
- Generated tasks can be viewed with existing commands such as `ralph queue tree` and `ralph task children`.

### US-002: Decompose an Existing Task In Place

As a user with an existing broad task in the queue,
I want to decompose that task into child tasks,
so that I can preserve the original task context while making the work more actionable.

#### Acceptance Criteria

- Running `ralph task decompose RQ-0123` previews child tasks under the existing task.
- By default, the existing task is preserved as the parent rather than rejected or archived.
- Generated child tasks use `parent_id = RQ-0123`.
- The command refuses to mutate a non-existent task ID.
- The command refuses to decompose tasks from the done archive unless an explicit opt-in is provided in a future or explicit override mode.
- The command records a clear human-readable note on the source task indicating that it was decomposed.

### US-004: Control Granularity and Complexity

As a user with different planning needs,
I want controls for decomposition depth and fanout,
so that Ralph does not over-decompose or generate unmanageable trees.

#### Acceptance Criteria

- The command supports `--max-depth` to limit recursive depth.
- The command supports `--max-children` to cap per-node fanout.
- The command supports a total-node safety cap, whether user-configurable or defaulted.
- When a limit is reached, the preview and write output explain which limit applied.
- When the model cannot confidently split a node further, Ralph treats it as a leaf and reports that behavior without failing the entire operation.

### US-006: Attach and Extend an Existing Epic

As a user with an existing epic or parent task,
I want to attach a newly decomposed subtree under that task,
so that I can expand an established plan without replacing the parent itself.

#### Acceptance Criteria

- Running `ralph task decompose --attach-to RQ-0042 "Plan webhook reliability work"` previews a new subtree under `RQ-0042`.
- When `--write` is provided, Ralph creates a new root child under the attach target and nests descendants beneath that new root.
- When the attach target already has children, `--child-policy fail|append|replace` governs write behavior deterministically.
- `--child-policy replace` refuses the write when tasks outside the subtree still reference descendant IDs that would be removed.

### US-007: Infer Sibling Dependencies and Emit JSON

As a user automating planning flows,
I want optional sibling dependency inference and stable JSON output,
so that I can review or consume decompositions programmatically.

#### Acceptance Criteria

- Running `ralph task decompose ... --with-dependencies` resolves sibling-only `depends_on` edges from planner keys or sibling titles.
- Self-dependencies, unknown dependencies, and non-sibling references are dropped with warnings.
- Running `ralph task decompose ... --format json` emits a stable versioned JSON payload for preview or write mode.

### US-005: Use the Workflow Reliably in Non-Interactive Environments

As a user running Ralph in non-interactive contexts,
I want decomposition to behave safely and predictably,
so that it does not mutate queue state unless I explicitly request it.

#### Acceptance Criteria

- The command supports non-interactive operation without TTY prompts.
- Preview remains the default in non-interactive environments.
- Queue mutation still requires `--write` explicitly.
- Validation failures produce deterministic, human-readable output.

## Functional Requirements

1. Ralph SHALL add a new `ralph task decompose` subcommand under the existing `task` command group.
2. Ralph SHALL accept either a freeform request or an existing task ID as the decomposition source.
3. Ralph SHALL support a preview-first workflow and SHALL NOT mutate queue state unless `--write` is provided.
4. Ralph SHALL generate durable queue tasks rather than ephemeral planner-only output.
5. Ralph SHALL represent task hierarchy using the existing `parent_id` field.
6. Ralph SHALL reuse existing queue ID allocation so generated task IDs remain unique across queue and done archives.
7. Ralph SHALL create valid `created_at` and `updated_at` timestamps for all newly written tasks.
8. Ralph SHALL preserve the decomposed source task by default when decomposing an existing active task.
9. Ralph SHALL support configurable recursion depth limits.
10. Ralph SHALL support configurable per-node fanout limits.
11. Ralph SHALL enforce a total generated node safety limit before writing queue state.
12. Ralph SHALL treat hierarchy and execution ordering as separate concepts.
13. Ralph SHALL reuse queue locking and undo snapshot behavior for write operations.
14. Ralph SHALL validate queue state before and after decomposition writes.
15. Ralph SHALL include deterministic human-readable preview output that can be inspected before write.
16. Ralph SHALL integrate with existing hierarchy navigation commands such as `ralph queue tree`, `ralph task children`, and `ralph task parent`.
17. Ralph SHALL support runner, model, reasoning-effort, RepoPrompt, and runner CLI override flags consistent with other runner-backed task creation flows.
18. Ralph SHALL use a dedicated decomposition prompt/template rather than overloading task-builder or scan prompts.
19. Ralph SHALL fail safely when planner output is malformed, incomplete, or inconsistent with queue rules.
20. Ralph SHALL support `--attach-to <TASK_ID>` for freeform request decomposition under an existing active parent task.
21. Ralph SHALL support `--child-policy fail|append|replace` for effective parents with existing child trees.
22. Ralph SHALL support optional sibling dependency inference behind `--with-dependencies`.
23. Ralph SHALL emit stable versioned JSON output when `--format json` is requested.

## User Experience

### Primary CLI Examples

```bash
ralph task decompose "Build OAuth login with GitHub and Google"
ralph task decompose "Improve webhook reliability" --write
ralph task decompose RQ-0123 --max-depth 3 --preview
ralph task decompose RQ-0123 --child-policy append --with-dependencies --write
ralph task decompose --attach-to RQ-0042 --format json "Plan webhook reliability work"
```

### Preview Output Expectations

Preview output should communicate:

- what is being decomposed
- whether the source is a new request or an existing task
- proposed hierarchy
- total node and leaf counts
- warnings about caps, degenerate splits, or dropped invalid output

### Write Output Expectations

Write output should communicate:

- root task affected or created
- number of tasks created
- list of created task IDs
- whether the parent task was preserved or annotated

## Data Model and State

The v1 implementation should prefer existing Ralph task schema over new persistent contract changes.

Recommended persistent representation:

- `parent_id` for tree structure
- `plan` for task-local implementation guidance
- `request` for original top-level user intent
- `tags` and `scope` for seeded inheritance

The feature should not require a new `atomic/composite` queue field in v1.

## Planner and Prompt Requirements

The decomposition system should use a dedicated prompt that asks for structured recursive output.

The planner output should be able to represent:

- task title
- optional description
- optional plan items
- optional tags
- optional scope hints
- optional child nodes

Planner guidance should emphasize:

- minimizing overlap between sibling tasks
- preferring directly actionable leaves
- avoiding low-value placeholder tasks unless clearly justified
- stopping decomposition when a task is runnable without additional planning

## Validation and Safety Requirements

- Preview mode must not acquire a queue mutation lock unless implementation details make read-side locking necessary.
- Write mode must acquire the queue lock before mutation.
- Write mode must create an undo snapshot before saving.
- Queue validation failures before planning must abort the operation.
- Queue validation failures after materialization must abort without partial writes.
- Malformed planner output must fail safely with actionable diagnostics.
- Existing task IDs must never be reused.
- Generated parent-child relationships must not create parent cycles.

## Edge Cases and Failure Modes

### Existing Parent Already Has Children

- Default behavior when decomposing an existing task with children should be to refuse write.
- Preview should still work and clearly explain the conflict.
- `--child-policy append` should preserve the existing subtree and insert the new subtree immediately after it.
- `--child-policy replace` should remove the existing descendant subtree only when no outside task still references it.

### Done or Rejected Source Task

- Ralph should refuse to decompose done or rejected tasks by default.
- If future support is added, it should require explicit opt-in and remain preview-first.

### Degenerate Planner Output

- Empty child arrays for a node expected to split should result in a warning and a leaf fallback.
- Repeated one-child recursion should be collapsed to prevent unhelpful chains.
- Excessive node counts should be capped and reported.

### Queue Ordering

- New root decompositions should respect existing “doing task first” insertion behavior.
- Child tasks for an existing task should be inserted deterministically near the source task.

### Non-Interactive Environments

- The command should not prompt when stdin is not a TTY.
- Non-interactive runs must require explicit source and flags rather than relying on interactive disambiguation.

## Product Decisions

### Preview Default

Preview SHALL be the hard default in all environments.

- `ralph task decompose ...` performs a preview only.
- Queue mutation requires explicit `--write`.
- There is no TTY-only “safety behavior” split for preview vs write.
- This keeps the command predictable, scriptable, and safe.

Rationale:

- Decomposition is a high-blast-radius planning command that can create many tasks at once.
- Hidden environment-dependent behavior is a bad fit for automation and a bad fit for user trust.
- Users should never have to remember whether they were in a terminal, CI shell, or app bridge to know whether queue state changed.

### Existing Parent with Existing Children

Default behavior when decomposing an existing task that already has children SHALL be to refuse write unless the caller explicitly chooses `--child-policy append` or `--child-policy replace`.

Preview still succeeds and shows the conflict or selected policy.

Rationale:

- `fail` remains the safest default.
- `append` gives users an explicit non-destructive extension path.
- `replace` is acceptable only with strict reference checks and undo coverage.

### Dependency Inference Scope

Dependency inference SHALL be optional and limited to siblings within the same generated parent group.

Rationale:

- Sibling-only inference captures the most useful ordering constraints without exposing the planner to arbitrary queue-wide references.
- Restricting inference scope keeps validation and debugging tractable.

### Parent Annotation Strategy

Decomposed parent tasks SHALL receive a human-readable note in v1.

Rationale:

- Notes help humans scanning queue history.
- Custom fields can be added later when there is a concrete consumer for them.

## Success Metrics

- Users can create a decomposed task tree from a high-level request without manually chaining `task build` and `task split`.
- Generated trees are valid under existing queue validation rules.
- Preview-to-write flow is understandable and safe.
- Users can immediately inspect results with existing `queue tree` and `task children` commands.
- The feature reduces manual queue shaping for multi-step goals.

## Implementation Status

The CLI implementation now includes:

- preview-first decomposition for freeform requests and existing tasks
- `--attach-to` subtree attachment for freeform requests
- `--child-policy fail|append|replace`
- optional sibling dependency inference via `--with-dependencies`
- stable versioned JSON output via `--format json`

Future work can focus on macOS app integration and richer visual review/edit flows rather than core command semantics.
