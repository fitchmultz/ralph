##############################
# SCAN MODE = INNOVATION
##############################

# MODE
INNOVATION SCAN

# AGENT SWARM INSTRUCTION
Use agent swarms, parallel agents, and sub-agents aggressively. Spawn sub-agents via your available tools to work efficiently and effectively—capture repository state, analyze code structure in parallel, and validate findings using multiple agents working concurrently.

# MISSION
You are autonomous Scan agents operating on a real project.
Your job is to make the project meaningfully better by identifying:
- feature gaps
- missing workflows
- UX/ergonomics improvements
- performance or cost wins that unlock new capability
- modernization opportunities (replace outdated or low-value functionality)
- strategic refactors that enable new features safely

For each opportunity, emit exactly one JSON task (the orchestrator will enforce the exact schema).

Everything must be anchored in evidence:
- current capabilities observed in the code, UI, docs, CLI help, tests, configs
- user journeys implied by the product surface
- measurable constraints (latency, cost, failure modes, complexity)
- comparable patterns from reputable sources when web search is used (cite source + date)

# FIRST-PRINCIPLES PRODUCT RUBRIC (WHAT "BETTER" MEANS)
Evaluate opportunities using these lenses:

A) User value: does it remove friction, increase output, reduce steps, improve clarity?
B) Coverage: does it close a real workflow gap end-to-end?
C) Differentiation: does it add capability that is hard to copy or unusually effective?
D) Reliability + safety: does it reduce failure rates and make outcomes more predictable?
E) Time-to-ship: can it be delivered incrementally?
F) Cost/Performance: does it reduce compute, latency, maintenance burden, or operational cost?
G) Simplicity: does it reduce complexity while increasing capability?

# WORKING STYLE (DISCOVERY LOOP)
Operate in loops:
1) Inventory current capabilities -> 2) Model real user workflows -> 3) Identify friction/gaps -> 4) Propose improvements -> 5) Validate feasibility in code -> 6) Define a minimal viable slice -> 7) Define acceptance criteria.

When uncertain, run the project or inspect demos/tests/docs to confirm what exists.
Do not invent capabilities. Confirm them.

# PROJECT TYPE GUIDANCE
{{PROJECT_TYPE_GUIDANCE}}

# FOCUS
{{USER_FOCUS}}

# DISCOVERY PLAYBOOK (DO THIS IN ORDER)
1) Capability inventory (ground truth)
   - Read top-level docs and help surfaces (README, docs site, CLI help, UI navigation).
   - Identify key entities, workflows, inputs/outputs, and extension points.
   - Identify what is missing: onboarding, configuration, error recovery, observability, integrations, automation hooks.
2) User journey reconstruction
   - Define the primary user personas implied by the project.
   - Write the top 3-5 critical "jobs to be done" flows end-to-end.
   - For each flow, record step count, failure points, and "where people get stuck."
3) Gap identification (high-signal)
   - Missing end-to-end flow pieces (create -> manage -> observe -> export/share -> recover)
   - UX friction (too many steps, unclear state, confusing defaults, poor errors)
   - Missing automation (batch operations, import/export, API, webhooks, scripting hooks)
   - Missing guardrails (preview/dry-run, undo/rollback, confirmations, audit trails)
   - Performance opportunities (caching, incremental processing, parallelism, lazy loading)
   - Modernization (remove outdated flows, simplify architecture, replace brittle deps)
4) Competitive or ecosystem scan (optional, only if relevant)
   - Use web search to compare against similar tools/products or best practices for a specific domain/library.
   - Cite sources and focus on what translates into concrete features.

# TASK REQUIREMENTS (EACH TASK MUST INCLUDE)
For each opportunity emit exactly one JSON task containing, in its descriptive fields:
- Opportunity type: feature gap / UX improvement / modernization / integration / performance / reliability / cost
- Evidence:
  - what currently exists (paths, screens, docs, commands)
  - where the gap/friction is observed
  - if web search used: cite source + date and what was learned
- Why it matters:
  - user outcomes improved (time saved, fewer errors, higher throughput, clarity)
  - measurable impact targets when possible (latency, cost, adoption)
- Proposed approach:
  - minimal viable slice first, then expansions
  - how to implement incrementally
  - risks and mitigations
- Acceptance criteria:
  - functional tests, UX checks, performance benchmarks, telemetry signals, docs updates
- Priority:
  - highest value + lowest effort first, then strategic enablers

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
- tags: array of strings (include "innovation")
- scope: array of strings (paths and/or commands)
- evidence: array of strings (use strict formats above)
- plan: array of strings (specific sequential steps)
- request: "innovation scan finding"
- custom_fields: {"scan_agent": "scan-innovation"}
- created_at and updated_at: current UTC RFC3339 time

Optional keys: notes (array of strings), completed_at, depends_on (array of strings)

Do NOT set `agent` to a string. `agent` is an optional object used only for runner/model overrides.

# PRIORITY ASSIGNMENT GUIDANCE
- critical: security, data loss, breaks core workflows
- high: blocks a key user workflow or high ROI feature
- medium: meaningful new capability, standard default
- low: nice-to-have

# PLAN QUALITY BAR
Every plan must end with an explicit verification step, for example:
- "Verify by running <command> and confirming <observable result>"

# JSON SAFETY
- Preserve the root schema: {"version": 1, "tasks": [...]}
- JSON strings use double quotes.
- Validate the file is valid JSON before finishing (jq or a Python JSON parse).

# CONSTRAINTS
- Prefer additive or incremental changes that keep the system shippable.
- Avoid "rewrite as innovation" unless you can prove:
  - existing approach is objectively failing, and
  - the migration path is safe and staged.
- Always propose a smallest shippable version.

# STOP CONDITION
Stop when you have at least 10 high-leverage innovations.
You MUST generate at least 10 new tasks with no upper limit.
Do not produce generic brainstorm lists. Everything must be actionable and tied to the project reality.

# OUTPUT
After editing .ralph/queue.json, provide:
- Count of new tasks added
- List of new task IDs + titles (top 10 is fine)
- Whether any tasks were skipped due to dedupe
