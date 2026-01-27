# Ralph TUI Task Management (CLI Parity Guide)

This guide maps **CLI task commands** (`ralph task ...`) to equivalent **TUI workflows**.

> Launch the TUI with:
>
> - `ralph tui`
> - (compat) `ralph run one -i` / `ralph run loop -i`

---

## Quick Key Reference (TUI)

In **Normal mode**:

- `j` / `k` or `↓` / `↑`: move selection
- `Enter`: run selected task
- `n`: create new task (manual title)
- `N`: build a task using the task builder agent (prompt for description)
- `e`: edit selected task fields
- `s`: cycle selected task status
- `p`: cycle selected task priority
- `d`: delete selected task (confirmation)
- `a`: archive terminal tasks (Done/Rejected) into done archive (confirmation)
- `:`: command palette (discoverable commands)
- `Ctrl+P`: command palette (shortcut)
- `/`: search (free-text)
- `Ctrl+F`: search (shortcut)
- `t`: filter by tags
- `o`: filter by scope
- `f`: cycle status filter
- `x`: clear filters
- `r`: reload queue from disk
- `?` / `h`: help overlay
- `q` / `Esc`: quit (may prompt if runner active)
- `Ctrl+C` / `Ctrl+Q`: quit (same as `q`/`Esc`)

---

## Feature Mapping: CLI `ralph task ...` → TUI Equivalent

### 1) `ralph task` / `ralph task build` (Build tasks from natural language requests)

**CLI behavior**
- Builds a new task using the task builder prompt/agent.
- Supports flags like `--runner`, `--model`, `--effort`, `--tags`, `--scope`, `--rp-on/--rp-off`.

**TUI equivalent**
- Press `N` in Normal mode
  or
- Press `:` and choose **"Build task with agent"**

**Workflow**
1. Press `N`
2. Type your natural language request/description
3. Press `Enter`
4. The TUI switches to an **Executing** view showing the task builder progress/logs.
5. On completion, the TUI reloads `queue.json` and returns to Normal mode.

**Important differences vs CLI**
- TUI task builder currently uses **project/global config defaults** for runner/model/effort.
- TUI currently does **not** expose:
  - `--runner`, `--model`, `--effort` overrides
  - `--tags`, `--scope` hinting
  - `--rp-on/--rp-off` overrides

If you need those controls, use the CLI `ralph task build ...`.

---

### 2) `ralph task ready <TASK_ID>` (Promote draft → todo)

**CLI behavior**
- Valid only if the task is currently `draft`.
- Sets status to `todo` and may append a note.

**TUI equivalent**
You can promote a draft task to todo by changing its status:

Option A (fast):
- Select the draft task
- Press `s` until status becomes `todo`

Option B (explicit editing UI):
- Select the draft task
- Press `e`
- Navigate to `status`
- Press `Enter` (cycle) until it becomes `todo`

**Note difference**
- CLI supports `--note` on "ready". TUI doesn't have a dedicated "ready with note" action:
  - To record context, edit `notes` (via `e` → `notes`) or append a note before/after changing status.

---

### 3) `ralph task status <STATUS> <TASK_ID>` (Update task status)

**CLI behavior**
- Sets status directly to `draft|todo|doing`.
- If status is `done|rejected`, CLI completes + archives the task (moves it to `done.json`).

**TUI equivalent**
- Select the task and press `s` to cycle status:
  `Draft → Todo → Doing → Done → Rejected → Draft`

Or:
- Press `e`, go to `status`, and press `Enter` to cycle.

**Note difference**
- The TUI cycles status rather than setting a specific target status in one step.
- For notes: edit the `notes` field manually.

---

### 4) `ralph task done <TASK_ID>` (Complete as done)

**CLI behavior**
- Validates task is `todo` or `doing`.
- Sets status `done`, stamps timestamps, and **moves task from `queue.json` → `done.json`** immediately.

**TUI equivalent (2-step)**
1. Mark the task as Done:
   - select task → press `s` until `done`
2. Archive terminal tasks into done archive:
   - press `a`, confirm with `y`

**Why two steps?**
- In the TUI, setting status to `Done` does **not** automatically move the task to `done.json`.
- Archival is an explicit operation (`a`) that moves all terminal tasks (Done/Rejected) to the done archive.

---

### 5) `ralph task reject <TASK_ID>` (Reject and archive)

**CLI behavior**
- Validates task is `todo` or `doing`.
- Sets status `rejected` and **moves task to `done.json`** immediately.

**TUI equivalent (2-step)**
1. Mark task as Rejected:
   - select task → press `s` until `rejected`
2. Archive terminal tasks:
   - press `a`, confirm with `y`

---

### 6) `ralph task edit <FIELD> <VALUE> <TASK_ID>` (Edit any task field)

**CLI behavior**
- Updates any task field (default + custom fields) in-place.
- List fields accept comma/newline-separated values.
- `custom_fields` expects `key=value` entries (comma/newline-separated).
- Optional fields (`request`, `completed_at`) can be cleared with `""`.
- Required timestamps (`created_at`, `updated_at`) must remain valid RFC3339 strings.

Examples:
```
ralph task edit title "Clarify CLI edit" RQ-0001
ralph task edit status doing RQ-0001
ralph task edit tags "cli, rust" RQ-0001
ralph task edit custom_fields "severity=high, owner=ralph" RQ-0001
ralph task edit request "" RQ-0001
```

**TUI equivalent**
- Select the task
- Press `e`
- Navigate to a field
- Press `Enter` to edit/cycle

---

### 7) `ralph task field <KEY> <VALUE> <TASK_ID>` (Set custom fields)

**CLI behavior**
- Sets one custom field key/value on a task.

**TUI equivalent**
- Select the task
- Press `e`
- Navigate to `custom_fields`
- Press `Enter` to edit

**Format in TUI**
The TUI expects a map-like input format:
- `key=value, other=thing`
- You can also separate entries by newlines.

Example:
```
severity=high
component=tui
estimate=2h
```

**Note difference**
- CLI sets a single key/value per invocation.
- TUI edits the entire `custom_fields` map value at once.

---

## Other Task Operations Available in TUI (Beyond `ralph task ...`)

### Delete a task
- Select task → `d` → confirm `y`

### Archive done/rejected tasks
- `a` → confirm `y`
Moves all terminal tasks from active queue into done archive.

### Edit any task field
- Select task → `e`
- Navigate with `j/k` (or arrows)
- `Enter` to edit/cycle
- While editing text:
  - `Enter` saves
  - `Esc` cancels edit
  - `Backspace` deletes
- `x` clears a text/list/map field (where applicable)

Editable fields include (at least):
- title, status, priority
- tags, scope, evidence, plan, notes, depends_on
- request
- custom_fields
- timestamps (created_at, updated_at, completed_at)

---

## Known Gaps / Behavioral Differences vs CLI (Current State)

1. **No direct "operate by TASK_ID" targeting**
   - CLI: `ralph task done RQ-0001`
   - TUI: you must navigate/select the task first.

2. **No bulk scripting / multi-ID operations**
   - CLI can be scripted in shell loops.
   - TUI is inherently interactive.

3. **Task builder overrides not exposed in the TUI**
   - CLI supports runner/model/effort/tags/scope overrides per call.
   - TUI uses config defaults and empty tag/scope hints.

4. **Done/Rejected are not auto-archived**
   - CLI `done/reject` immediately moves tasks to `done.json`.
   - TUI requires explicit archive (`a`) after setting terminal status.

---

## Suggested Enhancement Plan (If You Want Closer CLI Parity)

If you want the TUI to mirror the CLI UX more closely, consider:

1. **Auto-archive on terminal status**
   - When cycling status into Done/Rejected, optionally prompt:
     - "Archive now? (y/n)"
   - Or add a config toggle: `tui.auto_archive_terminal=true`.

2. **"Set status to …" palette commands**
   - Add palette commands:
     - "Set status: Draft/Todo/Doing/Done/Rejected"
   - Avoid multi-step cycling when you know the target.

3. **Task builder "advanced" input**
   - Extend the build-task mode to optionally capture:
     - tags/scope hints
     - runner/model/effort overrides
     - RepoPrompt requirement toggle
   - Could be done via a multi-field modal or a single-line syntax like:
     - `tags=cli,tui scope=crates/ralph :: Add feature ...`

4. **Jump-to-ID**
   - Add command palette action: "Select task by ID"
   - Input `RQ-####` and focus selection.

These are optional; core task management is already present in the TUI today.

---
