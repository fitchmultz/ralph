# Machine Contract
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


Ralph exposes a first-class machine API under `ralph machine ...`.

This surface exists for the macOS app and any other automation that needs stable, versioned JSON instead of human-oriented CLI behavior.

## Rules

- Every machine response is a named JSON document with a top-level `version`.
- Breaking wire changes require a version bump for the affected machine document.
- Human CLI output and flags may change without preserving app compatibility.
- Machine run streams emit NDJSON on stdout.
- Machine run terminal summaries are single-line JSON documents so stream consumers can parse them deterministically.
- Machine clients should consume structured resume payloads instead of scraping prose from stderr/stdout.
- Machine clients must decide compatibility fallback paths from structured capability/version signals (for example `machine cli-spec` or versioned machine-error metadata), not from human stderr phrases like `usage:` or `unknown command`.

## Current Machine Areas

- `ralph machine system info`
- `ralph machine queue read`
- `ralph machine queue graph`
- `ralph machine queue dashboard`
- `ralph machine queue validate`
- `ralph machine queue repair`
- `ralph machine queue undo`
- `ralph machine queue unlock-inspect`
- `ralph machine config resolve`
- `ralph machine workspace overview`
- `ralph machine task create`
- `ralph machine task mutate`
- `ralph machine task decompose`
- `ralph machine run one`
- `ralph machine run loop`
- `ralph machine run stop`
- `ralph machine run parallel-status`
- `ralph machine doctor report`
- `ralph machine cli-spec`
- `ralph machine schema`

## Important versioned documents

### machine command failures (`machine_error`, `version: 1`)

When any `ralph machine ...` command fails before it can emit its success document, stderr now carries a structured JSON error document:

- `version`
- `code`
- `message`
- optional `detail`
- `retryable`

Machine clients should decode that document instead of scraping English stderr text.

`ralph machine run loop` accepts the same `--parallel <N>` worker override pattern as the human `ralph run loop` surface, including bare `--parallel` defaulting to `2`.


### `machine config resolve` (`version: 4`)

Includes:
- resolved queue/config paths
- safety summary
- resolved config
- `execution_controls` for native runner, reasoning-effort, and parallel-worker affordances
- optional `resume_preview`

RalphMac and other machine clients should treat these resolved paths as the canonical queue, done, and config file locations for the workspace instead of assuming the default `.ralph/...` paths.

`execution_controls` is the canonical native-control catalog for RalphMac. It includes:
- runner options with stable ids, display names, source (`built_in`, `global_plugin`, `project_plugin`), reasoning-effort support, model-affordance metadata, and optional default model
- reasoning-effort options
- the numeric `parallel_workers` contract (`min`, `max`, `default_missing_value`)

`resume_preview` is the app/automation preflight signal for whether the next run would:
- resume the same session
- fall back to a fresh invocation
- refuse to resume

`resume_preview` is optional by design. When the active queue file is unavailable,
`machine config resolve` still succeeds for config/path introspection and omits
`resume_preview` (or emits `null`) instead of failing the command.

### `machine workspace overview` (`version: 1`)

Returns a single document that embeds the same payloads as `machine queue read` and `machine config resolve` under `queue` and `config` respectively, so clients can refresh both in one subprocess round-trip.

Because this command embeds `machine queue read`, queue availability is still
required; missing active queue files remain a structured failure on this
surface.

The embedded `queue.paths` and `config.paths` payloads are the canonical first-load source for app watcher targets, queue access checks, and workspace diagnostics.

Client fallback behavior for older CLIs is capability-driven: attempt `machine workspace overview` first, then only fall back to legacy `machine queue read` + `machine config resolve` when a structured capability probe (`machine cli-spec` command tree) explicitly omits `machine workspace overview`.

### `machine run` events (`version: 3`)

The NDJSON stream can emit both resume and progress-blocking state transitions.

Resume decisions remain structured:

```json
{
  "version": 3,
  "kind": "resume_decision",
  "timestamp": "2026-04-26T06:00:00Z",
  "task_id": "RQ-0001",
  "message": "Resume: continuing the interrupted session for task RQ-0001.",
  "payload": {
    "status": "resuming_same_session",
    "scope": "run_session",
    "reason": "session_valid",
    "task_id": "RQ-0001",
    "message": "Resume: continuing the interrupted session for task RQ-0001.",
    "detail": "Saved session is current and will resume from phase 2 with 1 completed loop task(s)."
  }
}
```

Every `machine run` event envelope includes `timestamp`. Machine clients should
validate the full envelope shape instead of assuming older timestamp-less event
examples remain valid.

Blocking-state transitions are also structured:

```json
{
  "version": 3,
  "kind": "blocked_state_changed",
  "timestamp": "2026-04-26T06:00:02Z",
  "message": "Ralph is blocked by unfinished dependencies.",
  "payload": {
    "status": "blocked",
    "reason": {
      "kind": "dependency_blocked",
      "blocked_tasks": 2
    },
    "task_id": null,
    "message": "Ralph is blocked by unfinished dependencies.",
    "detail": "2 candidate task(s) are waiting on dependency completion."
  }
}
```

`kind: "blocked_state_cleared"` indicates that Ralph resumed forward progress.

### `machine run` summaries (`version: 2`)

Terminal summaries include:
- `version`
- optional `task_id`
- `exit_code`
- `outcome`
- optional `blocking`

Startup failures and in-stream failures are intentionally classified differently:
- If `ralph machine run one` or `ralph machine run loop` fails before `run_started` is emitted, the command exits non-zero and stderr carries `machine_error`; stdout does not begin a machine run stream.
- Once `run_started` has been emitted, the authoritative terminal run state must arrive as the final stdout summary document, even if the process later exits non-zero and stderr also carries `machine_error`.

`ralph machine run one` and `ralph machine run loop` share the same summary document version, but loop runs may legitimately end in non-completed operator states. Current loop outcomes are:
- `completed`
- `no_candidates`
- `blocked`
- `stalled`
- `stopped`
- `failed`

When present, `blocking` is the canonical operator-state payload. App and automation clients should preserve it as the source of truth instead of inferring queue idle/blocked state from `outcome` strings alone.

Example loop summary for an idle queue:

```json
{
  "version": 2,
  "task_id": null,
  "exit_code": 0,
  "outcome": "no_candidates",
  "blocking": {
    "status": "waiting",
    "reason": {
      "kind": "idle",
      "include_draft": false
    },
    "task_id": null,
    "message": "Ralph is idle: no todo tasks are available.",
    "detail": "The queue currently has no runnable todo candidates; Ralph is waiting for new work."
  }
}
```

### `machine run stop` (`version: 1`)

Stop After Current is a machine-contract workflow under `ralph machine run stop`, not a human `ralph queue stop` parsing path.

The scenario-level app parity registry in `crates/ralph/src/cli/app_parity.rs`
should reference this contract through explicit Rust and RalphMac proof anchors
whenever Stop After Current behavior changes.

The success document includes:
- `dry_run`
- `action` (`would_create`, `created`, `already_present`)
- resolved `paths`
- `marker` with stop-marker path and before/after state
- optional top-level `blocking`
- `continuation`

App and automation clients should decode this document on stdout for successful stop requests and continue using stderr `machine_error` for pre-success failures.

### `machine queue read`

`runnability.summary.blocking` is the queue/read-side source of truth for why the queue is idle, dependency-blocked, schedule-blocked, or mixed.

### `machine queue validate` (`version: 1`)

Queue validation is now a continuation-oriented document instead of a bare validity boolean. It always includes:
- `valid`
- optional top-level `blocking`
- `warnings`
- `continuation` with a headline, detail, optional blocking payload, and explicit next-step commands.

When the queue is structurally valid but not immediately runnable, `blocking` may still be populated from queue runnability so app and automation surfaces can explain whether Ralph is waiting or blocked.

### `machine queue repair` (`version: 1`)

Queue repair returns a continuation document for both preview and apply modes:
- `dry_run`
- `changed`
- optional top-level `blocking`
- opaque `report`
- `continuation`

When present, the document-level `blocking` mirrors `continuation.blocking` so app and automation clients can consume a single canonical field.

Preview mode narrates whether recoverable fixes are available; apply mode confirms normalization and points to validation/undo follow-up steps.

### `machine queue undo` (`version: 1`)

Queue undo returns a continuation document for list, preview, and restore flows:
- `dry_run`
- `restored`
- optional top-level `blocking`
- optional `result`
- `continuation`

When present, the document-level `blocking` mirrors `continuation.blocking` so app and automation clients can consume a single canonical field.

This is the machine-safe counterpart to `ralph undo`, which now treats checkpoints as a normal continuation workflow rather than an emergency command.

### `machine queue unlock-inspect` (`version: 1`)

Queue-lock inspection returns a structured document for app and automation consumers:
- `condition` (`clear`, `live`, `stale`, `owner_missing`, `owner_unreadable`)
- optional top-level `blocking`
- `unlock_allowed`
- `continuation`

This is the machine-safe counterpart to `ralph queue unlock --dry-run`; app integrations should use this document instead of parsing human CLI prose.

### `machine task mutate` (`version: 2`) and `machine task decompose` (`version: 2`)

Task mutation and decomposition documents now include:
- optional top-level `blocking`
- a shared `continuation` object with `headline`, `detail`, optional `blocking`, and `next_steps`

When present, the document-level `blocking` mirrors `continuation.blocking` so app and automation surfaces can consume a single canonical field after preview, write, and write-blocked flows.

### `machine run parallel-status` (`version: 3`)

Parallel status now returns a continuation-oriented document instead of a raw state blob alone:
- optional top-level `blocking`
- `lifecycle_counts` with per-lifecycle worker totals (required; see `MachineParallelLifecycleCounts` in `schemas/machine.schema.json`)
- `continuation` with a headline, detail, optional blocking payload, and explicit next-step commands
- raw `status` payload with the persisted worker snapshot

When present, the document-level `blocking` mirrors `continuation.blocking` so automation can consume the canonical operator-state field directly while still inspecting worker details from `status`.

Scenario-level app parity entries should treat these continuation `next_steps`
as the canonical input for native action mapping and copy-only fallbacks.

### `machine doctor report` (`version: 2`)

Doctor reports now include a typed top-level `blocking` field so app and automation clients can consume the canonical operator-facing blocking model without decoding the untyped `report` payload first.

```json
{
  "version": 2,
  "blocking": {
    "status": "stalled",
    "reason": {
      "kind": "runner_recovery",
      "scope": "runner",
      "reason": "runner_binary_missing"
    },
    "task_id": null,
    "message": "Ralph is stalled because runner binary 'codex' is unavailable.",
    "detail": "Configured/default runner Codex cannot execute because 'codex' is not on PATH or not executable."
  },
  "report": {
    "success": false,
    "blocking": {
      "status": "stalled",
      "reason": {
        "kind": "runner_recovery",
        "scope": "runner",
        "reason": "runner_binary_missing"
      },
      "task_id": null,
      "message": "Ralph is stalled because runner binary 'codex' is unavailable.",
      "detail": "Configured/default runner Codex cannot execute because 'codex' is not on PATH or not executable."
    },
    "checks": []
  }
}
```

`blocking` is the doctor-side counterpart to run-event `blocked_state_changed`, run-summary `blocking`, and queue/read `runnability.summary.blocking`.

Together, those payloads are the source of truth for live operator-state UI.

## Schemas

Generated machine schemas live in [schemas/machine.schema.json](../schemas/machine.schema.json).

Generate them locally with:

```bash
make generate
```

## App Contract Boundary

The macOS app should consume only machine surfaces for:

- queue snapshots
- config resolution
- combined queue + config overview (`machine workspace overview`)
- task create/mutate/decompose flows
- graph and dashboard reads
- diagnostics consumed by the app
- run status and event streaming
- CLI spec loading
- resume preview / resume decision state

It should not infer app state from human CLI text, hidden commands, or direct queue-file decoding.
