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
- `scope` (list of strings, defaults to empty): Scope is a starting point, not a restriction. Use it for relevant paths/commands and expand as needed.
- `evidence` (list of strings, defaults to empty).
- `plan` (list of strings, defaults to empty).
- `notes` (list of strings, defaults to empty).
- `status`: `draft`, `todo`, `doing`, `done`, `rejected` (default: `todo`).
- `priority`: `critical`, `high`, `medium`, `low` (default: `medium`).
- `request`: original human request (string or null).
- `completed_at`: RFC3339 UTC timestamp (required if status is `done` or `rejected`, otherwise optional).
- `agent`: per-task runner override (see below).
- `depends_on` (list of task IDs, defaults to empty).
- `blocks` (list of task IDs, defaults to empty): Tasks that this task blocks. Semantically different from `depends_on`: blocks is "I prevent X" vs depends_on "I need X".
- `relates_to` (list of task IDs, defaults to empty): Tasks that this task relates to (loose coupling, no execution constraint).
- `duplicates` (string or null): Task ID that this task duplicates.
- `custom_fields` (map of strings, defaults to empty).
  - **Note**: The queue loader accepts string/number/boolean values and coerces them to strings (in memory, and on subsequent saves). When manually editing `.ralph/queue.json`, values should still be quoted strings for consistency.
  - **Reserved analytics keys**: Ralph automatically writes the following keys to completed tasks:
    - `runner_used`: The runner actually used for execution (e.g., `codex`, `claude`, `opencode`).
    - `model_used`: The model actually used for execution (e.g., `gpt-5.3-codex`, `sonnet`).
    - These fields are observational (what actually ran) and should not be confused with `agent.runner`/`agent.model` which express intent/override.

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
    "model": "gpt-5.3-codex",
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

### Relationship Validation

Ralph also validates task relationships (`blocks`, `relates_to`, `duplicates`) to ensure correctness:

**Hard Errors (blocking):**
- **Self-reference**: A task cannot block, relate to, or duplicate itself.
- **Missing target**: Referenced task ID must exist in `queue.json` or `done.json`.
- **Circular blocking**: Blocking relationships must form a DAG (no cycles).

**Warnings (non-blocking):**
- **Duplicate of done/rejected task**: Task duplicates a completed or rejected task.

Relationships are distinct from dependencies:
- `depends_on`: "I need X" (execution constraint - task waits for dependencies)
- `blocks`: "I prevent X" (execution constraint - blocked tasks wait for this task)
- `relates_to`: Loose coupling (no execution constraint, just semantic association)
- `duplicates`: Marks redundancy (no execution constraint, informational only)

### Warnings (non-blocking)
These issues are reported but do not prevent queue operations:

- **Dependency on rejected task**: Task depends on a rejected task that will never complete. The dependency will never be satisfied.
- **Deep dependency chain**: Dependency chain exceeds `queue.max_dependency_depth` (default: 10). This may indicate overly complex dependencies.
- **Blocked execution path**: All dependency paths from this task lead to incomplete or rejected tasks. The task cannot make progress until blocking dependencies are resolved.
- **Duplicate of done/rejected task**: Task marked as duplicate of a completed or rejected task. Consider if the duplicate is still needed.

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

## Task ID Validation

Ralph enforces unique task IDs across both `.ralph/queue.json` and `.ralph/done.json`. Duplicate IDs will cause validation errors.

### Duplicate Task ID Errors

**Error:** `Duplicate task ID detected across queue and done: RQ-XXXX`

This error occurs when the same task ID exists in both the active queue and the done archive. This typically happens when:

1. A new task was added to the queue without incrementing the ID properly
2. A task was manually copied/edited and the ID wasn't updated
3. Task files were edited directly and IDs became misaligned

### Fixing ID Collisions

**Important:** Do not delete tasks to resolve collisions. Instead, update the ID of the task in `queue.json` to the next available unique ID.

**Steps to fix:**

1. Identify the colliding ID (e.g., `RQ-0452` exists in both files)
2. Check if the tasks are different (different titles, descriptions, or content)
3. Find the next available ID using:
   ```bash
   ralph queue next-id
   ```
4. Update the task ID in `queue.json` to the next available ID
5. Re-run validation to confirm:
   ```bash
   ralph queue validate
   ```

**Example:**

If `RQ-0452` exists in both `done.json` (completed task about "Fix Kimi runner") and `queue.json` (new task about "Add feature X"), the fix is to change the queue task's ID to `RQ-0453` (or whatever `next-id` returns).

### Prevention

- Use `ralph task` commands to create tasks (handles ID generation automatically)
- Use `ralph queue next-id` to get the next ID when manually editing files
- Always run `ralph queue validate` after manual edits to catch issues early

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

## Import and Export

Ralph supports importing and exporting tasks for bulk operations, cross-repo migration, and integration with external tools.

### Export

Export tasks to CSV, TSV, JSON, Markdown, or GitHub issue format:

```bash
# Export all tasks to CSV (default)
ralph queue export

# Export to JSON for scripting
ralph queue export --format json --output tasks.json

# Export tasks with specific tags to TSV
ralph queue export --format tsv --tag rust --tag cli
```

### Import

Import tasks from CSV, TSV, or JSON into the active queue. This enables bulk backlog seeding and cross-repo task migration without hand-editing JSON.

```bash
# Import from JSON file
ralph queue import --format json --input tasks.json

# Import from CSV with dry-run to preview changes
ralph queue import --format csv --input tasks.csv --dry-run

# Pipe export to import (round-trip test)
ralph queue export --format json | ralph queue import --format json --dry-run
```

**Normalization**: During import, Ralph automatically:
- Trims all fields and drops empty list items
- Backfills missing `created_at`/`updated_at` timestamps
- Sets `completed_at` for tasks with `done`/`rejected` status
- Generates IDs for tasks without them
- Validates the final queue state before writing

**Duplicate handling**: Use `--on-duplicate` to control behavior when imported task IDs already exist:
- `fail` (default): error on duplicates
- `skip`: drop duplicate tasks
- `rename`: generate fresh IDs for duplicates

See `docs/cli.md` for full import documentation.
