# Dependencies System
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Feature Documentation](README.md)


Ralph's dependency system provides powerful task relationship management, enabling you to control execution order, visualize workflows, and analyze critical paths.

## Overview

Task dependencies in Ralph define **execution constraints**—relationships that determine when tasks can run. A task is considered **runnable** only when all its dependencies are satisfied (completed with `done` or `rejected` status).

Key capabilities:
- **Execution ordering**: Define which tasks must complete before others can start
- **Validation**: Detect cycles, missing dependencies, and self-references
- **Visualization**: View dependency trees, graphs, and critical paths
- **Analysis**: Identify bottlenecks and blocked execution paths

## Dependency Types

Ralph supports four types of task relationships, each with different semantics:

### `depends_on` - "I need X"

The primary execution constraint. A task with `depends_on` cannot run until all referenced tasks are `done` or `rejected`.

```json
{
  "id": "RQ-0003",
  "title": "Implement API endpoint",
  "depends_on": ["RQ-0001", "RQ-0002"]
}
```

**Behavior**:
- Task is blocked until all dependencies complete
- Dependencies can be in `queue.json` or `done.json`
- A `rejected` dependency satisfies the constraint (task can proceed)

### `blocks` - "I prevent X"

Semantic blocking—the inverse of `depends_on`. Task A `blocks` Task B means Task B implicitly depends on Task A.

```json
{
  "id": "RQ-0001",
  "title": "Design database schema",
  "blocks": ["RQ-0002", "RQ-0003"]
}
```

**Behavior**:
- Blocked tasks cannot run until the blocking task completes
- Forms a DAG (must be acyclic)
- Useful for expressing "this work unlocks other work"

**Note**: The `blocks` relationship is stored separately from `depends_on` but affects runnability similarly. When Task A blocks Task B, Task B is treated as having an implicit dependency on Task A.

### `relates_to` - Loose Coupling

Expresses semantic relationships without execution constraints.

```json
{
  "id": "RQ-0005",
  "title": "Update documentation",
  "relates_to": ["RQ-0003", "RQ-0004"]
}
```

**Behavior**:
- No effect on execution order
- Used for grouping and context
- Visualized as dotted lines in graphs

### `duplicates` - Marking Redundancy

Indicates that a task duplicates the work of another task.

```json
{
  "id": "RQ-0006",
  "title": "Fix login bug (duplicate)",
  "duplicates": "RQ-0005"
}
```

**Behavior**:
- No execution constraint
- Informational only
- Warns if duplicating a `done` or `rejected` task

## Task Status and Runnability

A task's runnability depends on its status and dependencies:

| Status | Runnable? | Dependencies Checked? |
|--------|-----------|----------------------|
| `todo` | Yes, if deps satisfied | Yes |
| `doing` | Already running | N/A |
| `done` | No (terminal) | No |
| `rejected` | No (terminal) | No |
| `draft` | Only with `--include-draft` | Yes |

### Runnability Rules

1. **Status check**: `done` and `rejected` tasks are never runnable
2. **Draft exclusion**: `draft` tasks are excluded unless `--include-draft` is set
3. **Dependency check**: All `depends_on` tasks must be `done` or `rejected`
4. **Schedule check**: Tasks with `scheduled_start` in the future are blocked

## Validation

Ralph validates dependencies on all queue operations to ensure correctness.

### Hard Errors (Blocking)

These prevent queue operations and must be fixed:

| Error | Cause | Fix |
|-------|-------|-----|
| **Self-dependency** | Task depends on itself | Remove the self-reference |
| **Missing dependency** | Referenced task doesn't exist | Create the task or fix the ID |
| **Circular dependency** | Cycle in `depends_on` graph | Break the cycle |
| **Self-blocking** | Task blocks itself | Remove the self-reference |
| **Circular blocking** | Cycle in `blocks` graph | Break the cycle |
| **Invalid terminal status** | `done.json` contains non-terminal tasks | Move tasks back to queue |

### Warnings (Non-Blocking)

These are logged but don't prevent operations:

| Warning | Cause | Action |
|---------|-------|--------|
| **Dependency on rejected task** | Task depends on `rejected` task | Review if task should be rejected too |
| **Deep dependency chain** | Chain exceeds `max_dependency_depth` | Simplify dependencies or increase limit |
| **Blocked execution path** | All paths lead to incomplete tasks | Identify and resolve blocking dependencies |
| **Duplicate of done/rejected task** | Task duplicates completed work | Consider if duplicate is needed |

### Validation Example

```bash
# Validate the queue
$ ralph queue validate

# Example output with warnings
[WARN] [RQ-0005] Task RQ-0005 depends on rejected task RQ-0002. This dependency will never be satisfied.
[WARN] [RQ-0001] Task RQ-0001 has a dependency chain depth of 12, which exceeds the configured maximum of 10.
```

## Dependency Graph

Internally, Ralph represents tasks as a **Directed Acyclic Graph (DAG)**. The graph structure ensures:

- **No cycles**: Dependencies must flow in one direction
- **Deterministic ordering**: Topological sort provides a valid execution order
- **Efficient queries**: Fast lookup of dependencies, dependents, and chains

### Graph Structure

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│  RQ-0001    │────────▶│  RQ-0002    │────────▶│  RQ-0003    │
│  (root)     │         │             │         │  (leaf)     │
└─────────────┘         └─────────────┘         └─────────────┘
        │                                               ▲
        │                                               │
        └───────────────────────────────────────────────┘
                    (direct dependency)
```

**Terminology**:
- **Root**: Task with no dependents (nothing depends on it)
- **Leaf**: Task with no dependencies (depends on nothing)
- **Chain**: A path through the dependency graph

## Visualization

Ralph provides multiple ways to visualize dependencies:

### ASCII Tree View

Default output of `ralph queue graph`:

```bash
$ ralph queue graph --task RQ-0003

Dependency tree for RQ-0003: Implement API endpoint

Tasks this task depends on (upstream):
* RQ-0003: Implement API endpoint [⏳]
  └─ RQ-0002: Design API schema [✅]
     └─ RQ-0001: Define requirements [✅]

Critical path from this task: 3 tasks
  Status: Unblocked
```

**Legend**:
- `*` = on critical path
- `⏳` = todo, `🔄` = doing, `✅` = done, `❌` = rejected, `📝` = draft

### DOT Format (Graphviz)

Export to Graphviz for advanced visualization:

```bash
# Export to DOT format
$ ralph queue graph --format dot > deps.dot

# Render to PNG
$ dot -Tpng deps.dot -o deps.png
```

The DOT output includes:
- **Solid edges**: `depends_on` relationships
- **Dashed edges**: `blocks` relationships  
- **Dotted edges**: `relates_to` relationships
- **Bold edges**: `duplicates` relationships
- **Red nodes**: Critical path tasks
- **Color coding**: Green (done), Orange (doing), Light blue (todo), Gray (rejected)

### JSON Format

For programmatic access:

```bash
$ ralph queue graph --format json --task RQ-0003
```

```json
{
  "task": "RQ-0003",
  "title": "Implement API endpoint",
  "status": "todo",
  "critical": true,
  "relationship": "depends_on",
  "related_tasks": [
    {
      "id": "RQ-0002",
      "title": "Design API schema",
      "status": "done",
      "critical": true
    },
    {
      "id": "RQ-0001",
      "title": "Define requirements",
      "status": "done",
      "critical": true
    }
  ]
}
```

### Full Graph Commands

```bash
# Show all dependency chains
$ ralph queue graph

# Include completed tasks
$ ralph queue graph --include-done

# Show only critical path
$ ralph queue graph --critical

# Show reverse dependencies (what this task blocks)
$ ralph queue graph --task RQ-0001 --reverse
```

## Critical Path Analysis

The **critical path** is the longest dependency chain in the graph. Tasks on the critical path are bottlenecks—delaying them delays all downstream work.

### Finding Critical Paths

```bash
# Highlight critical path in tree view
$ ralph queue graph --critical

# Critical path length is shown in summary
Summary:
  Total tasks: 12
  Ready to run: 3
  Blocked: 7
  Critical path length: 5
```

### Critical Path Visualization

In tree view, critical path tasks are marked with `*`:

```
* RQ-0005: Integration testing [⏳]
  └─ * RQ-0004: Implement service [🔄]
     └─ * RQ-0002: Design schema [✅]
        └─ RQ-0001: Requirements [✅]
```

### Impact

Completing critical path tasks unblocks the most downstream work. Prioritize these tasks to maximize throughput.

## macOS App Visualization

On macOS, you can explore dependency relationships interactively in the Ralph app:

```bash
ralph app open
```

For cross-platform and scripting use, prefer `ralph queue graph` (ASCII/DOT/JSON).

## Queue Explain

The `ralph queue explain` command provides detailed runnability analysis:

```bash
# Text explanation (default)
$ ralph queue explain

# JSON output for scripting
$ ralph queue explain --format json

# Include draft tasks
$ ralph queue explain --include-draft
```

### Example Output

```
Queue Runnability Report (generated at 2026-02-07T10:30:00Z)

Selection: include_draft=false, prefer_doing=true
Selected task: RQ-0004 (status: Doing)

Summary:
  Total tasks: 8
  Candidates: 5 (runnable: 1)
  Blocked by dependencies: 4

Blocking reasons (first 10 candidates):
  RQ-0005 (status: Todo):
    - Blocked by unmet dependencies:
      * RQ-0004: status is 'Doing' (must be done/rejected)
  RQ-0006 (status: Todo):
    - Blocked by unmet dependencies:
      * RQ-0005: status is 'Todo' (must be done/rejected)

Hints:
  - Run 'ralph queue graph --task <ID>' to visualize dependencies
  - Run 'ralph run one --dry-run' to see what would be selected
```

### JSON Report Structure

```json
{
  "version": 1,
  "now": "2026-02-07T10:30:00Z",
  "selection": {
    "include_draft": false,
    "prefer_doing": true,
    "selected_task_id": "RQ-0004",
    "selected_task_status": "Doing"
  },
  "summary": {
    "total_active": 8,
    "candidates_total": 5,
    "runnable_candidates": 1,
    "blocked_by_dependencies": 4,
    "blocked_by_schedule": 0,
    "blocked_by_status_or_flags": 0
  },
  "tasks": [
    {
      "id": "RQ-0005",
      "status": "Todo",
      "runnable": false,
      "reasons": [
        {
          "kind": "unmet_dependencies",
          "dependencies": [
            {
              "kind": "not_complete",
              "id": "RQ-0004",
              "status": "Doing"
            }
          ]
        }
      ]
    }
  ]
}
```

## Configuration

### `max_dependency_depth`

Controls the warning threshold for deep dependency chains.

**Default**: `10`

**Configuration**:

```json
{
  "version": 1,
  "queue": {
    "max_dependency_depth": 15
  }
}
```

**Behavior**:
- Warnings are issued when a task's dependency chain exceeds this depth
- Does not prevent operations (non-blocking)
- Useful for detecting overly complex task decomposition

### When to Adjust

- **Increase** (e.g., to 15-20): For large projects with naturally deep dependency chains
- **Decrease** (e.g., to 5-7): To enforce flatter task structures

## Hierarchy vs Dependencies

It's important to distinguish between **structural hierarchy** (`parent_id`) and **execution dependencies** (`depends_on`):

| Feature | `parent_id` | `depends_on` |
|---------|-------------|--------------|
| **Purpose** | Structural organization | Execution ordering |
| **Affects task order** | No | Yes |
| **Visualized with** | `ralph queue tree` | `ralph queue graph` |
| **Validation** | Warnings for cycles/missing | Hard errors for cycles/missing |
| **Direction** | Child → Parent | Task → Dependency |

### Example: Combined Usage

```json
{
  "id": "RQ-0001",
  "title": "Implement user authentication epic",
  "status": "doing"
},
{
  "id": "RQ-0002",
  "title": "Design auth schema",
  "status": "done",
  "parent_id": "RQ-0001"
},
{
  "id": "RQ-0003",
  "title": "Implement login endpoint",
  "status": "todo",
  "parent_id": "RQ-0001",
  "depends_on": ["RQ-0002"]
},
{
  "id": "RQ-0004",
  "title": "Implement logout endpoint",
  "status": "todo",
  "parent_id": "RQ-0001",
  "depends_on": ["RQ-0002"]
}
```

**Structure**:
- RQ-0001 (epic)
  - RQ-0002 (child)
  - RQ-0003 (child, depends on RQ-0002)
  - RQ-0004 (child, depends on RQ-0002)

**Execution**: RQ-0002 → (RQ-0003, RQ-0004 can run in any order)

## Practical Examples

### Example 1: Feature Development Chain

```json
[
  {
    "id": "RQ-0001",
    "title": "Define API specification",
    "status": "done",
    "depends_on": []
  },
  {
    "id": "RQ-0002",
    "title": "Design database schema",
    "status": "done",
    "depends_on": ["RQ-0001"]
  },
  {
    "id": "RQ-0003",
    "title": "Implement backend service",
    "status": "doing",
    "depends_on": ["RQ-0002"]
  },
  {
    "id": "RQ-0004",
    "title": "Implement frontend components",
    "status": "todo",
    "depends_on": ["RQ-0001"]
  },
  {
    "id": "RQ-0005",
    "title": "Integration testing",
    "status": "todo",
    "depends_on": ["RQ-0003", "RQ-0004"]
  }
]
```

**Visualization**:
```
RQ-0001 (Define API)
    ├── RQ-0002 (Design schema) ──▶ RQ-0003 (Backend) ──┐
    │                                                    ├──▶ RQ-0005 (Testing)
    └── RQ-0004 (Frontend) ──────────────────────────────┘
```

### Example 2: Handling Circular Dependencies

**INTENDED BEHAVIOR**: Ralph should detect and reject circular dependencies.

**CURRENTLY IMPLEMENTED BEHAVIOR**: 
- Circular `depends_on` chains are detected during validation
- Operations are blocked with a clear error message
- User must break the cycle before proceeding

```bash
# Attempting to validate with circular dependency
$ ralph queue validate

Error: Circular dependency detected involving task RQ-0001. 
Task dependencies must form a DAG (no cycles). 
Review the depends_on fields to break the cycle.
```

**Fixing a circular dependency**:

```json
// BEFORE: Circular (RQ-0001 → RQ-0002 → RQ-0001)
{
  "id": "RQ-0001",
  "depends_on": ["RQ-0002"]
}
{
  "id": "RQ-0002",
  "depends_on": ["RQ-0001"]
}

// AFTER: Linear (RQ-0001 → RQ-0002)
{
  "id": "RQ-0001",
  "depends_on": []
}
{
  "id": "RQ-0002",
  "depends_on": ["RQ-0001"]
}
```

### Example 3: Critical Path Analysis

```bash
# View full graph with critical path highlighting
$ ralph queue graph

Task Dependency Graph

Summary:
  Total tasks: 6
  Ready to run: 1
  Blocked: 4
  Critical path length: 4

Dependency Chains:

* RQ-0006: Deploy to production [⏳]
  └─ * RQ-0005: Run integration tests [⏳]
     └─ * RQ-0004: Implement feature [🔄]
        └─ * RQ-0003: Setup environment [✅]

RQ-0002: Write documentation [⏳]
  └─ RQ-0001: Define requirements [✅]

Legend:
  * = on critical path
  ⏳ = todo, 🔄 = doing, ✅ = done, ❌ = rejected
```

**Analysis**:
- RQ-0003, RQ-0004, RQ-0005, RQ-0006 are on the critical path
- RQ-0002 is not critical (can run in parallel with RQ-0003)
- Completing RQ-0004 unblocks the longest chain

### Example 4: Dependency on Rejected Task

```json
{
  "id": "RQ-0001",
  "title": "Research approach A",
  "status": "rejected"
},
{
  "id": "RQ-0002",
  "title": "Implement approach B",
  "status": "todo",
  "depends_on": ["RQ-0001"]
}
```

**Warning issued**:
```
[WARN] [RQ-0002] Task RQ-0002 depends on rejected task RQ-0001. 
This dependency will never be satisfied.
```

**Behavior**:
- RQ-0002 is considered **runnable** because `rejected` satisfies the dependency constraint
- Warning alerts you to review if RQ-0002 should also be rejected

## Best Practices

1. **Keep chains shallow**: Prefer depth ≤ 5 for clarity
2. **Use `blocks` for intent**: When task A "enables" task B, use `blocks` to express this
3. **Check critical path**: Prioritize tasks on the critical path
4. **Validate frequently**: Run `ralph queue validate` after editing dependencies
5. **Use `relates_to` liberally**: Add context without affecting execution
6. **Mark duplicates**: Use `duplicates` to track redundant work
7. **Review warnings**: Address dependency warnings promptly

## Troubleshooting

### Task appears runnable but isn't selected

```bash
# Check for unmet dependencies
$ ralph queue explain

# Visualize the dependency chain
$ ralph queue graph --task <TASK-ID>
```

### Validation fails with "Circular dependency"

1. Visualize the graph to find the cycle
2. Identify the loop in `depends_on` fields
3. Break the cycle by removing one dependency

### "Missing dependency" error

- Check that the task ID exists in `queue.json` or `done.json`
- Verify the ID spelling and case
- Use `ralph queue list --include-done` to search for the task

### Deep dependency chain warning

- Review task decomposition (may be too granular)
- Increase `max_dependency_depth` if appropriate
- Consider grouping related tasks

## See Also

- [Queue and Tasks](../queue-and-tasks.md) - Task fields and status lifecycle
- [Configuration](../configuration.md) - Configuring `max_dependency_depth`
- [CLI Reference](../cli.md) - `ralph queue graph`, `ralph queue explain`
