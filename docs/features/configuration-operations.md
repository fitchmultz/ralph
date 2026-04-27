# Queue and Parallel Configuration
Status: Active
Owner: Maintainers
Source of truth: this document for feature-level queue and parallel configuration guidance
Parent: [Configuration Feature Guide](configuration.md)

Use this guide when configuring Ralph queue files, task aging, archive behavior, and parallel worker workspaces. Exact fields and defaults live in [Queue Configuration](../configuration.md#queue-configuration) and [Parallel Configuration](../configuration.md#parallel-configuration).

---

## Queue File Locations

Default queue paths:

- `queue.file`: `.ralph/queue.jsonc`
- `queue.done_file`: `.ralph/done.jsonc`

Notes:

- Relative paths resolve from repo root; canonical config also documents absolute path and `~` expansion behavior.
- Machine and app integrations should consume CLI machine surfaces rather than hardcoded file assumptions.
- During `ralph run loop --parallel ...`, queue/done paths must remain under repo root.

Canonical details:

- [Queue Configuration](../configuration.md#queue-configuration)
- [Queue and Tasks](../queue-and-tasks.md)

---

## Task ID and Aging Settings

Common queue hygiene settings:

- `queue.id_prefix`
- `queue.id_width`
- `queue.aging_thresholds.warning_days`
- `queue.aging_thresholds.stale_days`
- `queue.aging_thresholds.rotten_days`

Invariant: `warning_days < stale_days < rotten_days`.

---

## Auto-Archive

`queue.auto_archive_terminal_after_days` controls terminal-task archiving:

- `null`: disabled
- `0`: archive immediately when sweep runs
- `N > 0`: archive when `completed_at` is at least `N` days old

---

## Parallel Workers

Common knobs:

- `parallel.workers`
- `parallel.workspace_root`
- `parallel.max_push_attempts`
- `parallel.push_backoff_ms`
- `parallel.workspace_retention_hours`

If `workspace_root` is inside the repo, keep it gitignored.

Current parallel mode does not use legacy PR-era keys; prefer the direct-push model documented in [Parallel](./parallel.md).

---

## Example

```jsonc
{
  "version": 2,
  "queue": {
    "file": ".ralph/queue.jsonc",
    "done_file": ".ralph/done.jsonc",
    "auto_archive_terminal_after_days": 7
  },
  "parallel": {
    "workers": 3,
    "workspace_retention_hours": 24
  }
}
```

---

## See Also

- [Configuration Feature Guide](configuration.md)
- [Main Configuration Reference](../configuration.md)
- [Queue](./queue.md)
- [Parallel](./parallel.md)
- [Queue and Tasks](../queue-and-tasks.md)
