# Ralph Queue System

The queue is the source of truth for active work in Ralph. It manages tasks through their lifecycle, from creation to completion, while providing validation, dependency tracking, and workflow orchestration.

## Table of Contents

- [Overview](#overview)
- [Queue File Format](#queue-file-format)
- [Task Lifecycle](#task-lifecycle)
- [Task Ordering](#task-ordering)
- [Task Fields](#task-fields)
- [Queue Operations](#queue-operations)
- [Queue Locking](#queue-locking)
- [Auto-Archive](#auto-archive)
- [Aging](#aging)

---

## Overview

Ralph uses two primary queue files:

| File | Purpose | Location |
|------|---------|----------|
| **Active Queue** | Source of truth for active work | `.ralph/queue.jsonc` |
| **Done Archive** | Archive for completed/rejected tasks | `.ralph/done.jsonc` |

### Key Principles

1. **Queue as Source of Truth**: The queue file is the authoritative record of what work needs to be done, in what order.

2. **File Order = Execution Order**: Tasks run in the order they appear in `queue.json` (top to bottom).

3. **Validation-First**: All queue operations validate the queue state before making changes, preventing corruption.

4. **Dependency-Aware**: The queue understands task dependencies and validates them to prevent circular dependencies or broken references.

---

## Queue File Format

### JSON Structure

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "title": "Implement feature X",
      "status": "todo",
      "priority": "high",
      "created_at": "2026-01-15T10:30:00Z",
      "updated_at": "2026-01-15T10:30:00Z",
      "tags": ["feature", "backend"],
      "scope": ["crates/api"],
      "evidence": ["make test"],
      "plan": ["Design API", "Implement endpoint", "Add tests"],
      "depends_on": []
    }
  ]
}
```

### Version Field

The `version` field indicates the queue schema version. Current version is `1`. Ralph validates this on load and will error if an unsupported version is detected.

### Tasks Array

The `tasks` array contains all tasks in the queue. Tasks are processed in array order (index 0 runs first).

### Minimum Valid Queue

```json
{
  "version": 1,
  "tasks": []
}
```

---

## Task Lifecycle

Tasks progress through a well-defined status lifecycle:

```
┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
│  Draft  │────▶│   Todo  │────▶│  Doing  │────▶│  Done   │
└─────────┘     └─────────┘     └─────────┘     └─────────┘
     │                                               │
     │                                               │
     └───────────────────────────────────────────────┘
                    (also via Rejected)
```

### Status Transitions

| From | To | How | Notes |
|------|-----|-----|-------|
| `draft` | `todo` | `ralph task ready <id>` | Promotes draft to runnable state |
| `draft` | `todo` | Auto-promotion on `--include-draft` | Draft tasks are skipped by default |
| `todo` | `doing` | `ralph task start <id>` or `ralph run one` | Marks task as in-progress |
| `todo` | `doing` | Auto-started by runner | Sets `started_at` timestamp |
| `doing` | `done` | `ralph task done <id>` | Completes task, moves to done archive |
| `doing` | `rejected` | `ralph task reject <id>` | Marks as rejected, moves to done archive |
| `done`/`rejected` | - | `ralph queue archive` | Moves terminal tasks to `.ralph/done.jsonc` |

### Status Definitions

| Status | Description | Runnable |
|--------|-------------|----------|
| `draft` | Task is being drafted, not ready for execution | No (unless `--include-draft`) |
| `todo` | Task is ready and waiting to be worked on | Yes, if dependencies satisfied |
| `doing` | Task is currently being worked on | Yes (in-progress) |
| `done` | Task completed successfully | No (terminal state) |
| `rejected` | Task was rejected/cancelled | No (terminal state) |

### Terminal Status Requirements

Tasks with status `done` or `rejected` **must** have:
- `completed_at` timestamp (RFC3339 UTC)

These tasks are eligible for archiving to `.ralph/done.jsonc`.

---

## Task Ordering

### Execution Order

**INTENDED BEHAVIOR**: Tasks execute strictly in file order (top to bottom).

**CURRENTLY IMPLEMENTED BEHAVIOR**: Same - the first runnable task in the array is selected by `ralph run one` and `ralph run loop`.

### Sorting Operations

The queue can be reordered using:

```bash
# Sort by priority (highest first, default)
ralph queue sort

# Sort by priority ascending
ralph queue sort --order ascending

# Sort by priority descending
ralph queue sort --order descending
```

> **Note**: `ralph queue sort` intentionally only supports priority sorting to prevent dangerous arbitrary reordering. For temporary viewing with different sort orders, use `ralph queue list --sort-by <field>`.

### Task Insertion Position

When new tasks are added via `ralph task \"...\"` (or `ralph task build \"...\"`) or `ralph queue import`:

- **Default**: Insert at position 0 (top of queue)
- **If first task is `doing`**: Insert at position 1 (after in-progress task)

This prevents new tasks from jumping ahead of work already in progress.

---

## Task Fields

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique task identifier (e.g., "RQ-0001") |
| `title` | string | Brief task description |
| `created_at` | string | RFC3339 UTC timestamp when task was created |
| `updated_at` | string | RFC3339 UTC timestamp of last modification |

### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `status` | enum | `"todo"` | Task status: draft, todo, doing, done, rejected |
| `priority` | enum | `"medium"` | Task priority: critical, high, medium, low |
| `description` | string\|null | null | Detailed task description |
| `tags` | string[] | [] | Categorization tags |
| `scope` | string[] | [] | Files/paths relevant to task |
| `evidence` | string[] | [] | Commands/tests to verify completion |
| `plan` | string[] | [] | Step-by-step plan for execution |
| `notes` | string[] | [] | Execution notes and observations |
| `request` | string\|null | null | Original human request |
| `depends_on` | string[] | [] | Task IDs this task depends on |
| `blocks` | string[] | [] | Task IDs blocked by this task |
| `relates_to` | string[] | [] | Related task IDs (loose coupling) |
| `duplicates` | string\|null | null | Task ID this task duplicates |
| `parent_id` | string\|null | null | Parent task ID for hierarchy |
| `completed_at` | string\|null | null | RFC3339 UTC completion timestamp |
| `started_at` | string\|null | null | RFC3339 UTC when work started |
| `scheduled_start` | string\|null | null | RFC3339 UTC when task becomes runnable |
| `custom_fields` | object | {} | User-defined key-value pairs |
| `agent` | object\|null | null | Per-task agent overrides |

### Priority Ordering

Priority values are ordered (highest to lowest):

```
critical > high > medium > low
```

### Custom Fields

Custom fields allow arbitrary key-value pairs for extensibility:

```json
{
  "custom_fields": {
    "owner": "platform-team",
    "sprint": "24",
    "story_points": "5"
  }
}
```

**INTENDED BEHAVIOR**: Values should be strings for consistency.

**CURRENTLY IMPLEMENTED BEHAVIOR**: The loader accepts string/number/boolean values and coerces them to strings. On save, all values are normalized to strings.

**Reserved Keys**: Ralph automatically writes these analytics keys to completed tasks:
- `runner_used`: The runner actually used (e.g., "codex", "claude")
- `model_used`: The model actually used (e.g., "gpt-5.3-codex")

### Agent Overrides

The `agent` field allows per-task runner/model configuration:

```json
{
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "model_effort": "high",
    "iterations": 2,
    "followup_reasoning_effort": "low",
    "runner_cli": {
      "approval_mode": "auto_edits",
      "output_format": "stream_json"
    }
  }
}
```

| Field | Description |
|-------|-------------|
| `runner` | Runner to use (codex, opencode, gemini, claude, cursor, kimi, pi) |
| `model` | Model identifier string |
| `model_effort` | Reasoning effort: default, low, medium, high, xhigh |
| `iterations` | Number of iterations (default: 1) |
| `followup_reasoning_effort` | Effort for iterations > 1 |
| `runner_cli` | CLI option overrides |

---

## Queue Operations

### Validation

Validate the queue and done archive:

```bash
# Validate active queue (and done archive if present)
ralph queue validate

# Verbose validation with warnings
ralph --verbose queue validate
```

Validation checks:
- Queue version compatibility
- Task ID format and uniqueness
- Required fields (id, title, created_at, updated_at)
- RFC3339 timestamp validity
- Dependency existence and acyclicity
- Relationship validity
- Terminal status requirements

### Archive

Move completed tasks from `queue.json` to `done.json`:

```bash
# Archive all terminal tasks (done/rejected)
ralph queue archive

# Force archive (with stale lock)
ralph queue archive --force
```

The archive operation:
1. Identifies tasks with `done` or `rejected` status
2. Stamps missing `completed_at` timestamps
3. Moves tasks to `.ralph/done.jsonc`
4. Validates the resulting queue state

### Prune

Remove old tasks from the done archive:

```bash
# Prune tasks older than 30 days
ralph queue prune --age 30

# Prune only rejected tasks
ralph queue prune --status rejected

# Keep last 100 completed tasks regardless of age
ralph queue prune --keep-last 100

# Combined filters (AND logic)
ralph queue prune --age 30 --status done --keep-last 50

# Dry run to preview changes
ralph queue prune --dry-run --age 90
```

Prune options:
- `--age`: Minimum age in days
- `--status`: Filter by status (repeatable)
- `--keep-last`: Protect N most recently completed tasks
- `--dry-run`: Preview without modifying

**Safety**: Tasks with missing or invalid `completed_at` are kept (not pruned).

### Repair

Fix common queue issues automatically:

```bash
# Repair queue and done files
ralph queue repair

# Dry run to see what would be fixed
ralph queue repair --dry-run
```

Repair operations:
- Fixes missing required fields (adds defaults)
- Resolves duplicate task IDs (remaps colliding IDs)
- Backfills missing timestamps
- Normalizes non-UTC timestamps to UTC
- Fixes missing `completed_at` for terminal tasks

### Sort

Reorder tasks by priority:

```bash
# Sort by priority descending (highest first)
ralph queue sort

# Sort by priority ascending
ralph queue sort --order ascending
```

### Import/Export

#### Export

```bash
# Export to CSV (default)
ralph queue export

# Export to different formats
ralph queue export --format json --output tasks.json
ralph queue export --format tsv --output tasks.tsv
ralph queue export --format md  # Markdown table
ralph queue export --format gh  # GitHub issue format

# Filter exports
ralph queue export --tag rust --tag cli
ralph queue export --status todo --scope crates/ralph
ralph queue export --include-archive  # Include done.json
```

#### Import

```bash
# Import from JSON
ralph queue import --format json --input tasks.json

# Import from CSV with duplicate handling
ralph queue import --format csv --input tasks.csv --on-duplicate skip
ralph queue import --format csv --input tasks.csv --on-duplicate rename

# Dry run to preview
ralph queue import --format json --input tasks.json --dry-run

# Import from stdin
ralph queue export --format json | ralph queue import --format json
```

Import normalization:
- Trims all fields and drops empty list items
- Backfills missing `created_at`/`updated_at` timestamps
- Sets `completed_at` for terminal statuses
- Generates IDs for tasks without them
- Validates final queue state before writing

Duplicate handling (`--on-duplicate`):
- `fail` (default): Error on duplicates
- `skip`: Drop duplicate tasks
- `rename`: Generate fresh IDs for duplicates

---

## Queue Locking

### Lock Mechanism

Ralph uses a directory-based lock at `.ralph/lock/` to coordinate access:

```bash
# Lock is automatically acquired by most commands
ralph queue archive
ralph queue repair
ralph queue import

# Manual unlock (use with caution)
ralph queue unlock
```

### Lock Structure

The lock directory contains:
- `owner`: Metadata about the lock holder (PID, command, timestamp, label)
- `owner_task_<pid>_<counter>`: Sidecar files for shared task locks

### Stale Lock Detection

Ralph detects stale locks by checking if the holding PID is still running:

```
Queue lock already held at: /project/.ralph/lock

Lock Holder:
  PID: 12345
  Label: run loop
  Started At: 2026-01-15T10:30:00Z
  Command: ralph run loop

Suggested Action:
  The process that held this lock is no longer running.
  Use --force to automatically clear it, or use the built-in unlock command:
  ralph queue unlock
```

### Force Flag

Use `--force` to override locks when safe:

```bash
# Bypass stale lock detection
ralph queue archive --force
ralph queue repair --force
```

> **Warning**: Only use `--force` when you're certain no other Ralph process is running.

### Shared Lock Mode

Task operations (running actual tasks) use a shared lock mode that allows:
- The supervisor (`run one`, `run loop`) holds the main lock
- Individual task executions create sidecar owner files
- Multiple tasks can run concurrently under the same supervisor

---

## Auto-Archive

### Configuration

Auto-archive automatically moves terminal tasks to done.json after a configured age:

```json
// .ralph/config.jsonc
{
  "queue": {
    "auto_archive_after_days": 7
  }
}
```

| Value | Behavior |
|-------|----------|
| `null` or omitted | Auto-archive disabled |
| `0` | Archive immediately on completion |
| `N` | Archive tasks N days after completion |

### How It Works

1. When a task is marked `done` or `rejected`, it gets a `completed_at` timestamp
2. On subsequent queue operations, tasks older than `auto_archive_after_days` are identified
3. Eligible tasks are moved from `queue.json` to `done.json`
4. The operation validates both files after the move

### Manual vs Auto Archive

| | Manual (`ralph queue archive`) | Auto-Archive |
|--|-------------------------------|--------------|
| Trigger | User command | Automatic on queue operations |
| Age filter | None (all terminal tasks) | Respects `auto_archive_after_days` |
| Use case | Immediate cleanup | Background maintenance |

---

## Aging

### Aging Buckets

Tasks are categorized by age to identify stale work:

| Bucket | Age | Indicator |
|--------|-----|-----------|
| **Fresh** | ≤ 7 days | ✅ Normal |
| **Warning** | 7-14 days | ⚠️ Getting old |
| **Stale** | 14-30 days | 🟧 Needs attention |
| **Rotten** | > 30 days | 🟥 Likely outdated |
| **Unknown** | Cannot determine | ❓ Missing timestamps |

### Aging Report

```bash
# Show aging report (default: todo, doing tasks)
ralph queue aging

# Filter by status
ralph queue aging --status todo --status doing

# JSON output for scripting
ralph queue aging --format json
```

Example output:
```
Task Aging Report
=================

Thresholds: warning > 7d, stale > 14d, rotten > 30d
Filtering by status: todo, doing

Totals (15 tasks)
  Fresh:    10
  Warning:  3
  Stale:    2

🟥 Rotten Tasks
---------------

🟧 Stale Tasks
---------------
  RQ-0005  todo       18d 2h        Update dependencies
  RQ-0007  doing      21d 15h       Refactor auth module

🟨 Warning Tasks
---------------
  RQ-0010  todo       9d 4h         Fix logging format
  RQ-0012  todo       11d 8h        Add metrics endpoint
```

### Configuring Thresholds

Customize aging thresholds in config:

```json
// .ralph/config.jsonc
{
  "queue": {
    "aging_thresholds": {
      "warning_days": 5,
      "stale_days": 10,
      "rotten_days": 20
    }
  }
}
```

**Requirement**: Must satisfy `warning_days < stale_days < rotten_days`

### Anchor Timestamp Selection

Aging is calculated from different timestamps based on status:

| Status | Primary Anchor | Fallback |
|--------|----------------|----------|
| `draft`, `todo` | `created_at` | - |
| `doing` | `started_at` | `created_at` |
| `done`, `rejected` | `completed_at` | `updated_at` → `created_at` |

---

## Example Queue Files

### Minimal Queue

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "title": "Fix login bug",
      "created_at": "2026-01-15T10:00:00Z",
      "updated_at": "2026-01-15T10:00:00Z"
    }
  ]
}
```

### Full-Featured Task

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "title": "Implement OAuth2 authentication",
      "status": "doing",
      "priority": "high",
      "description": "Add OAuth2 support for GitHub and Google providers",
      "created_at": "2026-01-10T08:00:00Z",
      "updated_at": "2026-01-15T14:30:00Z",
      "started_at": "2026-01-15T09:00:00Z",
      "tags": ["auth", "security", "oauth"],
      "scope": ["src/auth/", "src/middleware/"],
      "evidence": ["cargo test auth", "manual testing"],
      "plan": [
        "Research OAuth2 flows",
        "Implement GitHub provider",
        "Implement Google provider",
        "Add token refresh logic",
        "Write tests"
      ],
      "notes": [
        "Using oauth2 crate version 4.x",
        "PKCE flow required for security"
      ],
      "request": "Add OAuth2 login with GitHub and Google",
      "depends_on": ["RQ-0000"],
      "blocks": ["RQ-0002"],
      "relates_to": ["RQ-0003"],
      "custom_fields": {
        "owner": "security-team",
        "sprint": "S1"
      },
      "agent": {
        "runner": "codex",
        "model_effort": "high"
      }
    }
  ]
}
```

### Done Archive Example

```json
{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0000",
      "title": "Setup project structure",
      "status": "done",
      "created_at": "2026-01-01T00:00:00Z",
      "updated_at": "2026-01-02T00:00:00Z",
      "completed_at": "2026-01-02T00:00:00Z",
      "tags": ["setup"],
      "scope": [],
      "evidence": [],
      "plan": [],
      "custom_fields": {
        "runner_used": "claude",
        "model_used": "claude-sonnet-4-20250514"
      }
    }
  ]
}
```

---

## Common Workflows

### Creating and Running Tasks

```bash
# Add a new task
ralph task "Fix API validation"

# Show next task
ralph queue next

# Run the next task
ralph run one

# Mark complete and archive
ralph task done RQ-0001
ralph queue archive
```

### Bulk Operations

```bash
# Export tasks for review
ralph queue export --format md --status todo > review.md

# Import tasks from external source
ralph queue import --format csv --input backlog.csv --dry-run
ralph queue import --format csv --input backlog.csv

# Prune old done tasks
ralph queue prune --age 90 --keep-last 100
```

### Maintenance

```bash
# Regular validation
ralph queue validate

# Check for stale tasks
ralph queue aging

# Repair any issues
ralph queue repair --dry-run
ralph queue repair

# Archive completed work
ralph queue archive
```

---

## See Also

- [Queue and Tasks](../queue-and-tasks.md) - Detailed task field documentation
- [CLI Documentation](../cli.md) - All queue commands
- [Workflow](../workflow.md) - Task lifecycle and execution
- [Configuration](../configuration.md) - Queue configuration options
