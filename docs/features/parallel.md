# Ralph Parallel Execution

Parallel execution runs multiple tasks concurrently in isolated git workspace clones, with automatic PR creation and merge handling.

> **CLI Only**: Parallel execution is available only via CLI (`ralph run loop --parallel [N]`).

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Workspace Management](#workspace-management)
4. [Branch Management](#branch-management)
5. [PR Automation](#pr-automation)
6. [Configuration](#configuration)
7. [State Management](#state-management)
8. [Merge Runner](#merge-runner)
9. [Limitations](#limitations)
10. [Workflow](#workflow)
11. [Monitoring](#monitoring)

---

## Overview

### What is Parallel Execution?

Parallel execution enables Ralph to process multiple queue tasks simultaneously by:

- Running each task in its own isolated git workspace clone
- Creating Pull Requests automatically for completed work
- Merging PRs as they become eligible (or after all tasks complete)
- Auto-resolving merge conflicts using an AI runner
- Tracking all state for crash recovery and coordination

### When to Use Parallel Execution

| Use Case | Recommendation |
|----------|---------------|
| Multiple independent tasks | ✅ Ideal for parallel execution |
| Tasks with no dependencies | ✅ Parallel execution works well |
| Tasks requiring rapid completion | ✅ Significantly faster than sequential |
| Tasks requiring careful review | ⚠️ Consider `merge_when: after_all` |
| Tasks with heavy resource usage | ⚠️ Adjust `workers` to avoid overload |
| Tasks with complex interdependencies | ❌ Use sequential mode instead |

### Basic Usage

```bash
# Run with default settings (uses config value or fails)
ralph run loop --parallel

# Run with specific number of workers
ralph run loop --parallel 4

# Run with max tasks limit
ralph run loop --parallel 3 --max-tasks 10
```

---

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Parallel Coordinator                         │
│                    (Main ralph process)                          │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐ │
│  │   Worker 1  │  │   Worker 2  │  │        Worker N         │ │
│  │  (process)  │  │  (process)  │  │       (process)         │ │
│  └──────┬──────┘  └──────┬──────┘  └───────────┬─────────────┘ │
│         │                │                     │               │
│         ▼                ▼                     ▼               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐ │
│  │ Workspace 1 │  │ Workspace 2 │  │      Workspace N        │ │
│  │(git clone)  │  │(git clone)  │  │      (git clone)        │ │
│  │branch:      │  │branch:      │  │      branch: ralph/RQ-NN│ │
│  │ralph/RQ-0001│  │ralph/RQ-0002│  │                         │ │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Merge Runner Thread                        │
│              (Background PR monitoring/merging)                  │
├─────────────────────────────────────────────────────────────────┤
│  - Polls PR merge status                                         │
│  - Handles merge conflicts                                       │
│  - Applies queue sync after merge                                │
└─────────────────────────────────────────────────────────────────┘
```

### Key Components

#### 1. Coordinator (`orchestration.rs`)

The main supervisor process that:
- Holds the queue lock for the entire parallel run
- Spawns workers up to the configured limit
- Polls worker status every 500ms
- Creates PRs on task completion/failure
- Coordinates with the merge runner

#### 2. Workers (`worker.rs`)

Independent subprocesses that:
- Run `ralph run one --id <TASK_ID> --parallel-worker`
- Execute in isolated workspace directories
- Inherit config/prompts from the coordinator
- Have RepoPrompt forcibly disabled

#### 3. State File (`state.rs`)

JSON file at `.ralph/cache/parallel/state.json` tracking:
- `tasks_in_flight`: Currently running tasks
- `prs`: Created PRs with lifecycle state
- `finished_without_pr`: Tasks that completed without PR creation
- Base branch and merge settings

#### 4. Cleanup Guard (`cleanup_guard.rs`)

RAII guard ensuring:
- Workers are terminated on interrupt/error
- Workspaces are cleaned up appropriately
- State file is persisted
- Merge runner thread is joined

---

## Workspace Management

### Directory Structure

#### Default Location

```
<repo-parent>/.workspaces/<repo-name>/parallel/
├── RQ-0001/          # Task workspace
│   ├── .git/         # Git metadata
│   ├── .ralph/       # Ralph state (config, prompts)
│   └── ...           # Repository files
├── RQ-0002/          # Another task workspace
├── .base-sync/       # Ephemeral workspace for merge sync
└── ...
```

#### Custom Location

Configure via `parallel.workspace_root` in config:

```json
{
  "parallel": {
    "workspace_root": "/path/to/workspaces"
  }
}
```

### Workspace Creation Process

1. **Clone**: Create git clone from origin
2. **Checkout**: Create and checkout branch `ralph/<task_id>`
3. **Sync**: Copy config and prompts from main repo
4. **Execute**: Run worker process in workspace

### Gitignore Requirements

**CRITICAL**: If `workspace_root` is inside the repository, it MUST be gitignored.

```bash
# Add to .gitignore (shared)
echo ".workspaces/" >> .gitignore

# Or add to .git/info/exclude (local-only)
echo ".workspaces/" >> .git/info/exclude
```

Ralph performs a preflight check and will fail fast with an actionable error if the workspace root is not gitignored.

### What Gets Synced to Workspaces

| Item | Synced? | Reason |
|------|---------|--------|
| `config.json` | ✅ Yes | Workers need configuration |
| `prompts/*.md` | ✅ Yes | Custom prompt overrides |
| `.env` files | ✅ Yes | Allowlisted gitignored files |
| `queue.json` | ❌ No | Coordinator-only |
| `done.json` | ❌ No | Coordinator-only |
| Build artifacts | ❌ No | Excluded by policy |
| Cache directories | ❌ No | Excluded by policy |

---

## Branch Management

### Branch Naming Convention

Branches are named using the pattern: `{branch_prefix}{task_id}`

Default prefix: `ralph/`

Examples:
- Task `RQ-0001` → Branch `ralph/RQ-0001`
- Task `RQ-0002` → Branch `ralph/RQ-0002`

### Configurable Branch Prefix

```json
{
  "parallel": {
    "branch_prefix": "feature/ralph-"
  }
}
```

With this config:
- Task `RQ-0001` → Branch `feature/ralph-RQ-0001`

### Branch Protection

Ralph validates PR head branches match the expected naming convention. If a mismatch is detected:

1. A `merge_blocker` is set in the state file
2. The PR is skipped for auto-merge
3. A warning is logged with details

This prevents accidental merges when branch naming conventions change.

### Conflict Handling

When two parallel tasks modify the same files:

1. First PR to become eligible merges cleanly
2. Second PR enters "Dirty" merge state
3. Merge runner attempts auto-resolution (if enabled)
4. If auto-resolution fails, PR remains open for manual resolution

---

## PR Automation

### PR Creation (`auto_pr`)

When enabled, Ralph automatically creates PRs for:

**Successful Tasks:**
- Title: `<TASK_ID>: <TASK_TITLE>`
- Body: Phase 2 final response (implementation summary)
- Branch: `ralph/<TASK_ID>` → Base: configured base branch
- Draft: `false`

**Failed Tasks** (with `draft_on_failure: true`):
- Title: `<TASK_ID>: <TASK_TITLE>`
- Body: "Failed run for <TASK_ID>. Draft PR generated by Ralph."
- Branch: `ralph/<TASK_ID>` → Base: configured base branch
- Draft: `true`

### PR Merging (`auto_merge`)

Two merge timing strategies:

| Setting | Behavior | Use Case |
|---------|----------|----------|
| `as_created` | Merge PRs as soon as they become eligible | Continuous integration |
| `after_all` | Wait for all tasks, then merge all PRs | Batch review workflow |

### Merge Methods

```json
{
  "parallel": {
    "merge_method": "squash"  // Options: "squash", "merge", "rebase"
  }
}
```

### Draft on Failure (`draft_on_failure`)

When enabled and a task fails:

1. Changes are committed (if any)
2. Branch is pushed
3. Draft PR is created
4. Auto-merge is skipped for this PR

This allows developers to review and recover failed work.

### Delete Branch on Merge

```json
{
  "parallel": {
    "delete_branch_on_merge": true
  }
}
```

When enabled, feature branches are deleted after successful merge.

---

## Configuration

### All Parallel Configuration Options

```json
{
  "version": 1,
  "parallel": {
    "workers": 3,
    "merge_when": "as_created",
    "merge_method": "squash",
    "auto_pr": true,
    "auto_merge": true,
    "draft_on_failure": true,
    "conflict_policy": "auto_resolve",
    "merge_retries": 5,
    "workspace_root": "/path/to/workspaces",
    "branch_prefix": "ralph/",
    "delete_branch_on_merge": true,
    "merge_runner": {
      "runner": "claude",
      "model": "sonnet",
      "reasoning_effort": "medium"
    }
  }
}
```

### Configuration Reference

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `workers` | `integer` | `null` | Number of concurrent workers (≥2) |
| `merge_when` | `string` | `"as_created"` | When to merge: `"as_created"` or `"after_all"` |
| `merge_method` | `string` | `"squash"` | Merge method: `"squash"`, `"merge"`, or `"rebase"` |
| `auto_pr` | `boolean` | `true` | Auto-create PRs for completed tasks |
| `auto_merge` | `boolean` | `true` | Auto-merge eligible PRs |
| `draft_on_failure` | `boolean` | `true` | Create draft PRs on task failure |
| `conflict_policy` | `string` | `"auto_resolve"` | Conflict handling: `"auto_resolve"`, `"retry_later"`, `"reject"` |
| `merge_retries` | `integer` | `5` | Max merge retry attempts |
| `workspace_root` | `string` | `<repo-parent>/.workspaces/<repo-name>/parallel` | Root directory for workspaces |
| `branch_prefix` | `string` | `"ralph/"` | Prefix for branch names |
| `delete_branch_on_merge` | `boolean` | `true` | Delete branches after merge |
| `merge_runner` | `object` | `null` | Runner config for conflict resolution |

### Merge Runner Configuration

The `merge_runner` config specifies which runner/model to use for auto-resolving merge conflicts:

```json
{
  "parallel": {
    "merge_runner": {
      "runner": "claude",
      "model": "sonnet",
      "reasoning_effort": "medium"
    }
  }
}
```

If not specified, inherits from `agent.*` settings.

### CLI Overrides

| Flag | Config Key | Description |
|------|------------|-------------|
| `--parallel [N]` | `workers` | Enable parallel mode with N workers |
| `--max-tasks N` | - | Limit total tasks to process |
| `--merge-when [as_created\|after_all]` | `merge_when` | Override merge timing |
| `--git-commit-push-on/off` | `git_commit_push_enabled` | Control git operations |

### Configuration Examples

**Basic parallel setup (3 workers):**
```json
{
  "parallel": {
    "workers": 3
  }
}
```

**Review-before-merge workflow:**
```json
{
  "parallel": {
    "workers": 4,
    "merge_when": "after_all",
    "auto_merge": false,
    "auto_pr": true
  }
}
```

**Custom branch naming:**
```json
{
  "parallel": {
    "workers": 2,
    "branch_prefix": "agent/",
    "workspace_root": "/tmp/ralph-workspaces"
  }
}
```

---

## State Management

### State File Location

```
.ralph/cache/parallel/state.json
```

### State File Structure

```json
{
  "started_at": "2026-02-07T10:30:00Z",
  "base_branch": "main",
  "merge_method": "squash",
  "merge_when": "as_created",
  "tasks_in_flight": [
    {
      "task_id": "RQ-0001",
      "workspace_path": "/path/to/workspaces/RQ-0001",
      "branch": "ralph/RQ-0001",
      "pid": 12345,
      "started_at": "2026-02-07T10:30:00Z"
    }
  ],
  "prs": [
    {
      "task_id": "RQ-0001",
      "pr_number": 42,
      "pr_url": "https://github.com/user/repo/pull/42",
      "head": "ralph/RQ-0001",
      "base": "main",
      "workspace_path": "/path/to/workspaces/RQ-0001",
      "merged": false,
      "lifecycle": "open",
      "merge_blocker": null
    }
  ],
  "finished_without_pr": [
    {
      "task_id": "RQ-0005",
      "workspace_path": "/path/to/workspaces/RQ-0005",
      "branch": "ralph/RQ-0005",
      "success": true,
      "finished_at": "2026-02-07T10:35:00Z",
      "reason": "auto_pr_disabled",
      "message": null
    }
  ]
}
```

### State File Fields

| Field | Type | Description |
|-------|------|-------------|
| `started_at` | `string` | RFC3339 timestamp when parallel run started |
| `base_branch` | `string` | Base branch for PRs |
| `merge_method` | `string` | Merge method for this run |
| `merge_when` | `string` | Merge timing for this run |
| `tasks_in_flight` | `array` | Currently running tasks |
| `prs` | `array` | Created PRs with lifecycle state |
| `finished_without_pr` | `array` | Tasks completed without PR |

### PR Lifecycle States

| State | Description |
|-------|-------------|
| `open` | PR is open and not yet merged |
| `closed` | PR was closed without merging |
| `merged` | PR was successfully merged |

### Finished Without PR Reasons

| Reason | Description | Blocking Behavior |
|--------|-------------|-------------------|
| `auto_pr_disabled` | PR automation was disabled | Blocks only while auto_pr disabled |
| `draft_pr_disabled` | Draft PR on failure was disabled | Blocks only while draft_on_failure disabled |
| `pr_create_failed` | PR creation failed (API error, etc.) | Blocks for 24h TTL |
| `draft_pr_skipped_no_changes` | Worker failed with no changes | Blocks for 24h TTL |
| `unknown` | Unknown/unexpected reason | Blocks for 24h TTL |

### Crash Recovery

On startup, Ralph performs state recovery:

1. **Prune stale tasks**: Remove tasks with missing workspaces or dead PIDs
2. **Reconcile PRs**: Query GitHub to update PR lifecycle states
3. **Clean workspaces**: Remove workspaces for merged/closed PRs
4. **Prune non-blocking**: Clear finished-without-pr records that no longer block
5. **Validate base branch**: Ensure base branch consistency

If the base branch doesn't match:
- With no blocking work: Auto-heal to current branch
- With blocking work: Fail with recovery guidance

---

## Merge Runner

### Purpose

The merge runner handles:
- Polling PR merge status
- Attempting merges when eligible
- Auto-resolving merge conflicts using AI
- Applying queue sync after merge

### Merge Process Flow

```
PR Created ──► Check Merge Status
                     │
        ┌───────────┼───────────┐
        ▼           ▼           ▼
    [Clean]     [Dirty]     [Other]
        │           │           │
        ▼           ▼           ▼
    Merge PR   Try Resolve   Retry Later
        │           │           │
        ▼           ▼           ▼
   On Success  On Success   Exceed Retries
        │           │           │
        ▼           ▼           ▼
   Queue Sync  Merge PR     Skip PR
        │           │
        └─────┬─────┘
              ▼
        Mark Merged
```

### Conflict Resolution

When `conflict_policy: auto_resolve` and a PR has merge conflicts:

1. **Create workspace**: Clone/checkout the PR branch
2. **Attempt merge**: Merge base branch into PR branch
3. **Identify conflicts**: List files with merge conflicts
4. **AI resolution**: Run merge runner with `merge_conflicts` prompt
5. **Validate**: Check all conflicts resolved
6. **Commit**: Commit resolution changes
7. **Push**: Push resolved branch
8. **Retry merge**: Attempt merge again

### Conflict Policy Options

| Policy | Behavior |
|--------|----------|
| `auto_resolve` | Use AI runner to resolve conflicts |
| `retry_later` | Wait and retry merge later |
| `reject` | Skip PR, mark as failed |

### Merge Runner Prompt

The merge runner uses the `merge_conflicts` prompt template:

```
.ralph/prompts/merge_conflicts.md (if exists)
→ Fallback to embedded default
```

---

## Limitations

### No Session Resume

**INTENDED BEHAVIOR**: In-flight tasks should be resumable after interruption.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Parallel mode does not support session resume for individual tasks. If a worker is interrupted, the task will be recorded as finished without PR (if incomplete) and must be re-run manually.

**Workaround**: The state file tracks in-flight tasks, and stale tasks are pruned on restart. You can manually resume by marking the task incomplete and re-running.

### RepoPrompt Forced Off

**INTENDED BEHAVIOR**: RepoPrompt should be available in all modes.

**CURRENTLY IMPLEMENTED BEHAVIOR**: RepoPrompt is forcibly disabled for parallel workers (`repoprompt_plan_required: false`, `repoprompt_tool_injection: false`) to prevent context leakage and keep edits within workspace clones.

**Rationale**: RepoPrompt instructions could cause workers to reference files outside their workspace or perform operations that break isolation.

### Git Commit/Push Required for PR Automation

**INTENDED BEHAVIOR**: PR automation should work independently of git commit/push settings.

**CURRENTLY IMPLEMENTED BEHAVIOR**: If `git_commit_push_enabled: false`, PR automation (`auto_pr`, `auto_merge`, `draft_on_failure`) is automatically disabled because PRs require pushed commits.

### Dependency Handling

Parallel execution respects task dependencies (`depends_on`), but:
- Dependencies must be completed (moved to `done.json`) before dependent tasks are eligible
- Parallel tasks with dependencies on each other will not run concurrently
- Consider using `--wait-when-blocked` for dependency chains

---

## Workflow

### Step-by-Step Parallel Execution Flow

1. **Preflight Checks**
   - Validate queue/done files
   - Check workspace_root is gitignored (if inside repo)
   - Verify `gh` CLI available (if PR automation enabled)
   - Verify origin remote exists
   - Require clean repo

2. **State Initialization**
   - Load existing state or create new
   - Prune stale tasks
   - Reconcile PR records with GitHub
   - Clean workspaces for merged/closed PRs
   - Validate base branch

3. **Worker Spawning Loop**
   - While workers < limit and tasks available:
     - Select next eligible task (respecting dependencies)
     - Create git workspace
     - Sync config/prompts to workspace
     - Spawn worker process
     - Record in state file

4. **Worker Execution**
   - Worker runs `ralph run one --parallel-worker`
   - Changes committed to workspace branch
   - Branch pushed to origin
   - Exit status returned

5. **Post-Worker Processing**
   - On success: Create PR (if auto_pr enabled)
   - On failure: Create draft PR (if draft_on_failure enabled)
   - Update state file
   - Clean up workspace (after merge or on failure without PR)

6. **Merge Runner**
   - Polls PRs for merge eligibility
   - Attempts merges
   - Resolves conflicts (if configured)
   - Applies queue sync after merge

7. **Cleanup**
   - Terminate remaining workers
   - Join merge runner thread
   - Clear state file tasks_in_flight
   - Remove workspaces

### Interrupt Handling

On Ctrl+C or error:

1. Signal merge runner to stop
2. Terminate all in-flight workers
3. Clean up workspaces (best effort)
4. Save state file
5. Exit with appropriate status

---

## Monitoring

### Checking Parallel State

```bash
# View state file
cat .ralph/cache/parallel/state.json | jq

# Quick status check
ralph run loop --parallel --dry-run 2>&1 | head -20
```

### State File Monitoring

Key fields to monitor:

| Field | Indication |
|-------|------------|
| `tasks_in_flight` | Currently running tasks |
| `prs` (lifecycle: open) | Pending PRs awaiting merge |
| `prs` (merge_blocker) | PRs blocked from auto-merge |
| `finished_without_pr` | Tasks needing manual intervention |

### Log Output

Ralph logs parallel execution to console with progress indicators:

```
[INFO] Starting parallel run with 3 workers
[INFO] Spawning worker for RQ-0001 (pid: 12345)
[INFO] Spawning worker for RQ-0002 (pid: 12346)
[INFO] Spawning worker for RQ-0003 (pid: 12347)
[INFO] Worker RQ-0001 completed successfully
[INFO] Created PR #42 for RQ-0001
[INFO] Merged PR #42 for RQ-0001
[INFO] Removed workspace for RQ-0001
...
```

### Reading Worker Logs

Worker output is not captured to separate log files. To debug a specific task:

```bash
# Run the task manually in a workspace
ralph run one --id RQ-0001

# Or check the workspace directly
cd .workspaces/myrepo/parallel/RQ-0001
git log --oneline
git status
```

### Health Checks

```bash
# Verify no stale state
if [ -f .ralph/cache/parallel/state.json ]; then
  echo "Parallel state exists - check if run is active"
  jq '.tasks_in_flight | length' .ralph/cache/parallel/state.json
fi

# Check for workspaces
ls -la .workspaces/*/parallel/ 2>/dev/null || echo "No workspaces"
```

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "workspace_root not gitignored" | Workspace inside repo without ignore | Add to .gitignore |
| "base branch mismatch" | Branch changed during parallel run | Checkout original branch or clear state |
| "PR head mismatch" | Branch prefix changed | Rename branches or clear state blockers |
| "gh CLI check failed" | GitHub CLI not installed/auth | Install `gh` and authenticate |
| "origin remote check failed" | No origin configured | Configure origin remote |

---

## Practical Examples

### Example 1: Basic Parallel Run

```bash
# Start parallel run with 3 workers
ralph run loop --parallel 3

# Output:
# [INFO] Starting parallel run with 3 workers on branch 'main'
# [INFO] Spawning worker for RQ-0001
# [INFO] Spawning worker for RQ-0002
# [INFO] Spawning worker for RQ-0003
# ...
# [INFO] Parallel run completed: 10/10 succeeded, 0 failed
```

### Example 2: Review-Before-Merge Workflow

```bash
# Config: merge_when = "after_all", auto_merge = false
ralph run loop --parallel 4

# All tasks run in parallel
# PRs created as tasks complete
# Run ends with all PRs open for review
# After review, run again or merge manually
```

### Example 3: Handling Failures

```bash
# With draft_on_failure enabled
ralph run loop --parallel 2

# Task RQ-0005 fails
# [WARN] Worker RQ-0005 failed with exit code 1
# [INFO] Created draft PR #47 for RQ-0005

# Review draft PR, fix issues manually
# Then mark task done or re-run
```

### Example 4: Custom Merge Runner

```json
{
  "parallel": {
    "workers": 3,
    "conflict_policy": "auto_resolve",
    "merge_runner": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "reasoning_effort": "high"
    }
  }
}
```

```bash
ralph run loop --parallel

# When conflicts occur, Codex will attempt resolution
# [INFO] PR #50 has merge conflicts, attempting auto-resolution
# [INFO] Resolved conflicts in src/lib.rs, src/main.rs
# [INFO] Merged PR #50
```

### Example 5: Monitoring and Recovery

```bash
# Check if parallel run is active
jq '.tasks_in_flight | length' .ralph/cache/parallel/state.json

# Check for blocked PRs
jq '.prs[] | select(.merge_blocker != null) | {task_id, merge_blocker}' .ralph/cache/parallel/state.json

# Clear specific merge blocker (after fixing issue)
jq '(.prs[] | select(.task_id == "RQ-0001")).merge_blocker = null' .ralph/cache/parallel/state.json > state.tmp.json && mv state.tmp.json .ralph/cache/parallel/state.json
```

---

## See Also

- [Configuration](../configuration.md) - Full configuration reference
- [Workflow](../workflow.md) - General workflow documentation
- [Queue and Tasks](../queue-and-tasks.md) - Task management
