//! Scan prompt loading and rendering.
//!
//! Responsibilities: load the scan prompt template, render user focus and scan mode, and apply
//! project-type guidance where enabled.
//! Not handled: task creation logic, queue mutations, or phase-specific prompt composition.
//! Invariants/assumptions: required placeholders exist and empty user focus normalizes to "(none)".

use super::registry::{load_prompt_template, prompt_template, PromptTemplateId};
use super::util::{
    apply_project_type_guidance_if_needed, ensure_no_unresolved_placeholders,
    ensure_required_placeholders, escape_placeholder_like_text,
};
use crate::cli::scan::ScanMode;
use crate::contracts::{Config, ProjectType};
use anyhow::Result;

/// Mode-specific guidance for maintenance scan mode (default).
const MAINTENANCE_MODE_GUIDANCE: &str = r#"# MISSION
You are a scan-only agent for this repository.
Perform an agentic code review to find bugs, workflow gaps, design flaws, high-leverage reliability and UX fixes, flaky behavior, and safety issues.
Focus on: correctness, security, performance regressions, reliability, repo rule violations, inconsistent or incomplete behavior, and maintainability problems that create real risk.
Prioritize correctness and safety over new features.
You must add a MINIMUM of 7 tasks to the queue.

# CONTEXT (READ IN ORDER)
1. ~/.codex/AGENTS.md
2. AGENTS.md
3. .ralph/README.md
4. .ralph/queue.json

# PROJECT TYPE GUIDANCE
{{PROJECT_TYPE_GUIDANCE}}

# FOCUS
{{USER_FOCUS}}

# HARD CONSTRAINTS
- You must modify .ralph/queue.json only.
- Do not implement fixes in this run. Only create tasks.
- Do not edit existing tasks except:
  - inserting new tasks
  - minimal reordering required by explicit dependencies you introduce (prefer depends_on over reordering)

# SCAN METHOD (DO THIS, IN THIS ORDER)
1. Identify critical paths:
   - main entrypoints, core modules, data handling, auth/secrets, IO boundaries
2. Find likely defect patterns:
   - error handling gaps, unchecked assumptions, inconsistent validation, unsafe filesystem/network usage
3. Check workflow and tooling:
   - Makefile targets, CI scripts, lint/type/test config alignment, dev setup traps
4. Identify flaky or nondeterministic behavior:
   - time, randomness, concurrency, external calls, environment dependent tests
5. Convert findings into outcome-sized tasks suitable for a single worker run.

# MAINTENANCE TASK FILTER
Allowed:
- bugs, security issues, data loss risks
- flaky tests, unreliable behavior, broken workflows
- performance regressions with evidence
- missing validation, unsafe defaults, footguns
Not allowed:
- large refactors without a defect or risk justification
- style only cleanups, drive-by renames, subjective rewrites

# EVIDENCE RULES (DO NOT BE VAGUE)
Do not invent evidence. Every task must include evidence entries in one of these formats:
- "path: <file> :: <symbol or section> :: <what you observed>"
- "workflow: <command or make target> :: <what you observed>"
- "config: <file> :: <key/section> :: <what you observed>"
If you claim a bug, include reproduction details when possible:
- "repro: <command/steps> :: expected <x> :: actual <y>"

# DEDUPE REQUIREMENT
Before adding a new task, search .ralph/queue.json for likely duplicates by:
- similar title keywords
- overlapping scope paths
- matching tags
If a duplicate exists, do not add another.

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
- Note: `ralph queue next` (without `-id`) returns the next queued task, not a new ID.

# TASK SHAPE (REQUIRED KEYS)
Queue schema requires: id, title, created_at, updated_at.
For scan-created tasks, include:
- id (string)
- status: "todo"
- priority: one of "critical" | "high" | "medium" | "low"
- title (string; short, outcome-sized)
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

# OUTPUT
After editing .ralph/queue.json, provide:
- Count of new tasks added
- List of new task IDs + titles (top 10 is fine)
- Whether any tasks were skipped due to dedupe
"#;

/// Mode-specific guidance for innovation scan mode.
const INNOVATION_MODE_GUIDANCE: &str = r#"# MISSION
You are a scan-only agent for this repository.
Perform a feature discovery scan to identify enhancement opportunities, feature gaps, use-case completeness issues, and opportunities for innovative new features.
Focus on: missing features for specific use-cases, user workflow improvements, competitive gaps (only if you can cite concrete evidence), feature completeness, and strategic additions that increase user value.
Prioritize new capabilities and user value over maintenance tasks.
You must add a MINIMUM of 7 tasks to the queue.

# CONTEXT (READ IN ORDER)
1. ~/.codex/AGENTS.md
2. AGENTS.md
3. .ralph/README.md
4. .ralph/queue.json

# PROJECT TYPE GUIDANCE
{{PROJECT_TYPE_GUIDANCE}}

# FOCUS
{{USER_FOCUS}}

# HARD CONSTRAINTS
- You must modify .ralph/queue.json only.
- Do not implement fixes in this run. Only create tasks.
- Do not edit existing tasks except:
  - inserting new tasks
  - minimal reordering required by explicit dependencies you introduce (prefer depends_on over reordering)

# SCAN METHOD (DO THIS, IN THIS ORDER)
1. Map product surfaces:
   - entrypoints (CLI commands, APIs, binaries, services)
   - user facing docs and examples
   - configuration surfaces (env vars, config files)
2. Enumerate the top user workflows the repo appears to support (jobs-to-be-done).
3. For each workflow, identify:
   - missing capability that blocks completion
   - friction that causes needless steps or confusion
   - gaps in examples, templates, or defaults
4. Identify cross-cutting feature opportunities:
   - automation, observability, integrations, extensibility points, safety rails
5. Convert the best findings into outcome-sized tasks suitable for a single worker run.

# INNOVATION TASK FILTER (IMPORTANT)
Allowed:
- New capability, new workflow, new integration, new automation, major UX improvement, extensibility system, safer defaults that unlock adoption.
Allowed maintenance ONLY if tagged as a blocker:
- Add it only when it blocks the feature from being usable or trusted.
Not allowed:
- Pure refactors, style cleanups, minor docs polish, routine dependency bumps, general test coverage increases (unless a blocker).

# EVIDENCE RULES (DO NOT BE VAGUE)
Do not invent evidence. Every task must include evidence entries in one of these formats:
- "path: <file> :: <symbol or section> :: <what you observed>"
- "workflow: <command or make target> :: <what you observed>"
- "config: <file> :: <key/section> :: <what you observed>"
Competitive gaps are allowed ONLY if you can cite concrete evidence. If you use external sources, evidence must be:
- "external: <url> :: accessed <YYYY-MM-DD> :: <what the source shows>"
Keep external evidence minimal and directly tied to a proposed feature.

# DEDUPE REQUIREMENT
Before adding a new task, search .ralph/queue.json for likely duplicates by:
- similar title keywords
- overlapping scope paths
- matching tags
If a duplicate exists, do not add another. Prefer skipping, or create a single rollup task only if it replaces many overlapping items.

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

# TASK SHAPE (REQUIRED KEYS)
Queue schema requires: id, title, created_at, updated_at.
For scan-created tasks, include:
- id (string)
- status: "todo"
- priority: one of "critical" | "high" | "medium" | "low"
- title (string; short, outcome-sized)
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

# OUTPUT
After editing .ralph/queue.json, provide:
- Count of new tasks added
- List of new task IDs + titles (top 10 is fine)
- Whether any tasks were skipped due to dedupe
"#;

pub(crate) fn load_scan_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::Scan)
}

pub(crate) fn render_scan_prompt(
    template: &str,
    user_focus: &str,
    mode: ScanMode,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::Scan);
    ensure_required_placeholders(template, template_meta.required_placeholders)?;

    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };

    // Select mode-specific guidance
    let mode_guidance = match mode {
        ScanMode::Maintenance => MAINTENANCE_MODE_GUIDANCE,
        ScanMode::Innovation => INNOVATION_MODE_GUIDANCE,
    };

    let expanded_template = super::util::expand_variables(template, config)?;
    let injected_mode = expanded_template.replace("{{MODE_GUIDANCE}}", mode_guidance.trim());
    let base = apply_project_type_guidance_if_needed(
        &injected_mode,
        project_type,
        template_meta.project_type_guidance,
    );
    let rendered = base.replace("{{USER_FOCUS}}", focus);
    let safe_focus = escape_placeholder_like_text(focus);
    let rendered_for_validation = base.replace("{{USER_FOCUS}}", safe_focus.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(rendered)
}
