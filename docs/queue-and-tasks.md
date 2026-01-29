# Queue and Tasks

Purpose: Define the queue file format, task fields, and status lifecycle based on `schemas/queue.schema.json`.

## Queue File
The queue file (`.ralph/queue.json`) is the source of truth for active work. Completed tasks are moved to `.ralph/done.json`, which must contain only `done` or `rejected` tasks.

Minimum queue structure:
```json
{
  "version": 1,
  "tasks": []
}
```

## Task Fields
Each task is an object with required and optional fields.

Required:
- `id` (string)
- `title` (string)
- `created_at` (string, RFC3339 UTC)
- `updated_at` (string, RFC3339 UTC)

Common optional fields:
- `tags` (list of strings, defaults to empty).
- `scope` (list of strings, defaults to empty).
- `evidence` (list of strings, defaults to empty).
- `plan` (list of strings, defaults to empty).
- `notes` (list of strings, defaults to empty).
- `status`: `draft`, `todo`, `doing`, `done`, `rejected` (default: `todo`).
- `priority`: `critical`, `high`, `medium`, `low` (default: `medium`).
- `request`: original human request (string or null).
- `completed_at`: RFC3339 UTC timestamp (required if status is `done` or `rejected`, otherwise optional).
- `agent`: per-task runner override (see below).
- `depends_on` (list of task IDs, defaults to empty).
- `custom_fields` (map of strings, defaults to empty).

Per-task agent overrides:
- `agent.runner`: `codex`, `opencode`, `gemini`, `claude`, or `cursor`.
- `agent.model`: model id string.
- `agent.model_effort`: `default`, `low`, `medium`, `high`, `xhigh` (Codex only).
- `agent.iterations`: number of iterations for this task (default: 1).
- `agent.followup_reasoning_effort`: reasoning effort for iterations after the first (Codex only).

Notes:
- `agent.model_effort: default` (or omitting the field) uses the configured `agent.reasoning_effort`.
- `agent.followup_reasoning_effort` is ignored for non-Codex runners.
- Breaking change: `agent.reasoning_effort` in task entries is replaced by `agent.model_effort`.

## Example Task
```json
{
  "id": "RQ-0007",
  "title": "Add CI validation for queue format",
  "status": "doing",
  "priority": "high",
  "created_at": "2026-01-25T03:45:00Z",
  "updated_at": "2026-01-25T03:45:00Z",
  "tags": ["cli", "queue"],
  "scope": ["schemas/queue.schema.json", "crates/ralph/src/cli/queue.rs"],
  "plan": ["Add schema validation to queue validate."],
  "evidence": ["make ci"],
  "depends_on": [],
  "custom_fields": {
    "owner": "platform"
  },
  "agent": {
    "runner": "codex",
    "model": "gpt-5.2-codex",
    "model_effort": "high",
    "iterations": 2,
    "followup_reasoning_effort": "low"
  }
}
```

## Lifecycle Notes
- Tasks run in the file order from `.ralph/queue.json`.
- Completed tasks are removed from `.ralph/queue.json` and appended to `.ralph/done.json`.
- Dependencies: A task is blocked until all IDs in its `depends_on` list have status `done` or `rejected`.
- Draft tasks (`status: draft`) are skipped by `run one` and `run loop` unless `--include-draft` is set.

## Dependency Validation

Ralph validates task dependencies on queue operations to ensure correctness and prevent issues:

### Hard Errors (blocking)
These validation failures prevent queue operations and must be fixed:

- **Self-dependency**: A task cannot depend on itself.
- **Missing dependency**: Referenced task ID must exist in `queue.json` or `done.json`.
- **Circular dependency**: Dependency graph must be acyclic (DAG).
- **Invalid terminal status**: `done.json` must only contain tasks with `done` or `rejected` status.

### Warnings (non-blocking)
These issues are reported but do not prevent queue operations:

- **Dependency on rejected task**: Task depends on a rejected task that will never complete. The dependency will never be satisfied.
- **Deep dependency chain**: Dependency chain exceeds `queue.max_dependency_depth` (default: 10). This may indicate overly complex dependencies.
- **Blocked execution path**: All dependency paths from this task lead to incomplete or rejected tasks. The task cannot make progress until blocking dependencies are resolved.

### Configuration

Set `queue.max_dependency_depth` in `.ralph/config.json` to adjust the depth warning threshold:

```json
{
  "queue": {
    "max_dependency_depth": 15
  }
}
```

Validation warnings are logged during queue operations. Review them with `ralph queue validate` or by checking the queue after operations.

## Dependency Visualization

Ralph provides multiple ways to visualize task dependencies:

### CLI Graph Command

The `ralph queue graph` command displays dependency relationships:

```bash
# ASCII tree view of dependencies
ralph queue graph --task RQ-0001

# Graphviz DOT format for external rendering
ralph queue graph --format dot > deps.dot
dot -Tpng deps.dot -o deps.png

# Show what tasks are blocked by a specific task
ralph queue graph --task RQ-0001 --reverse

# Highlight critical path (longest dependency chain)
ralph queue graph --critical
```

### TUI Dependency Overlay

In the TUI, press `v` to open the dependency graph overlay for the selected task:

- Shows upstream dependencies (what this task depends on) by default
- Press `t` or `Tab` to toggle to downstream view (what this task blocks)
- Press `c` to toggle critical path highlighting
- Press `d`, `v`, `Esc`, or `q` to close the overlay

### Critical Path

The critical path is the longest dependency chain in the graph. Tasks on the critical path are highlighted with `*` in tree/list output and in red in the TUI overlay. Completing critical path tasks unblocks the most downstream work.
- `ralph task` inserts new tasks near the top of the queue:
  - Default: insert at position 0 (top).
  - If the first task is already `doing`, insert at position 1 (immediately below the in-progress task).
