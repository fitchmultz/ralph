# Session Management
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Feature Documentation](README.md)


Ralph's session management system provides crash recovery, explicit resume decisions, and runner-level continue support for long-running agent work.

---

## Overview

### Purpose

Session management serves two related purposes:

1. **Run-session recovery**: detect interrupted `run one` / `run loop` work and decide whether to resume, start fresh, or refuse to guess.
2. **Continue-session recovery**: keep runner sessions alive across CI-fix / revert-and-continue loops when the underlying runner supports reuse.

### Operator-visible decision model

Ralph now narrates resume behavior with one of three states:

| State | Meaning |
|-------|---------|
| `resuming_same_session` | Ralph is continuing the interrupted run or runner session. |
| `falling_back_to_fresh_invocation` | Ralph decided the saved state should not be reused and is starting fresh. |
| `refusing_to_resume` | Ralph cannot safely choose resume vs fresh without operator confirmation. |

Those decisions appear across:
- `ralph run one`
- `ralph run loop`
- `ralph run resume`
- `ralph machine config resolve`
- `ralph machine run ...` event streams
- RalphMac Run Control

### Key features

| Feature | Description |
|---------|-------------|
| **Explicit recovery narration** | Ralph says whether it is resuming, starting fresh, or refusing. |
| **Configurable timeout** | Sessions older than `session_timeout_hours` require explicit confirmation. |
| **Read-only previews** | Machine/app config preview can show resume state without mutating cache. |
| **Per-phase runner isolation** | Continue sessions stay phase-scoped, including deterministic Kimi session IDs. |
| **Atomic persistence** | Session state is written atomically to prevent corruption. |

---

## Session State File

Session state is persisted to:

```text
.ralph/cache/session.jsonc
```

This file is created when a task starts and is normally cleared when the run completes successfully or when Ralph explicitly abandons an invalid saved session during execution.

### Structure

```json
{
  "version": 1,
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "task_id": "RQ-0001",
  "run_started_at": "2026-02-07T10:00:00.000000000Z",
  "last_updated_at": "2026-02-07T10:30:00.000000000Z",
  "iterations_planned": 2,
  "iterations_completed": 1,
  "current_phase": 2,
  "runner": "claude",
  "model": "sonnet",
  "tasks_completed_in_loop": 0,
  "max_tasks": 10,
  "git_head_commit": "abc123def456",
  "phase1_settings": {
    "runner": "claude",
    "model": "sonnet",
    "reasoning_effort": null
  },
  "phase2_settings": {
    "runner": "codex",
    "model": "gpt-5.4",
    "reasoning_effort": "high"
  },
  "phase3_settings": {
    "runner": "claude",
    "model": "haiku",
    "reasoning_effort": null
  }
}
```

### Field notes

- `task_id`, `current_phase`, and `tasks_completed_in_loop` drive crash-recovery routing.
- `phase*_settings` are display-only; Ralph recomputes effective settings from config + task + CLI overrides.
- `git_head_commit` is advisory context, not a hard resume gate.

---

## Run-session recovery flow

### Validation outcomes

When Ralph starts a run, it classifies saved session state into one of these buckets:

| Validation result | Meaning |
|------------------|---------|
| `NoSession` | No interrupted run exists. |
| `Valid(session)` | Session still targets a live runnable task. |
| `Stale` | Task disappeared or entered a terminal / incompatible state. |
| `Timeout` | Session is older than the configured safety threshold. |

### Decision rules

#### `ralph run resume`
- Behaves like `run loop --resume`.
- Valid sessions resume immediately.
- Timed-out sessions still require confirmation.
- If no saved session exists, Ralph explicitly says it is starting fresh.

#### `ralph run one`
- Always inspects interrupted-session state first.
- `--resume` auto-resumes when safe.
- Without `--resume`, Ralph prompts when confirmation is required and available.
- If you explicitly pass `--id <TASK_ID>`, that selection overrides an unrelated interrupted session and Ralph says so.

#### `ralph run loop`
- Supports the same session decision model as `run one`.
- A resumed task is used only for the first loop iteration, then normal queue selection resumes.

### Timeout behavior

Sessions older than `session_timeout_hours` are not auto-resumed just because `--resume` is present.
Timed-out sessions require an explicit operator confirmation unless Ralph is in a non-interactive context, in which case it refuses instead of guessing.

### Non-interactive behavior

If a saved session requires a decision and Ralph cannot ask safely:

| Situation | Result |
|----------|--------|
| Valid session + no `--resume` | `refusing_to_resume` |
| Timed-out session | `refusing_to_resume` |
| No saved session | start fresh |
| Stale session | start fresh |

This prevents headless automation from silently discarding or duplicating interrupted work.

---

## Machine + app surfaces

### Config preview

`ralph machine config resolve` now includes an optional `resume_preview` payload:

```json
{
  "version": 3,
  "paths": { "repo_root": "/repo", "queue_path": "/repo/.ralph/queue.jsonc", "done_path": "/repo/.ralph/done.jsonc" },
  "safety": { "repo_trusted": true, "dirty_repo": false, "git_publish_mode": "off", "ci_gate_enabled": true, "git_revert_mode": "ask", "parallel_configured": false, "execution_interactivity": "noninteractive_streaming", "interactive_approval_supported": false },
  "config": { "agent": { "model": "gpt-5.4" } },
  "resume_preview": {
    "status": "refusing_to_resume",
    "scope": "run_session",
    "reason": "session_timed_out_requires_confirmation",
    "task_id": "RQ-0001",
    "message": "Resume: refusing to continue timed-out session RQ-0001 without explicit confirmation.",
    "detail": "The saved session is 48 hour(s) old, exceeding the configured 24-hour safety threshold."
  }
}
```

This preview is **read-only**: it must not clear or rewrite saved session state.

### Machine run events

`ralph machine run ...` streams can emit:

```json
{"version":2,"kind":"resume_decision","task_id":"RQ-0001","message":"Resume: continuing the interrupted session for task RQ-0001.","payload":{"status":"resuming_same_session","scope":"run_session","reason":"session_valid","task_id":"RQ-0001","message":"Resume: continuing the interrupted session for task RQ-0001.","detail":"Saved session is current and will resume from phase 2 with 1 completed loop task(s)."}}
```

RalphMac consumes both `resume_preview` and `resume_decision` so Run Control can show the expected action before the run starts and the actual action once the run begins.

---

## Continue-session recovery

Run-session recovery decides whether Ralph resumes a task. Continue-session recovery decides whether Ralph can reuse the **runner's** own session during CI-fix / supervision loops.

### Continue behavior

- Ralph prefers same-session reuse when a runner session identifier exists.
- If the session identifier is missing or known-invalid, Ralph says it is falling back to a fresh invocation.
- Unknown resume failures still hard-fail.

### Known safe fallback cases

| Runner | Safe fresh fallback cases |
|--------|---------------------------|
| Pi | missing session file / lookup failures |
| Gemini | invalid session identifier resume failures |
| Claude | invalid `--resume` / invalid UUID failures |
| OpenCode | session validation failures, including semantic zero-exit failures |

---

## Kimi per-phase session IDs

For runners that support explicit session IDs (notably **Kimi**), Ralph uses deterministic per-phase identifiers:

```text
{task_id}-p{phase}-{timestamp}
```

Example:

```text
RQ-0001-p2-1704153600
```

This keeps planning / implementation / review recovery isolated from each other.

---

## Configuration

Configure timeout behavior in `.ralph/config.jsonc`:

```json
{
  "agent": {
    "session_timeout_hours": 24
  }
}
```

Guidance:
- daily development: `24`
- long weekend work: `72`
- extended analysis: `168`
- CI/headless automation: keep low and pair with an explicit `--resume` policy

---

## Best practices

### Interactive use
- Review the resume message before continuing old work.
- Use `ralph run one --resume` or `ralph run resume` when you want an explicit auto-continue path.
- Treat `refusing_to_resume` as a prompt to choose deliberately, not as an error to suppress blindly.

### Automation / app integrations
- Read `resume_preview` for preflight UI.
- Consume `resume_decision` run events for live state.
- Do not infer resume behavior from plain text when machine payloads exist.

### CI / headless usage
- Prefer an explicit policy:

```bash
# Explicitly continue when safe
ralph run loop --resume --non-interactive

# Or require fresh orchestration with no recovery
ralph run loop --non-interactive
```

---

## Troubleshooting

### Ralph started fresh instead of resuming
Common causes:
- the task no longer exists
- the task is already terminal (`done`, `rejected`)
- you explicitly selected a different task
- the saved session was stale

### Ralph refused to resume
Common causes:
- non-interactive mode prevented a required confirmation
- the saved session timed out and needed operator approval

### Runner continue fell back to fresh
Common causes:
- the runner rejected the saved session id
- no runner session id was available to reuse

---

## See also

- [Workflow](../workflow.md)
- [Configuration](../configuration.md)
- [Phases](./phases.md)
- [Runners](./runners.md)
- [Machine Contract](../machine-contract.md)
