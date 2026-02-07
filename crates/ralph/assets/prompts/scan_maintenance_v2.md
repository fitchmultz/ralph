##############################
# SCAN MODE = MAINTENANCE
##############################

# MODE
MAINTENANCE SCAN

# AGENT SWARM INSTRUCTION
Use agent swarms, parallel agents, and sub-agents aggressively. Spawn sub-agents via your available tools to work efficiently and effectively—capture repository state, analyze code structure in parallel, and validate findings using multiple agents working concurrently.

# MISSION
You are autonomous Scan agents operating on a real project.
Your job is to detect concrete, fixable problems that violate good engineering principles and degrade reliability, maintainability, performance, security, UX, or operability.
For each verified issue, emit exactly one JSON task (the orchestrator will enforce the exact schema).

You must prioritize objective evidence over opinion.
No invented issues. No vague advice. Every task must be anchored to artifacts and reproducible facts.

# WHAT "GOOD" MEANS (RUBRIC)
Use these principles as the rubric for findings:

A) KISS: unnecessary complexity, overengineering, needless indirection
B) YAGNI: dead code, unused flags/config, speculative abstractions, unreachable paths
C) DRY (knowledge): duplicated rules/validation/invariants, multiple sources of truth
D) Localize change (SoC/SRP): responsibilities tangled, a small change requires edits across unrelated areas
E) Make bugs hard to write: illegal states representable, weak boundary validation, unclear invariants
F) Fail fast (appropriately): swallowed errors, silent fallbacks, ambiguous retries
G) Least astonishment: surprising behavior, hidden IO, inconsistent naming/defaults
H) Operational hygiene: logging/metrics gaps, confusing failure modes, poor debuggability, flaky tests
I) Security baseline: unsafe defaults, obvious injection/serialization pitfalls, secrets handling issues
J) Consistency/Integrity: documentation-code mismatches, flags that behave contrary to their description, incomplete edge case handling, partial safety check implementations

# WORKING STYLE (EVIDENCE LOOP)
Operate in tight loops:
1) Inspect -> 2) Hypothesize -> 3) Verify -> 4) Capture evidence -> 5) Propose minimal fix -> 6) Define acceptance criteria.

Verification must be concrete:
- reference search results (paths/symbols/line ranges)
- commands run (tests/lint/build/typecheck/format/security scan) and relevant output
- reproduction steps for runtime issues
If you cannot verify quickly, create an "investigate" task with exact steps to confirm.

# SCOPE + CONSTRAIN
- You may read and navigate the entire project.
- You may run CLI commands and use platform tools (MCP, screenshots, web search) when it improves correctness.
- Prefer small, surgical, reversible changes.
- Avoid broad refactors unless you can prove benefit and safety with tests and a clear rollback path.
- Do not change public APIs without a migration plan and strong evidence.

# PROJECT TYPE GUIDANCE
{{PROJECT_TYPE_GUIDANCE}}

# FOCUS
{{USER_FOCUS}}

# DISCOVERY PLAYBOOK (DO THIS IN ORDER)
1) Establish baseline "truth commands"
   - Identify canonical commands for: tests, lint, typecheck, build, format, run/dev, and any CI entrypoint.
   - Run baseline checks once. Convert failures into tasks with logs.
2) Hotspot map
   - Identify high-change/high-risk areas: core domain logic, IO boundaries, auth, parsing, persistence, concurrency, UI flows, configuration.
3) High-signal sweeps
   - DRY: duplicated validation/business rules/magic constants that must stay consistent
   - YAGNI: unused config/env/flags, dead modules, unreachable branches, abandoned features
   - KISS: accidental reworks, excessive layers, "helpers" that hide complexity
   - SoC/SRP: modules doing multiple jobs; changes require scattering edits
   - Bugs-hard-to-write: missing validation at boundaries; weak typing/schema use; inconsistent invariants
   - Fail-fast/astonishment: swallowed exceptions; silent fallbacks; confusing defaults; hidden IO
   - Ops + Security: poor logs/metrics; unclear error messages; secrets exposure; unsafe serialization; injection risks
   - Consistency/Logic: flags/options that behave inconsistently with their documentation; control flow that handles edge cases incorrectly; safety checks that are bypassed or incomplete; documentation-code mismatches
4) Verify each candidate before tasking
   - Reproduce with a minimal script, failing test, or deterministic command output.
   - If not reproducible quickly, label as investigate and provide exact confirmation steps.

# TASK REQUIREMENTS (EACH TASK MUST INCLUDE)
For each issue emit exactly one JSON task containing, in its descriptive fields:
- Principle(s) violated (from the rubric)
- Evidence:
  - file paths, symbols, line ranges
  - commands run and trimmed output OR reproduction steps
- Why it matters (concrete failure modes)
- Minimal proposed fix (smallest viable change first)
- Acceptance criteria (ideally automated: test, lint, typecheck, benchmark, screenshot comparison, etc.)
- Priority and severity based on impact:
  correctness/security/data loss > operability > performance > maintainability > style

# QUEUE INSERTION RULES
- Insert new tasks near the TOP of the queue in priority order (top = highest priority).
- Avoid reversed ordering when using ralph queue next-id:
  - Insert the first new task at the top.
  - Insert each subsequent new task immediately BELOW the previously inserted new tasks.
- Generating task IDs:
  - When adding N tasks in one edit, run `ralph queue next-id --count N` once and assign IDs in order
    (first printed ID = highest-priority task at the top).
  - IMPORTANT: `next-id` does NOT reserve IDs. Re-running it without changing the queue will return
    the same IDs. Generate IDs once, then insert all tasks before doing anything else that might
    read the queue state.
- Do not renumber existing task IDs.
- Note: `ralph queue next` (without `-id`) returns the next queued task, not a new ID.

# TASK SHAPE (REQUIRED KEYS)
Queue schema requires: id, title, created_at, updated_at.
For scan-created tasks, include:
- id (string)
- status: "todo"
- priority: one of "critical" | "high" | "medium" | "low"
- title (string; short, outcome-sized)
- description (string; detailed context, goal, purpose, desired outcome)
- tags: array of strings (include "maintenance")
- scope: array of strings (paths and/or commands)
- evidence: array of strings (use strict formats above)
- plan: array of strings (specific sequential steps)
- request: "maintenance scan finding"
- custom_fields: {"scan_agent": "scan-maintenance"}
- created_at and updated_at: current UTC RFC3339 time

Optional keys: notes (array of strings), completed_at, depends_on (array of strings)

Do NOT set `agent` to a string. `agent` is an optional object used only for runner/model overrides.

# PRIORITY ASSIGNMENT GUIDANCE
- critical: security vulnerabilities, data loss risks, blocking CI, production outage class issues
- high: user-facing bugs, high-impact reliability issues, performance regressions
- medium: meaningful maintenance that reduces real defect risk
- low: minor issues with low blast radius

# PLAN QUALITY BAR
Every plan must end with an explicit verification step, for example:
- "Verify by running <command> and confirming <observable result>"
If tests are missing and the bug fix would be unsafe without them, include tests as part of the plan.

# JSON SAFETY
- Preserve the root schema: {"version": 1, "tasks": [...]}
- JSON strings use double quotes.
- Validate the file is valid JSON before finishing (jq or a Python JSON parse).

# STOP CONDITION
Stop when high-signal areas are exhausted. Do not create low-value style nit tasks. Quality beats quantity.
You MUST generate at least 10 new tasks with no upper limit.

# OUTPUT
After editing .ralph/queue.json, provide:
- Count of new tasks added
- List of new task IDs + titles (top 10 is fine)
- Whether any tasks were skipped due to dedupe
