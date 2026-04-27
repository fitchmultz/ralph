# Ralph Watch Mode
Status: Active
Owner: Maintainers
Source of truth: this document for watch-mode task detection and reconciliation
Parent: [Daemon and Watch](../daemon-and-watch.md)

This guide covers Ralph watch mode: detecting actionable comments, queueing tasks, debouncing file changes, deduplicating comments, and reconciling removed comments.

## Watch Mode

### Overview

Watch mode monitors source files for changes and automatically creates tasks from actionable code comments (`TODO`, `FIXME`, `HACK`, `XXX`).

**Use Cases:**

- Capture technical debt as you code
- Ensure TODOs don't get forgotten
- Track refactoring ideas
- Monitor code review comments

**INTENDED BEHAVIOR**: Detect comments in real-time as files change, with intelligent deduplication and optional auto-closing when comments are removed.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Full implementation with fingerprint-based deduplication, debounced file processing, and comment reconciliation.

---

### Commands

```bash
# Basic watch mode (suggests tasks, doesn't create)
ralph watch

# Watch specific directories
ralph watch src/ tests/

# Auto-create tasks without prompting
ralph watch --auto-queue

# Watch with custom patterns
ralph watch --patterns "*.rs,*.toml"

# Only detect TODO and FIXME
ralph watch --comments todo,fixme

# Enable desktop notifications
ralph watch --auto-queue --notify

# Auto-close tasks when comments are removed
ralph watch --auto-queue --close-removed

# Custom debounce interval
ralph watch --debounce-ms 1000

# Ignore additional patterns
ralph watch --ignore-patterns "vendor/,target/,node_modules/"
```

---

### Triggers

Watch mode triggers on file system events that indicate potential changes to source files.

**INTENDED BEHAVIOR**: React to all meaningful file modifications (create, modify, delete) that could contain actionable comments.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Uses the `notify` crate with `RecommendedWatcher` to monitor file events. The watcher operates in:

- **Recursive mode** for directories
- **Non-recursive mode** for individual files

**Event Types Processed:**

| Event | Action |
|-------|--------|
| File created | Scan for comments |
| File modified | Scan for comments |
| File deleted | Trigger reconciliation (with `--close-removed`) |

**Supported Comment Types:**

| Type | Pattern Example |
|------|-----------------|
| TODO | `// TODO: fix error handling` |
| FIXME | `// FIXME: this is broken` |
| HACK | `// HACK: isolate flaky watcher path` |
| XXX | `// XXX: review this before release` |

**Pattern Matching:**

Comments are matched case-insensitively with flexible separators:

```rust
// All these match TODO pattern:
// TODO: fix this
// TODO - fix this
// TODO; fix this
// TODO fix this
```

---

### Debouncing

Watch mode implements debouncing to batch rapid file changes and avoid redundant processing.

**INTENDED BEHAVIOR**: Wait for a quiet period after file changes before processing, to batch multiple rapid changes into a single scan.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Two-layer debouncing system:

1. **Event-level debouncing**: `WatchState` tracks pending files and only processes when `debounce_duration` has elapsed since the last event
2. **File-level debouncing**: Individual files can't be reprocessed within the debounce window

**Configuration:**

```bash
# Default debounce: 500ms
ralph watch

# Faster response (lower latency, more CPU)
ralph watch --debounce-ms 100

# Slower response (higher latency, less CPU)
ralph watch --debounce-ms 2000
```

**Implementation Details:**

```rust
// From crates/ralph/src/commands/watch/state.rs
pub struct WatchState {
    pub pending_files: HashSet<PathBuf>,
    pub last_event: Instant,
    pub debounce_duration: Duration,
}
```

The event loop:
1. Collects file paths from watch events
2. Adds to `pending_files` set (deduplicates)
3. Updates `last_event` timestamp
4. Processes when `now - last_event >= debounce_duration`

---

### Configuration

#### File Patterns

Default patterns: `*.rs,*.ts,*.js,*.py,*.go,*.java,*.md,*.toml,*.json`

**Custom patterns:**

```bash
# Only Rust and TOML files
ralph watch --patterns "*.rs,*.toml"

# Include YAML and config files
ralph watch --patterns "*.rs,*.yaml,*.yml,*.conf"

# Watch all source files
ralph watch --patterns "*"
```

**Pattern Syntax:**

Uses `globset` for pattern matching:

| Pattern | Matches |
|---------|---------|
| `*.rs` | All Rust files |
| `test_*.py` | Python test files |
| `file[0-9].txt` | file1.txt, file2.txt, etc. |
| `*.{js,ts}` | JavaScript and TypeScript files |

#### Ignore Patterns

Built-in ignores (always applied):
- `/target/`
- `/node_modules/`
- `/.git/`
- `/vendor/`
- `/.ralph/`

**Custom ignores:**

```bash
# Ignore generated files
ralph watch --ignore-patterns "generated/,dist/,build/"

# Ignore specific directories
ralph watch --ignore-patterns "legacy/,third_party/"
```

#### Comment Type Selection

```bash
# All comment types (default)
ralph watch --comments all

# Only TODO
ralph watch --comments todo

# TODO and FIXME only
ralph watch --comments todo,fixme

# Everything except XXX
ralph watch --comments todo,fixme,hack
```

---

### Deduplication

Watch mode uses fingerprint-based deduplication to prevent duplicate tasks.

**INTENDED BEHAVIOR**: Never create duplicate tasks for the same comment, even if the file is moved or line numbers change.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Two-tier deduplication system:

1. **Fingerprint-based** (primary): SHA256 hash of normalized comment content
2. **Location-based** (fallback): File path + line number matching

**Fingerprint Generation:**

```rust
// From crates/ralph/src/commands/watch/tasks.rs
pub fn generate_comment_fingerprint(content: &str) -> String {
    let normalized = content.to_lowercase().trim().to_string();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[..16].to_string()  // First 16 hex chars
}
```

**Deduplication Logic:**

1. Check if task with matching `watch.fingerprint` exists
2. Fallback to `watch.file` + `watch.line` match
3. Legacy fallback: Check task title/notes for file path references

**Stable Across:**
- File moves (fingerprint unchanged)
- Line number changes (fingerprint unchanged)
- Whitespace changes (normalization handles this)
- Case changes (normalization handles this)

---

### Comment Reconciliation

With `--close-removed`, watch mode automatically closes tasks when their originating comments are deleted.

**INTENDED BEHAVIOR**: Keep the task queue in sync with actual code comments.

**CURRENTLY IMPLEMENTED BEHAVIOR**: Full reconciliation system that:

1. Tracks all detected comments across watched files
2. Compares against existing watch-tagged tasks
3. Marks tasks as `done` when comments are removed
4. Adds explanatory note with timestamp

**Reconciliation Process:**

```rust
// Pseudocode of reconciliation logic
for task in queue.tasks:
    if not task.tags.contains("watch"): continue
    if task.status in [Done, Rejected]: continue
    
    comment_exists = check_fingerprint(task) || check_location(task)
    
    if not comment_exists:
        task.status = Done
        task.completed_at = now()
        task.notes.push("Auto-closed: originating comment was removed")
```

**Example:**

```bash
# Terminal 1: Start watch with auto-close
ralph watch --auto-queue --close-removed

# Terminal 2: Work on code
# 1. Add "// TODO: refactor this" → Task auto-created
# 2. Complete the refactoring
# 3. Remove the TODO comment → Task auto-closed
```

---

### Task Metadata

Watch-created tasks include structured metadata in `custom_fields`:

```json
{
  "custom_fields": {
    "watch.file": "/absolute/path/to/file.rs",
    "watch.line": "42",
    "watch.comment_type": "todo",
    "watch.fingerprint": "a1b2c3d4e5f67890",
    "watch.version": "1"
  },
  "tags": ["watch", "todo"]
}
```

**Fields:**

| Field | Description |
|-------|-------------|
| `watch.file` | Absolute path to source file |
| `watch.line` | Line number of comment |
| `watch.comment_type` | Comment type (todo/fixme/hack/xxx) |
| `watch.fingerprint` | SHA256 prefix for deduplication |
| `watch.version` | Metadata format version |

---

### Use Cases

#### Development Workflow

```bash
# Terminal 1: Start watch with auto-queue and auto-close
ralph watch --auto-queue --close-removed

# Terminal 2: Regular development
# - Add TODO/FIXME comments as you code
# - Tasks automatically created
# - Complete work and remove comments
# - Tasks automatically closed
```

#### Code Review Cleanup

```bash
# After code review, scan for new action items
ralph watch src/ --auto-queue --patterns "*.rs"

# Clean up completed items
ralph watch src/ --close-removed --auto-queue
```

#### CI Integration

```bash
# Scan for existing TODOs in CI pipeline
ralph watch --auto-queue --patterns "*.rs" --comments todo,fixme

# Or run continuously in container
ralph watch --auto-queue --close-removed --patterns "*.rs"
```

#### Project Health Monitoring

```bash
# Generate report of all open watch tasks
ralph queue list --tag watch --format json | jq '.tasks | group_by(.custom_fields."watch.comment_type") | map({type: .[0].custom_fields."watch.comment_type", count: length})'
```

---

## See Also

- [Daemon and Watch overview](../daemon-and-watch.md)
- [CLI Reference](../../cli.md)
- [Queue and Tasks](../../queue-and-tasks.md)
- [Daemon Mode](./daemon.md)
- [Operations](./operations.md)
- [Troubleshooting](./troubleshooting.md)
