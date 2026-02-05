# Ralph TUI Task Management (CLI Parity Guide)

This guide maps **CLI task commands** (`ralph task ...`) to equivalent **TUI workflows**.

> Launch the TUI with:
>
> - `ralph tui`
> - (compat) `ralph run one -i` / `ralph run loop -i`

---

## Quick Key Reference (TUI)

In **Normal mode**:

Navigation:
- `Tab/Shift+Tab`: switch focus between list/details
- `j` / `k` or `↓` / `↑`: move selection (list focus) / scroll details (details focus)
- `PgUp/PgDn`: page list/details (focused panel)
- `Home/End`: jump to top/bottom (focused panel)
- `K/J`: move selected task up/down
- `G`: jump to task by ID (prompts for task ID)
- `Enter`: run selected task

Actions:
- `l`: toggle loop mode
- `a`: archive terminal tasks (Done/Rejected) into done archive (confirmation)
- `d`: delete selected task (confirmation)
- `e`: edit selected task fields
- `n`: create new task (manual title)
- `N`: build a task using the task builder agent (prompt for description)
- `c`: edit project config
- `g`: scan repository
- `v`: view dependency graph
- `P`: view parallel run state (read-only)
- `r`: reload queue from disk
- `O`: open selected task scope in $EDITOR
- `y`: copy file:line refs from notes/evidence
- `?` / `h`: help overlay
- `q` / `Esc`: quit (may prompt if runner active)
- `Ctrl+C` / `Ctrl+Q`: quit (same as `q`/`Esc`)

Command Palette:
- `:`: open command palette (type to filter, Enter to run, Esc to cancel)
- `Ctrl+P`: command palette (shortcut)

Filters & Search:
- `/`: search (free-text)
- `Ctrl+F`: search tasks (shortcut)
- `t`: filter by tags
- `o`: filter by scope
- `f`: cycle status filter
- `x`: clear filters
- `C`: toggle case-sensitive search
- `R`: toggle regex search

Quick Changes:
- `s`: cycle selected task status
- `p`: cycle selected task priority

In **Execution view** (while a task is running):
- `Esc`: return to task list
- `j` / `k` or `↓` / `↑`: scroll logs
- `PgUp/PgDn`: page logs
- `a`: toggle auto-scroll
- `l`: stop loop mode

---

## Feature Mapping: CLI `ralph task ...` → TUI Equivalent

### 1) `ralph task` / `ralph task build` (Build tasks from natural language requests)

**CLI behavior**
- Builds a new task using the task builder prompt/agent.
- Supports flags like `--runner`, `--model`, `--effort`, `--tags`, `--scope`, `--repo-prompt`.

**TUI equivalent**
- Press `N` in Normal mode
  or
- Press `:` and choose **"Build task with agent"**

**Workflow**
1. Press `N`
2. Type your natural language request/description
3. Press `Enter` to continue to advanced options
4. Configure optional overrides (or leave as defaults):
   - **Tags hint**: Comma-separated tags to suggest for the new task
   - **Scope hint**: Scope is a starting point, not a restriction; comma-separated scope paths to suggest
   - **Runner**: Override the agent runner (claude, codex, opencode, gemini, cursor)
   - **Model**: Override the model (e.g., "sonnet", "gpt-5.3-codex")
   - **Reasoning effort**: Override effort level (low, medium, high, xhigh)
   - **RepoPrompt mode**: Override RepoPrompt behavior (tools, plan, off)
5. Navigate to "[ Build Task ]" and press `Enter`
6. The TUI switches to an **Executing** view showing the task builder progress/logs.
7. On completion, the TUI reloads `queue.json` and returns to Normal mode.

**Controls in Advanced Options**
- `↑/↓` or `j/k`: Navigate between fields
- `Space` or `Enter`: Cycle through enum options (runner, effort, repo-prompt)
- Type directly: Edit text fields (tags, scope, model)
- `x`: Clear the current field (reset to config default)
- `Esc`: Cancel the task builder

**CLI Parity**
The TUI task builder now supports the same override options as the CLI:
- `--runner` → Runner field
- `--model` → Model field
- `--effort` → Reasoning effort field
- `--tags` → Tags hint field
- `--scope` → Scope hint field
- `--repo-prompt` → RepoPrompt mode field

All overrides are optional; leaving them as "(use config default)" uses the project/global config values.

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

### Repair queue
- Press `:` and type "repair"
- Choose from:
  - **"Repair queue"** - Validates and fixes queue issues (confirmation required)
  - **"Repair queue (dry run)"** - Validates without modifying files

Repairs include:
- Fixing missing fields (title, tags, scope, evidence, plan, request)
- Normalizing timestamps to UTC
- Backfilling completed_at for done/rejected tasks
- Remapping duplicate or invalid IDs
- Updating dependencies after ID remapping

### Unlock queue
- Press `:` and type "unlock"
- Choose **"Unlock queue"** (confirmation required)

Removes the queue lock file (`.ralph/lock/`). Use this when the queue is stuck due to a stale lock.

### Set status directly (without cycling)
- Press `:` and type "set status"
- Choose from: Draft, Todo, Doing, Done, Rejected
- Or use palette commands directly:
  - "Set status: Draft"
  - "Set status: Todo"
  - "Set status: Doing"
  - "Set status: Done"
  - "Set status: Rejected"

### Set priority directly (without cycling)
- Press `:` and type "set priority"
- Choose from: Critical, High, Medium, Low
- Or use palette commands directly:
  - "Set priority: Critical"
  - "Set priority: High"
  - "Set priority: Medium"
  - "Set priority: Low"

### Auto-archive configuration
The TUI supports optional auto-archive behavior when setting tasks to Done/Rejected.
Configure via `.ralph/config.json`:

```json
{
  "tui": {
    "auto_archive_terminal": "never"
  }
}
```

Values:
- `"never"` (default): No auto-archive; tasks remain in queue until you press `a`.
- `"prompt"`: Ask for confirmation before archiving when setting Done/Rejected.
- `"always"`: Archive immediately when setting Done/Rejected (no confirmation).

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

### Text Editing Key Reference

While editing in **single-line inputs** or **textareas**:

- `Enter`: commit/submit (save edit)
- `Esc`: cancel edit (discard changes)
- `←/→` (Left/Right): move cursor by character
- `Home/End`: move cursor to start/end of line
- `Backspace`: delete previous character
- `Delete`: delete next character
- `Ctrl+Backspace` or `Ctrl+W`: delete previous word
- `Paste` (terminal paste): insert text at cursor

**Textarea-only bindings:**
- `Alt+Enter`: insert newline (keeps edit active)
- `↑/↓` (Up/Down): move cursor vertically between lines
- `PgUp/PgDn`: scroll within the textarea
- `Ctrl+←/→`: move cursor by word

---

## Component Behaviors

### Scrollable Containers (Log/Detail Panels)

When focused:
- `↑/↓`: scroll up/down by one line
- `PgUp/PgDn`: scroll up/down by one viewport
- `Home/End`: jump to top/bottom
- Mouse wheel: scroll (3 lines per tick)
- Sticky scroll: auto-follows new content until you scroll up manually

### Diff Viewer (Unified)

`DiffViewerComponent` renders a unified, line-based diff with:
- `-` deletions (red)
- `+` insertions (green)
- ` ` unchanged (gray)

It uses the same scrolling behavior as other scrollable panels:
- `↑/↓`, `PgUp/PgDn`, `Home/End`
- Mouse wheel scrolling
- Scrollbar indicator

**Usage:**
```rust
use crate::tui::components::{DiffViewerComponent, DiffViewerStyle};

let mut diff_viewer = DiffViewerComponent::new();
diff_viewer.set_title("Changes");
diff_viewer.set_inputs(old_content, new_content);
// Render via Component::render()
```

### Line Number Gutter

`LineNumberGutterComponent` renders a dedicated gutter area (separate from content) with:
- Right-aligned line numbers
- Optional per-line highlight background + symbol
- Optional error/warning diagnostic symbols

**Compose gutter + content side-by-side:**

```rust
use crate::tui::foundation::layout::{row, Item};
use crate::tui::components::{
    LineNumberGutterComponent, 
    LineNumberGutterConfig, 
    DiffViewerComponent,
    DiagnosticSeverity, 
    LineHighlight,
};
use std::collections::HashMap;

// Layout: fixed-width gutter + flexible content area
let rects = row(area, 0, &[Item::fixed(6), Item::flex(1)]);
let gutter_area = rects[0];
let content_area = rects[1];

// Configure gutter
let mut gutter = LineNumberGutterComponent::new(LineNumberGutterConfig::default());
gutter.set_range(1, 100); // start at line 1, 100 total lines

// Add highlights
let mut highlights = HashMap::new();
highlights.insert(5, LineHighlight {
    bg: Color::Blue,
    symbol: Some("▶".to_string()),
});
gutter.set_highlights(highlights);

// Add diagnostics
let mut diagnostics = HashMap::new();
diagnostics.insert(10, DiagnosticSeverity::Error);
diagnostics.insert(15, DiagnosticSeverity::Warning);
gutter.set_diagnostics(diagnostics);

// Render gutter and content
gutter.render(f, gutter_area, app, ctx);
content.render(f, content_area, app, ctx);
```

### Select Lists / Dropdowns

When a select list is open:
- `↑/↓`: move highlight
- `PgUp/PgDn`: page highlight
- `Home/End`: jump to first/last item
- `Enter`: select highlighted item
- `Esc`: close (or clear filter first, if active)
- Mouse: click to select, wheel to scroll

If type-to-filter is enabled:
- Type to filter (case-insensitive substring match)
- `Backspace`: delete last filter character
- `Ctrl+U`: clear filter

### Sliders

When focused:
- `←/→`: adjust by step
- `PgUp/PgDn`: adjust by page step
- `Home/End`: jump to min/max

---

## Known Gaps / Behavioral Differences vs CLI (Current State)

1. **No direct "operate by TASK_ID" targeting**
   - CLI: `ralph task done RQ-0001`
   - TUI: you must navigate/select the task first.

2. **No bulk scripting / multi-ID operations**
   - CLI can be scripted in shell loops.
   - TUI is inherently interactive.

3. ~~**Task builder overrides not exposed in the TUI**~~ ✅ **RESOLVED**
   - TUI now supports runner/model/effort/tags/scope/repo-prompt overrides.
   - Press `N`, enter description, then configure advanced options.

4. ~~**Done/Rejected are not auto-archived**~~ ✅ **RESOLVED**
   - TUI now supports direct set-status commands via palette (`:` → "Set status: ...").
   - Config option `tui.auto_archive_terminal` controls auto-archive behavior:
     - `"never"` (default): No auto-archive; use `a` to archive manually.
     - `"prompt"`: Ask before archiving when setting Done/Rejected.
     - `"always"`: Archive immediately when setting Done/Rejected.

5. **No phase-specific runner/model/effort overrides in TUI task builder**
   - CLI: Supports `--runner-phaseN`, `--model-phaseN`, `--effort-phaseN` flags for per-phase overrides
   - TUI: Task builder (`N` key) only supports global runner/model/effort overrides
   - Workaround: Use config `agent.phase_overrides.phase1/phase2/phase3` for per-phase defaults,
     or use CLI `ralph run` commands with phase-specific flags instead of TUI

---

## Suggested Enhancement Plan (If You Want Closer CLI Parity)

If you want the TUI to mirror the CLI UX more closely, consider:

1. ~~**Auto-archive on terminal status**~~ ✅ **IMPLEMENTED**
   - Config `tui.auto_archive_terminal` controls behavior: `never`, `prompt`, or `always`.
   - When set to `prompt` or `always`, setting status to Done/Rejected triggers auto-archive.

2. ~~**"Set status to …" palette commands**~~ ✅ **IMPLEMENTED**
   - Palette commands available: "Set status: Draft/Todo/Doing/Done/Rejected".
   - Also available: "Set priority: Critical/High/Medium/Low".
   - Access via `:` and type "set status" or "set priority".

3. ~~**Task builder "advanced" input**~~ ✅ **IMPLEMENTED**
   - TUI now has a two-step task builder flow with override options.
   - Supports tags/scope hints, runner/model/effort overrides, and RepoPrompt mode.

4. ~~**Jump-to-ID**~~ ✅ **IMPLEMENTED**
   - Press `G` (uppercase) or use the "Jump to task by ID" palette command.
   - Input `RQ-####` (case-insensitive) and press Enter to jump to that task.
   - If the task is filtered out, filters are automatically cleared.

These are optional; core task management is already present in the TUI today.

---

## Visual Polish Features

### Big Text Headers

The TUI displays large ASCII art headers in select screens when the terminal is wide enough:

- **Help overlay**: Shows a big "RALPH" header at the top
- **Empty queue welcome**: Shows a big "RALPH" header when no tasks exist

These headers automatically:
- Scale to fit the available terminal width (font size: Block → Shade → Slick → Tiny)
- Fall back to plain text on narrow terminals (< 22 columns)
- Gracefully handle very small terminals without errors

### Animation System

The TUI includes a minimal, deterministic animation system for subtle visual polish:

- **Fade-in effect**: Overlays (like Help) fade in smoothly when opened
- **Frame-based timing**: Animations use a frame counter for consistent timing across different terminal refresh rates
- **Graceful degradation**: Animations are automatically disabled when:
  - `NO_COLOR` environment variable is set
  - `TERM=dumb` is detected
  - `RALPH_TUI_NO_ANIM=1` or `true` is set

To disable animations manually:
```bash
RALPH_TUI_NO_ANIM=1 ralph tui
```

Or permanently in your shell configuration:
```bash
export RALPH_TUI_NO_ANIM=1
```

---

## Parallel Run State Overlay

The TUI includes a read-only overlay for monitoring parallel execution state without leaving the interface.

### Opening the Overlay

- Press `P` (uppercase) in Normal mode to view the parallel run state
- This reads from `.ralph/cache/parallel/state.json`

### What It Shows

The overlay displays three tabs:

1. **In-Flight Tasks** - Currently running worker tasks with:
   - Task ID
   - Workspace path
   - Branch name
   - Process ID (PID)

2. **PRs** - Pull request records with:
   - Task ID
   - PR number
   - State (open/closed/merged)
   - Merge blockers (if any)
   - PR URL

3. **Finished Without PR** - Tasks that completed without creating a PR:
   - Task ID
   - Success/failure status
   - Reason code
   - Message (if any)

### Controls

When the overlay is open:

- `Esc` or `P`: Close the overlay
- `Tab` or `←/→`: Switch between tabs (In-Flight, PRs, No PR)
- `r`: Reload state from disk
- `↑/↓` or `j/k`: Navigate within the current tab
- `PgUp/PgDn`: Page up/down
- `Home/End` or `g/G`: Jump to top/bottom
- `Enter` or `o`: Open selected PR URL in browser
- `y`: Copy selected PR URL to clipboard

### When No Parallel Run is Active

If no state file exists (i.e., no `ralph run loop --parallel` is running), the overlay shows:
- A message indicating no parallel run is in progress
- The expected path to the state file
- Instructions on how to start a parallel run

---

## Markdown Rendering in Task Details

The TUI now supports rich Markdown rendering for task content, providing better readability for structured text and code.

### Supported Markdown Subset

The following Markdown elements are supported when viewing task details:

- **Headings**: `# H1`, `## H2`, `### H3+` - Rendered with distinct colors and bold styling
- **Emphasis**: `*italic*`, `**bold**` - Properly styled text emphasis
- **Inline code**: `` `code` `` - Highlighted with distinct background
- **Code blocks**: ` ```language ` - Fenced code blocks with optional syntax highlighting
- **Lists**:
  - Unordered: `- item`, `* item` - Bullet points
  - Ordered: `1. item`, `2. item` - Numbered lists
- **Line breaks**: Soft and hard breaks are handled correctly

### Syntax Highlighting

Code blocks with language hints receive syntax highlighting:

```markdown
```rust
fn main() {
    println!("Hello, Ralph!");
}
```
```

Currently supported languages:
- **Rust** (`rust`, `rs`) - Full Tree-sitter-based highlighting

For unsupported languages, code blocks are rendered with monospace styling and a visual gutter.

### Where Markdown is Rendered

Markdown rendering is applied automatically to:

- **Task Evidence** - Bullet-point lists support Markdown formatting
- **Task Plan** - Numbered steps support Markdown formatting  
- **Task Notes** - Free-form notes support Markdown formatting
- **Task Description** (Task Builder) - Your natural language description is rendered as Markdown
- **Help Overlay** - All help content is rendered as Markdown

### Graceful Degradation

If Tree-sitter highlighting is unavailable or fails to initialize:
- Code blocks still render with proper monospace font and visual gutter
- All other Markdown elements continue to work normally
- No error messages or visual glitches appear

### Example Task Content

When you write task content like this:

```markdown
## Implementation Steps

1. Add the `pulldown-cmark` dependency to **Cargo.toml**
2. Create a `MarkdownRenderer` component
3. Handle code blocks with ` ```rust ` fences:
   ```rust
   fn render() -> Vec<Line> {
       // Your code here
   }
   ```
4. Test with *italic* and **bold** formatting
```

It will render in the TUI with proper styling, colors, and code highlighting.
