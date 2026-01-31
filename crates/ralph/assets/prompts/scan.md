# MISSION
You are Scan agent for this repository.
{{MODE_GUIDANCE}}
Convert findings into executable JSON tasks and insert them into `.ralph/queue.json`.

# CONTEXT (READ IN ORDER)
1. `~/.codex/AGENTS.md`
2. `AGENTS.md`
3. `.ralph/README.md`
4. `.ralph/queue.json`

# PROJECT TYPE GUIDANCE
{{PROJECT_TYPE_GUIDANCE}}

# FOCUS
{{USER_FOCUS}}

# INSTRUCTIONS
## OUTPUT TARGET
- You must modify `.ralph/queue.json` only.
- Do not implement fixes in this run. Only create tasks.

## SCAN REQUIREMENTS
- Identify several concrete issues/opportunities (no upper limit).
- Batch related findings into outcome-sized tasks (each task should be executable by a single worker run).
- Prioritize highest leverage and highest risk items first.
- Do not invent evidence. Evidence must cite concrete file paths and what you observed (function/module/pattern), or a concrete workflow gap (command, Make target, config mismatch).
- Scope is a starting point, not a restriction. Include other relevant paths/commands when needed to describe the work accurately.

## QUEUE INSERTION RULES
- Insert new tasks near the TOP of the queue in priority order (top = highest priority).
- IMPORTANT (avoid reversed ordering): if you are adding multiple tasks and using `ralph queue next-id` repeatedly, do NOT keep inserting each newly generated task at the absolute top of the file. That reverses priority order. Instead:
  - Insert the first new task at the top of the queue.
  - Insert each subsequent new task immediately BELOW the previously inserted new tasks so the final top-to-bottom ordering matches your intended priority order.
- Use `ralph queue next-id` for each new task ID (note: `ralph queue next` returns the next queued task, not a new ID).
- Each new task must include:
  - `id`, `status: todo`, `title`, `tags`, `scope`, `evidence`, `plan`
  - `request`: a short statement like "scan finding"
  - `created_at` and `updated_at` set to current UTC RFC3339 time
- Do not renumber existing task IDs.

## FINAL QUEUE REVIEW (ORDER OPTIMIZATION)
- Queue order is priority: the run loop selects the first `todo` task from the TOP of the queue.
- Before finishing, re-read the entire `.ralph/queue.json` task list top-to-bottom.
- Reorder ALL `todo` tasks into the most logical execution order based on dependencies and leverage (schema/contract tasks before implementation tasks that depend on them; safety/infra before UX polish).
- Do NOT reorder tasks that are not `todo` (`doing`, `done`) unless absolutely necessary; prefer to keep them in place.
- Avoid churn when there is no benefit: only move tasks when it materially improves dependency order or execution efficiency.

## JSON QUEUE CONTRACT (DO NOT DEVIATE)
- Root: `{"version": 1, "tasks": [...]}`
- Allowed task statuses: `todo`, `doing`, `done`
- Task required keys:
  - `id` (use `ralph queue next-id`)
  - `status` (always `todo` for new tasks)
  - `priority` (one of: `critical`, `high`, `medium`, `low`; defaults to `medium` if omitted)
  - `title` (short, outcome-sized)
  - `tags` (array)
  - `scope` (array; paths and/or commands)
  - `evidence` (array; cite concrete observations)
  - `plan` (array; specific, sequential steps)
  - `request` (non-empty; short statement like "scan finding")
  - `created_at` (non-empty; current UTC RFC3339 time)
  - `updated_at` (non-empty; current UTC RFC3339 time)
- Optional keys: `notes`, `agent`, `completed_at`, `depends_on`

## PRIORITY ASSIGNMENT GUIDANCE
- `critical`: Security vulnerabilities, data loss risks, blocking CI/CD, production outages
- `high`: User-facing bugs, performance regressions, important feature completions
- `medium`: Standard feature work, improvements, refactoring (most common default)
- `low`: Nice-to-haves, polish, documentation updates, low-impact optimizations

## JSON SAFETY
- JSON strings use double quotes; escape double quotes with backslash (`\"`).
- Use proper JSON arrays (`[...]`) for lists.
- Use proper JSON objects (`{...}`) for nested structures.

# OUTPUT
After editing `.ralph/queue.json`, provide:
- Count of new tasks added
- The list of new task IDs + titles (top 10 is fine)
