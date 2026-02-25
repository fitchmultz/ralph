# Ralph Task System

![Task Lifecycle](../assets/images/2026-02-07-11-32-24-task-lifecycle.png)

The Task System is the core unit of work in Ralph. Tasks represent discrete pieces of work with rich metadata, relationships, and execution configuration. This document provides comprehensive coverage of task concepts, fields, lifecycle, and operations.

---

## Table of Contents

1. [Overview](#overview)
2. [Task Fields](#task-fields)
3. [Task Status Lifecycle](#task-status-lifecycle)
4. [Task Priority](#task-priority)
5. [Task Relationships](#task-relationships)
6. [Per-Task Agent Configuration](#per-task-agent-configuration)
7. [Task Creation](#task-creation)
8. [Task Editing](#task-editing)
9. [Task Templates](#task-templates)
10. [Task Validation](#task-validation)

---

## Overview

### What is a Task?

A **Task** in Ralph is a JSON object representing a discrete unit of work. Tasks are stored in `.ralph/queue.jsonc` (active work) or `.ralph/done.jsonc` (completed work). Each task has:

- **Identity**: Unique ID, title, timestamps
- **State**: Status, priority, tags
- **Context**: Scope, evidence, plan, notes, description
- **Relationships**: Dependencies, blocking, related tasks, hierarchy
- **Execution config**: Per-task runner, model, and phase overrides

### Task as Unit of Work

Tasks serve as the fundamental interface between you and AI agents:

1. **Capture Intent**: The `request` field preserves the original human request
2. **Guide Execution**: Scope, plan, and evidence help agents understand context
3. **Track Progress**: Status transitions provide visibility into work state
4. **Enable Recovery**: Timestamps and relationships support crash recovery and planning

### Queue Files

```
.ralph/
├── queue.json   # Active tasks (source of truth for execution)
├── done.json    # Completed/rejected tasks (archive)
└── cache/       # Plans, completions, queue backups (auto-pruned to latest 50)
```

**Minimum queue structure:**
```json
{
  "version": 1,
  "tasks": []
}
```

---

## Task Fields

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique task identifier (e.g., `RQ-0001`) |
| `title` | string | Short, descriptive task title |
| `created_at` | string | RFC3339 UTC timestamp of creation |
| `updated_at` | string | RFC3339 UTC timestamp of last modification |

### Common Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `description` | string \| null | null | Detailed context, goal, and desired outcome |
| `status` | enum | `todo` | Current status: `draft`, `todo`, `doing`, `done`, `rejected` |
| `priority` | enum | `medium` | Priority level: `critical`, `high`, `medium`, `low` |
| `tags` | string[] | [] | Categorical labels for filtering/grouping |
| `scope` | string[] | [] | Starting points for work (files, paths, commands) |
| `evidence` | string[] | [] | Observed behavior, references, justifications |
| `plan` | string[] | [] | Step-by-step execution plan |
| `notes` | string[] | [] | Working notes, observations, references |
| `request` | string \| null | null | Original human request that created the task |

### Relationship Fields

| Field | Type | Description |
|-------|------|-------------|
| `depends_on` | string[] | Task IDs that must complete before this task can run |
| `blocks` | string[] | Task IDs that are blocked by this task (inverse of depends_on) |
| `relates_to` | string[] | Task IDs with loose semantic coupling (no execution constraint) |
| `duplicates` | string \| null | Task ID this task duplicates (singular reference) |
| `parent_id` | string \| null | Parent task ID for hierarchical organization |

### Agent Override Fields

| Field | Type | Description |
|-------|------|-------------|
| `agent.runner` | string \| null | Override runner: `codex`, `claude`, `opencode`, `gemini`, `cursor` |
| `agent.model` | string \| null | Override model identifier |
| `agent.model_effort` | enum | Override reasoning effort: `default`, `low`, `medium`, `high`, `xhigh` |
| `agent.iterations` | integer \| null | Number of iterations for this task (default: 1) |
| `agent.followup_reasoning_effort` | enum \| null | Reasoning effort for iterations > 1 |
| `agent.runner_cli` | object | Normalized CLI overrides (approval_mode, sandbox, etc.) |

### Scheduling Fields

| Field | Type | Description |
|-------|------|-------------|
| `started_at` | string \| null | RFC3339 UTC when work actually started |
| `completed_at` | string \| null | RFC3339 UTC when task was done/rejected |
| `scheduled_start` | string \| null | RFC3339 UTC when task should become runnable |

### Custom Fields

| Field | Type | Description |
|-------|------|-------------|
| `custom_fields` | object | User-defined key-value pairs (values coerced to strings) |

**Custom Field Constraints:**
- Keys must not contain whitespace
- Values may be string, number, or boolean (coerced to strings on load)
- Arrays and objects are not allowed as values
- Reserved analytics keys: `runner_used`, `model_used` (auto-populated on completion)

---

## Task Status Lifecycle

### Status Values

| Status | Description |
|--------|-------------|
| `draft` | Work in progress definition, skipped by default in execution |
| `todo` | Ready to work, pending dependency resolution |
| `doing` | Currently being worked on |
| `done` | Completed successfully |
| `rejected` | Will not be completed (duplicate, obsolete, out of scope) |

### Status Transitions

```
                    ┌─────────┐
         ┌─────────▶│  draft  │◀────────┐
         │          └────┬────┘         │
         │               │              │
         │               ▼              │
    ┌────┴────┐     ┌─────────┐    ┌────┴────┐
    │rejected │◀────│   todo  │───▶│  doing  │
    └────┬────┘     └────┬────┘    └────┬────┘
         │               │              │
         │               ▼              │
         └─────────▶│  done   │◀─────────┘
                    └─────────┘
```

### Transition Rules

**INTENDED BEHAVIOR:**
- `draft` → `todo`: Task definition finalized, ready for execution
- `todo` → `doing`: Work begins, `started_at` timestamp set
- `doing` → `done`: Work completed, `completed_at` timestamp set
- `doing` → `rejected`: Work abandoned
- `todo` → `rejected`: Task cancelled before starting
- Any → `draft`: Task needs redefinition

**CURRENTLY IMPLEMENTED BEHAVIOR:**
- Status cycling via CLI (`ralph task edit RQ-0001 status` with no value) cycles: `todo` → `doing` → `done` → `rejected` → `draft` → `todo`
- Direct status setting validates the target status is valid
- `started_at` is automatically set when transitioning to `doing`
- `completed_at` is automatically set when transitioning to `done` or `rejected`

### Status Policy Enforcement

```rust
// When transitioning to 'doing'
if next_status == TaskStatus::Doing && task.started_at.is_none() {
    task.started_at = Some(now.to_string());
}

// When transitioning to terminal status
if next_status.is_terminal() {
    task.completed_at = Some(now.to_string());
    // Trigger auto-archive if configured
}
```

---

## Task Priority

### Priority Levels

| Priority | Weight | Use Case |
|----------|--------|----------|
| `critical` | 3 | Blockers, security fixes, data loss prevention |
| `high` | 2 | Important features, significant improvements |
| `medium` | 1 | Normal work (default) |
| `low` | 0 | Nice-to-have, backlog items |

### Priority Ordering

Priority follows natural ordering: `Critical > High > Medium > Low`

```rust
// Comparison: Critical is "greater than" High
assert!(TaskPriority::Critical > TaskPriority::High);
assert!(TaskPriority::High > TaskPriority::Medium);
assert!(TaskPriority::Medium > TaskPriority::Low);
```

### Priority Cycling

When editing priority in an interactive UI, an empty value cycles through levels:
```
low → medium → high → critical → low
```

### Effect on Execution

**INTENDED BEHAVIOR:**
- Priority affects task ordering within the queue
- Higher priority tasks should be suggested first when multiple tasks are runnable
- Critical priority tasks may bypass normal scheduling

**CURRENTLY IMPLEMENTED BEHAVIOR:**
- Priority is stored and displayed but does not affect automatic execution order
- Tasks execute in file order (top to bottom)
- Priority can be used for manual filtering and UI sorting

---

## Task Relationships

### Dependencies (`depends_on`)

**Semantic Meaning**: "I need X before I can run"

**Execution Constraint**: A task is blocked until all tasks in `depends_on` have status `done` or `rejected`.

```json
{
  "id": "RQ-0003",
  "title": "Implement API endpoint",
  "depends_on": ["RQ-0001", "RQ-0002"]
}
```

**Validation Rules:**
- Self-dependency: **Hard error** (cannot depend on yourself)
- Missing dependency: **Hard error** (target must exist in queue or done)
- Circular dependency: **Hard error** (must form a DAG)
- Dependency on rejected task: **Warning** (will never be satisfied)

### Blocking (`blocks`)

**Semantic Meaning**: "I prevent X from running"

**Execution Constraint**: Tasks in `blocks` cannot run until this task is `done` or `rejected`.

```json
{
  "id": "RQ-0001",
  "title": "Design database schema",
  "blocks": ["RQ-0002", "RQ-0003"]
}
```

**Validation Rules:**
- Self-blocking: **Hard error**
- Missing blocked task: **Hard error**
- Circular blocking: **Hard error** (must form a DAG)

**Relationship to `depends_on`:**
- `blocks` is semantically inverse of `depends_on`
- If A `blocks` B, then B should logically `depends_on` A
- Ralph validates consistency but does not enforce bidirectional links

### Related Tasks (`relates_to`)

**Semantic Meaning**: "This work is related to X" (loose coupling)

**Execution Constraint**: None. Purely informational.

```json
{
  "id": "RQ-0005",
  "title": "Refactor auth module",
  "relates_to": ["RQ-0003", "RQ-0004"]
}
```

**Validation Rules:**
- Self-reference: **Hard error**
- Missing related task: **Hard error**

### Duplicates

**Semantic Meaning**: "This task is a duplicate of X"

**Execution Constraint**: None. Informational for cleanup.

```json
{
  "id": "RQ-0006",
  "title": "Fix login bug",
  "duplicates": "RQ-0005"
}
```

**Validation Rules:**
- Self-duplication: **Hard error**
- Missing duplicated task: **Hard error**
- Duplicate of done/rejected task: **Warning**

### Parent/Child Hierarchy (`parent_id`)

**Semantic Meaning**: "This task is a subtask of X"

**Execution Constraint**: None. Used for organizational structure.

```json
{
  "id": "RQ-0002",
  "title": "Implement Part A",
  "parent_id": "RQ-0001"
}
```

**Key Characteristics:**
- A task can have at most one parent
- A parent can have multiple children
- Cycles are not allowed (A → B → A)
- Does not affect execution order (unlike `depends_on`)

**Validation Rules:**
- Self-parent: **Warning**
- Missing parent: **Warning** (orphaned task)
- Circular parent chain: **Warning**

**CLI Navigation:**
```bash
# List children
ralph task children RQ-0001
ralph task children RQ-0001 --recursive

# Show parent
ralph task parent RQ-0002

# Visualize hierarchy
ralph queue tree
ralph queue tree --root RQ-0001
```

### Relationship Comparison

| Feature | `depends_on` | `blocks` | `relates_to` | `duplicates` | `parent_id` |
|---------|--------------|----------|--------------|--------------|-------------|
| Execution constraint | Yes | Yes (inverse) | No | No | No |
| Must form DAG | Yes | Yes | No | N/A | Yes (warnings) |
| Self-reference allowed | No | No | No | No | No |
| Validation severity | Error | Error | Error | Error | Warning |
| Visualization | `queue graph` | `queue graph` | None | None | `queue tree` |

---

## Per-Task Agent Configuration

The `agent` field allows overriding global configuration for individual tasks.

### Configuration Precedence (Highest to Lowest)

1. Per-task `agent` field in task
2. Project config (`.ralph/config.jsonc`)
3. Global config (`~/.config/ralph/config.jsonc`)
4. Schema defaults

### Override Fields

```json
{
  "id": "RQ-0001",
  "title": "Complex refactoring task",
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "model_effort": "high",
    "iterations": 2,
    "followup_reasoning_effort": "low",
    "runner_cli": {
      "approval_mode": "auto_edits",
      "sandbox": "enabled"
    }
  }
}
```

### Field Reference

| Field | Values | Description |
|-------|--------|-------------|
| `runner` | `codex`, `claude`, `opencode`, `gemini`, `cursor`, `kimi`, `pi` | Which AI runner to use |
| `model` | model identifier string | Specific model version |
| `model_effort` | `default`, `low`, `medium`, `high`, `xhigh` | Reasoning effort (Codex only) |
| `iterations` | integer ≥ 1 | Number of execution iterations |
| `followup_reasoning_effort` | `low`, `medium`, `high`, `xhigh` | Effort for iterations > 1 |

### Runner CLI Overrides

```json
{
  "agent": {
    "runner_cli": {
      "approval_mode": "yolo",        // "default", "auto_edits", "yolo", "safe"
      "output_format": "stream_json",  // "stream_json", "json", "text"
      "plan_mode": "disabled",        // "default", "enabled", "disabled"
      "sandbox": "enabled",           // "default", "enabled", "disabled"
      "verbosity": "verbose",          // "quiet", "normal", "verbose"
      "unsupported_option_policy": "warn"  // "ignore", "warn", "error"
    }
  }
}
```

### Override Behavior Notes

**INTENDED BEHAVIOR:**
- `agent.model_effort: default` falls back to config's `agent.reasoning_effort`
- `agent.followup_reasoning_effort` is ignored for non-Codex runners
- CLI overrides should merge with config, with CLI taking precedence

**CURRENTLY IMPLEMENTED BEHAVIOR:**
- Overrides are resolved at task execution time
- Some runners may not support all CLI options (handled per `unsupported_option_policy`)
- `approval_mode=safe` fails fast in non-interactive contexts (task building/updating)

---

## Task Creation

### Methods Overview

| Method | Command | Use Case |
|--------|---------|----------|
| Direct CLI | `ralph task "description"` | Quick task creation |
| Task Builder | `ralph task build "description"` | AI-assisted task generation |
| Template | `ralph task template build <name>` | From predefined template |
| Refactor Scan | `ralph task refactor` | Auto-generate from large files |
| Import | `ralph queue import` | Bulk import from CSV/JSON |
| Clone | `ralph task clone RQ-0001` | Duplicate existing task |
| App (macOS) | `ralph app open` | Visual task creation and triage |

### Direct CLI Creation

```bash
# Create task from description
ralph task "Add user authentication to API"

# With tags and scope hints
ralph task "Fix memory leak" --tags bug,rust --scope src/memory.rs

# With runner override
ralph task "Complex analysis" --runner claude --effort high
```

**Positioning:** New tasks are inserted at the top of the queue (position 0), or position 1 if the first task is already `doing`.

### Task Builder (AI-Assisted)

```bash
# AI generates task fields from description
ralph task build "Implement OAuth2 flow with Google and GitHub providers"

# With template hint
ralph task build "Fix race condition" --template bug

# With strict template validation
ralph task build "Add feature" --template feature --strict-templates
```

The task builder uses the prompt at `.ralph/prompts/task_builder.md` (or embedded default) to guide AI task generation.

### Template-Based Creation

```bash
# List available templates
ralph task template list

# Show template details
ralph task template show bug

# Create from template
ralph task template build bug "Login form validation fails on Safari"

# Create with target substitution
ralph task template build refactor "Split large module" --target src/main.rs
```

**Built-in Templates:**
- `bug` - Bug fix tasks
- `feature` - New feature tasks
- `refactor` - Code refactoring tasks
- `test` - Test writing tasks
- `docs` - Documentation tasks

**Custom Templates:** Place JSON files in `.ralph/templates/` to override or extend.

### Refactor Scan

```bash
# Scan for large files and create refactor tasks
ralph task refactor

# With custom threshold (default: 500 LOC)
ralph task refactor --threshold 800

# Dry run to preview
ralph task refactor --dry-run

# Batch modes
ralph task refactor --batch never       # One task per file
ralph task refactor --batch auto        # Group related files (default)
ralph task refactor --batch aggressive  # Group by directory
```

Scans for `.rs` files exceeding the LOC threshold (excluding comments/empty lines).

### Import

```bash
# Import from JSON
ralph queue import --format json --input tasks.json

# Import from CSV with preview
ralph queue import --format csv --input tasks.csv --dry-run

# Handle duplicates
ralph queue import --format json --input tasks.json --on-duplicate rename
```

**Normalization during import:**
- Trims all fields, drops empty list items
- Backfills missing timestamps
- Sets `completed_at` for terminal statuses
- Generates IDs for tasks without them

### Clone

```bash
# Clone existing task
ralph task clone RQ-0001

# Clone with status override
ralph task clone RQ-0001 --status todo

# Clone with title prefix
ralph task clone RQ-0001 --title-prefix "[Follow-up] "
```

Creates a new task with copied fields (except ID and timestamps) and a reference in `relates_to`.

---

## Task Editing

### Edit Commands

```bash
# Edit single field
ralph task edit priority high RQ-0001
ralph task edit status doing RQ-0001
ralph task edit tags "rust,cli" RQ-0001

# Edit multiple tasks
ralph task edit priority low RQ-0001 RQ-0002 RQ-0003

# Edit by tag filter
ralph task edit status doing --tag-filter rust

# Dry run to preview
ralph task edit scope "src/auth.rs" RQ-0001 --dry-run
```

### Editable Fields

| Field | Input Format | Example |
|-------|--------------|---------|
| `title` | string | `"New title"` |
| `status` | enum or empty (cycles) | `doing`, `""` |
| `priority` | enum or empty (cycles) | `high`, `""` |
| `tags` | comma/newline separated | `rust,cli` |
| `scope` | comma/newline separated | `src/main.rs,src/lib.rs` |
| `evidence` | comma/newline separated | `logs/error.txt` |
| `plan` | comma/newline separated | `Step 1, Step 2` |
| `notes` | comma/newline separated | `Note 1; Note 2` |
| `depends_on` | comma/newline separated | `RQ-0001,RQ-0002` |
| `blocks` | comma/newline separated | `RQ-0003` |
| `relates_to` | comma/newline separated | `RQ-0004` |
| `duplicates` | string or empty | `RQ-0005`, `""` |
| `custom_fields` | key=value pairs | `severity=high,owner=ralph` |

### Custom Field Editing

```bash
# Set custom fields
ralph task field severity high RQ-0001
ralph task field owner platform RQ-0001
ralph task field story-points 5 RQ-0001

# Set on multiple tasks
ralph task field sprint 24 RQ-0001 RQ-0002 RQ-0003
```

### AI-Powered Update

```bash
# AI updates fields based on repository state
ralph task update RQ-0001

# Update specific fields
ralph task update RQ-0001 --fields scope,evidence

# Update all tasks
ralph task update --fields all

# Dry run
ralph task update RQ-0001 --dry-run
```

Uses the prompt at `.ralph/prompts/task_updater.md` to guide AI field updates.

### Batch Operations

```bash
# Batch status change
ralph task batch status doing RQ-0001 RQ-0002

# Batch with tag filter
ralph task batch status done --tag-filter "completed"

# Batch field edit
ralph task batch edit priority high RQ-0001 RQ-0002

# Continue on error
ralph task batch status doing RQ-0001 RQ-0002 --continue-on-error

# Dry run
ralph task batch edit priority low --tag-filter backlog --dry-run
```

---

## Task Templates

### Template Structure

Templates are partial Task JSON objects:

```json
{
  "title": "",
  "status": "todo",
  "priority": "medium",
  "tags": ["bug"],
  "scope": [],
  "evidence": [],
  "plan": [
    "Reproduce the issue",
    "Identify root cause",
    "Implement fix",
    "Add regression test",
    "Verify fix"
  ]
}
```

### Variable Substitution

Templates support variable substitution:

```json
{
  "title": "Refactor ${TARGET}",
  "scope": ["${TARGET}"]
}
```

Usage:
```bash
ralph task template build refactor "Split module" --target src/main.rs
```

### Template Locations

1. **Built-in**: Embedded in Ralph binary
2. **Custom**: `.ralph/templates/<name>.json`
3. **Project overrides**: Custom templates shadow built-ins with same name

### Creating Custom Templates

```bash
# Create template directory
mkdir -p .ralph/templates

# Create template file
cat > .ralph/templates/security.json << 'EOF'
{
  "tags": ["security"],
  "priority": "critical",
  "plan": [
    "Assess security impact",
    "Identify affected components",
    "Implement security fix",
    "Add security tests",
    "Request security review"
  ],
  "evidence": ["Security audit findings"]
}
EOF
```

---

## Task Validation

### Validation Levels

| Level | Behavior | Examples |
|-------|----------|----------|
| **Hard Errors** | Block queue operations | Invalid IDs, missing required fields, circular dependencies |
| **Warnings** | Logged but non-blocking | Deep dependency chains, dependency on rejected task |

### Hard Error Conditions

#### ID Validation
- Empty ID
- Missing `-` separator (must be `PREFIX-NUMBER`)
- Wrong prefix (must match config `id_prefix`)
- Wrong width (must match config `id_width`)
- Non-digit characters in numeric suffix
- Duplicate IDs (within queue or across queue/done)

#### Required Fields
- Missing `id`
- Missing `title` (or empty)
- Missing `created_at`
- Missing `updated_at`
- Missing `completed_at` when status is `done` or `rejected`

#### Timestamp Validation
- Invalid RFC3339 format
- Non-UTC timestamps (must end in `Z`)

#### List Field Validation
- Empty strings within lists (e.g., `["a", "", "b"]`)

#### Custom Field Validation
- Empty keys
- Keys containing whitespace
- Non-scalar values (arrays, objects, null)

#### Dependency Validation
- Self-dependency (`depends_on` contains own ID)
- Missing dependency (target doesn't exist)
- Circular dependency (cycles in `depends_on` graph)

#### Relationship Validation
- Self-blocking, self-relation, self-duplication
- Missing target task
- Circular blocking relationships

### Warning Conditions

| Warning | Trigger |
|---------|---------|
| Dependency on rejected task | Task depends on a `rejected` task |
| Deep dependency chain | Chain depth exceeds `queue.max_dependency_depth` (default: 10) |
| Blocked execution path | All dependency paths lead to incomplete/rejected tasks |
| Duplicate of done/rejected task | `duplicates` points to terminal task |
| Missing parent | `parent_id` references non-existent task |
| Self-parent | Task references itself as parent |
| Circular parent chain | Cycle in `parent_id` hierarchy |

### Running Validation

```bash
# Validate queue
ralph queue validate

# Validation runs automatically on most queue operations
ralph task edit status done RQ-0001  # Validates after edit
```

### Configuration

```json
{
  "queue": {
    "max_dependency_depth": 15
  }
}
```

---

## Complete Task Examples

### Basic Task

```json
{
  "id": "RQ-0001",
  "title": "Add user authentication",
  "description": "Implement JWT-based authentication for the API",
  "status": "todo",
  "priority": "high",
  "created_at": "2026-01-15T10:00:00Z",
  "updated_at": "2026-01-15T10:00:00Z",
  "tags": ["api", "auth", "security"],
  "scope": ["src/auth.rs", "src/middleware/"],
  "evidence": ["API spec v2.1"],
  "plan": [
    "Design JWT token structure",
    "Implement token generation",
    "Add authentication middleware",
    "Write tests"
  ],
  "notes": [],
  "request": "Add JWT authentication to protect API endpoints"
}
```

### Task with Dependencies

```json
{
  "id": "RQ-0003",
  "title": "Implement login endpoint",
  "status": "todo",
  "priority": "high",
  "created_at": "2026-01-15T10:30:00Z",
  "updated_at": "2026-01-15T10:30:00Z",
  "depends_on": ["RQ-0001", "RQ-0002"],
  "tags": ["api", "endpoint"],
  "scope": ["src/routes/login.rs"]
}
```

### Task with Agent Overrides

```json
{
  "id": "RQ-0005",
  "title": "Complex algorithm optimization",
  "status": "todo",
  "priority": "critical",
  "created_at": "2026-01-15T11:00:00Z",
  "updated_at": "2026-01-15T11:00:00Z",
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "model_effort": "xhigh",
    "iterations": 3,
    "followup_reasoning_effort": "high",
    "runner_cli": {
      "approval_mode": "auto_edits",
      "sandbox": "enabled"
    }
  },
  "tags": ["performance", "algorithm"],
  "scope": ["src/optimizer.rs"]
}
```

### Task Hierarchy

```json
{
  "id": "RQ-0010",
  "title": "Implement feature X",
  "status": "doing",
  "priority": "high",
  "created_at": "2026-01-15T12:00:00Z",
  "updated_at": "2026-01-15T14:00:00Z",
  "started_at": "2026-01-15T14:00:00Z",
  "tags": ["epic", "feature-x"]
}
```

```json
{
  "id": "RQ-0011",
  "title": "Implement feature X - Backend API",
  "status": "todo",
  "priority": "high",
  "parent_id": "RQ-0010",
  "created_at": "2026-01-15T12:30:00Z",
  "updated_at": "2026-01-15T12:30:00Z",
  "depends_on": ["RQ-0001"]
}
```

```json
{
  "id": "RQ-0012",
  "title": "Implement feature X - Frontend UI",
  "status": "todo",
  "priority": "medium",
  "parent_id": "RQ-0010",
  "created_at": "2026-01-15T12:30:00Z",
  "updated_at": "2026-01-15T12:30:00Z",
  "depends_on": ["RQ-0011"]
}
```

### Task with Custom Fields

```json
{
  "id": "RQ-0020",
  "title": "Fix critical security vulnerability",
  "status": "doing",
  "priority": "critical",
  "created_at": "2026-01-15T13:00:00Z",
  "updated_at": "2026-01-15T13:30:00Z",
  "started_at": "2026-01-15T13:30:00Z",
  "tags": ["security", "urgent"],
  "custom_fields": {
    "cve_id": "CVE-2026-1234",
    "severity": "9.8",
    "owner": "security-team",
    "sprint": "24.01",
    "story_points": "8"
  }
}
```

---

## CLI Quick Reference

| Operation | Command |
|-----------|---------|
| Create task | `ralph task "description"` |
| Build with AI | `ralph task build "description"` |
| Show task | `ralph task show RQ-0001` |
| Edit field | `ralph task edit <field> <value> RQ-0001` |
| Set custom field | `ralph task field <key> <value> RQ-0001` |
| Change status | `ralph task status <status> RQ-0001` |
| Mark done | `ralph task done RQ-0001` |
| Clone task | `ralph task clone RQ-0001` |
| Add dependency | `ralph task edit depends_on "RQ-0001,RQ-0002" RQ-0003` |
| Relate tasks | `ralph task relate RQ-0001 RQ-0002` |
| Mark duplicate | `ralph task mark-duplicate RQ-0001 RQ-0002` |
| List children | `ralph task children RQ-0001` |
| Show parent | `ralph task parent RQ-0002` |
| Validate queue | `ralph queue validate` |

---

## See Also

- [Queue and Tasks](../queue-and-tasks.md) - Queue file format and lifecycle
- [Configuration](../configuration.md) - Global and project configuration
- [CLI](../cli.md) - Complete CLI reference
- [Workflow](../workflow.md) - Execution workflow and phases
