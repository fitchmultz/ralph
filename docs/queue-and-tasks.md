# Queue and Tasks

Purpose: Define the queue file format, task fields, and status lifecycle based on `schemas/queue.schema.json`.

## Queue File
The queue file (`.ralph/queue.json`) is the source of truth for active work. Completed tasks are moved to `.ralph/done.json`.

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
- `tags` (list of strings, can be empty)
- `scope` (list of strings, can be empty)
- `evidence` (list of strings, can be empty)
- `plan` (list of strings, can be empty)
- `notes` (list of strings, can be empty)
- `depends_on` (list of task IDs, can be empty)
- `custom_fields` (map of strings, can be empty)

Common optional fields:
- `status`: `draft`, `todo`, `doing`, `done`, `rejected` (default: `todo`).
- `priority`: `critical`, `high`, `medium`, `low` (default: `medium`).
- `request`: original human request (string or null).
- `completed_at`: RFC3339 UTC timestamp (required if status is `done` or `rejected`, otherwise optional).
- `agent`: per-task runner override (see below).

Per-task agent overrides:
- `agent.runner`: `codex`, `opencode`, `gemini`, or `claude`.
- `agent.model`: model id string.
- `agent.reasoning_effort`: `minimal`, `low`, `medium`, `high`.

## Example Task
```json
{
  "id": "RQ-0007",
  "title": "Add CI validation for queue format",
  "status": "doing",
  "priority": "high",
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
    "reasoning_effort": "medium"
  }
}
```

## Lifecycle Notes
- Tasks run in the file order from `.ralph/queue.json`.
- Completed tasks are removed from `.ralph/queue.json` and appended to `.ralph/done.json`.
- Draft tasks (`status: draft`) are skipped by `run one` and `run loop` unless `--include-draft` is set.
