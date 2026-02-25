# Import/Export System

![Import & Export](../assets/images/2026-02-07-11-32-24-import-export.png)

Ralph's import/export system enables bulk operations, cross-repository migration, and integration with external tools. Tasks can be exported to various formats for reporting or external processing, and imported from CSV, TSV, or JSON for bulk backlog seeding.

---

## Overview

The import/export system provides:

- **Multiple export formats**: CSV, TSV, JSON, Markdown table, and GitHub issue format
- **Flexible import sources**: CSV, TSV, and JSON with automatic normalization
- **Smart duplicate handling**: Configurable policies for existing task IDs
- **Powerful filtering**: Export subsets by status, tags, scope, date range, or ID pattern
- **GitHub integration**: Publish tasks directly as GitHub issues
- **Round-trip support**: Export and re-import without data loss

---

## Export

Export tasks from the active queue (`.ralph/queue.jsonc`) and optionally the done archive (`.ralph/done.jsonc`).

### Command

```bash
ralph queue export [OPTIONS]
```

### Supported Formats

| Format | Description | Use Case |
|--------|-------------|----------|
| `csv`  | Comma-separated values | Spreadsheet import, data processing |
| `tsv`  | Tab-separated values | Unix pipeline processing |
| `json` | JSON array of tasks | API integration, backups |
| `md`   | Markdown table | Documentation, reports |
| `gh`   | GitHub issue format | Issue creation templates |

### Basic Examples

```bash
# Export all tasks to CSV (default)
ralph queue export

# Export to JSON file
ralph queue export --format json --output tasks.json

# Export to TSV for pipeline processing
ralph queue export --format tsv --output tasks.tsv

# Export as Markdown table for documentation
ralph queue export --format md --output tasks.md
```

### Filtering Options

Filter tasks to export only what you need:

```bash
# Filter by status (repeatable)
ralph queue export --status todo --status doing

# Filter by tag (repeatable, case-insensitive)
ralph queue export --tag rust --tag cli

# Filter by scope token (repeatable, substring match)
ralph queue export --scope "crates/ralph"

# Filter by ID pattern (substring match)
ralph queue export --id_pattern "RQ-000"

# Filter by creation date (RFC3339 or YYYY-MM-DD)
ralph queue export --created-after 2026-01-01
ralph queue export --created-before 2026-02-01

# Combine filters
ralph queue export --status todo --tag bug --created-after 2026-01-15
```

### Archive Options

Include completed tasks from the archive:

```bash
# Include done.json archive with active queue
ralph queue export --include-archive

# Export only from archive (ignore active queue)
ralph queue export --only-archive

# Export completed tasks from January
ralph queue export --only-archive --status done --created-after 2026-01-01 --created-before 2026-02-01
```

### Output Formats

#### CSV/TSV Format

The CSV/TSV export uses these columns:

```
id,title,status,priority,tags,scope,evidence,plan,notes,request,created_at,updated_at,completed_at,depends_on,custom_fields,parent_id
```

**Array field delimiters:**
- `tags`, `scope`, `depends_on`: comma-separated
- `evidence`, `plan`, `notes`: semicolon-separated
- `custom_fields`: `key=value` pairs, comma-separated

**Example CSV row:**
```csv
RQ-0001,Fix authentication bug,high,todo,"bug,auth","src/auth.rs","unit tests pass;integration tests pass",,,,2026-01-15T00:00:00Z,2026-01-15T12:00:00Z,,,,
```

#### JSON Format

JSON export produces a pretty-printed array of task objects:

```json
[
  {
    "id": "RQ-0001",
    "title": "Fix authentication bug",
    "status": "todo",
    "priority": "high",
    "tags": ["bug", "auth"],
    "scope": ["src/auth.rs"],
    "evidence": ["unit tests pass", "integration tests pass"],
    "plan": [],
    "notes": [],
    "created_at": "2026-01-15T00:00:00Z",
    "updated_at": "2026-01-15T12:00:00Z",
    "depends_on": [],
    "custom_fields": {},
    "parent_id": null
  }
]
```

#### Markdown Format

Markdown export produces a GitHub-flavored table:

```markdown
| ID | Status | Priority | Title | Tags | Scope | Created |
|---|---|---|---|---|---|---|
| RQ-0001 | todo | high | Fix authentication bug | `bug`, `auth` | `src/auth.rs` | 2026-01-15 |
```

#### GitHub Issue Format

GitHub format produces Markdown optimized for issue bodies:

```markdown
## RQ-0001: Fix authentication bug

**Status:** `todo` | **Priority:** `high`

**Tags:** `bug`, `auth`

### Scope

- `src/auth.rs`

### Evidence

- unit tests pass
- integration tests pass

<!-- ralph_task_id: RQ-0001 -->
```

Multiple tasks are separated by horizontal rules (`---`).

---

## Import

Import tasks from CSV, TSV, or JSON into the active queue.

### Command

```bash
ralph queue import --format <FORMAT> [OPTIONS]
```

### Supported Formats

| Format | Description | Source |
|--------|-------------|--------|
| `csv`  | Comma-separated values | Spreadsheets, exports |
| `tsv`  | Tab-separated values | Unix tools, exports |
| `json` | JSON array or wrapper object | API responses, backups |

### Basic Examples

```bash
# Import from CSV file
ralph queue import --format csv --input tasks.csv

# Import from TSV file
ralph queue import --format tsv --input tasks.tsv

# Import from JSON file
ralph queue import --format json --input tasks.json

# Import from stdin (useful for piping)
cat tasks.csv | ralph queue import --format csv --input -
ralph queue export --format json | ralph queue import --format json --input -
```

### Dry Run

Preview changes without modifying the queue:

```bash
# See what would be imported
ralph queue import --format csv --input tasks.csv --dry-run

# Validate import data
ralph queue export --format json | ralph queue import --format json --dry-run
```

### Normalization and Backfill

During import, Ralph automatically normalizes and backfills task data:

| Field | Normalization |
|-------|---------------|
| `id` | Trimmed; empty IDs get auto-generated |
| `title` | Trimmed; must be non-empty |
| List fields | Trimmed, empty items dropped |
| `tags`, `scope`, `depends_on`, `blocks`, `relates_to` | Trimmed, empty values removed |
| `evidence`, `plan`, `notes` | Trimmed, empty values removed |
| `custom_fields` | Keys trimmed, empty keys removed |
| `created_at` | Backfilled to current time if missing |
| `updated_at` | Backfilled to current time if missing |
| `completed_at` | Backfilled for `done`/`rejected` status |

### CSV/TSV Format Requirements

**Required column:**
- `title` (string, non-empty) - Task title

**Optional columns:**
- `id` (string) - Task ID (auto-generated if empty/missing)
- `status` (string) - One of: `draft`, `todo`, `doing`, `done`, `rejected` (default: `todo`)
- `priority` (string) - One of: `critical`, `high`, `medium`, `low` (default: `medium`)
- `tags` (comma-separated) - List of tags
- `scope` (comma-separated) - List of scope paths
- `evidence` (semicolon-separated) - Evidence items
- `plan` (semicolon-separated) - Plan steps
- `notes` (semicolon-separated) - Notes
- `request` (string) - Original request text
- `created_at` (RFC3339) - Creation timestamp
- `updated_at` (RFC3339) - Update timestamp
- `completed_at` (RFC3339) - Completion timestamp
- `depends_on` (comma-separated) - Dependency task IDs
- `blocks` (comma-separated) - Blocked task IDs
- `relates_to` (comma-separated) - Related task IDs
- `duplicates` (string) - Duplicated task ID
- `custom_fields` (comma-separated `k=v` pairs) - Custom key-value pairs
- `parent_id` (string) - Parent task ID

**Example CSV:**
```csv
title,status,priority,tags,scope
core::fmt: Debug for DirectoryIter,done,high,"good-first-issue,rustdoc","library/core/src/fs.rs"
Add CI for macOS,todo,medium,ci,".github/workflows/"
```

**Example TSV:**
```tsv
title	status	priority	tags
Refactor error handling	todo	high	error-handling
```

### JSON Format

JSON import accepts two formats:

**1. Array of tasks:**
```json
[
  {
    "title": "Task one",
    "status": "todo"
  },
  {
    "title": "Task two",
    "status": "doing",
    "priority": "high"
  }
]
```

**2. Wrapper object (for versioned schemas):**
```json
{
  "version": 1,
  "tasks": [
    {
      "title": "Task one",
      "status": "todo"
    }
  ]
}
```

### Duplicate Handling

Control behavior when imported task IDs already exist:

```bash
# Fail on duplicates (default)
ralph queue import --format json --input tasks.json --on-duplicate fail

# Skip duplicate tasks
ralph queue import --format csv --input tasks.csv --on-duplicate skip

# Generate new IDs for duplicates
ralph queue import --format json --input tasks.json --on-duplicate rename
```

| Policy | Behavior |
|--------|----------|
| `fail` | Abort import with error if any duplicate ID exists |
| `skip` | Skip duplicate tasks, continue importing others |
| `rename` | Generate fresh IDs for duplicate tasks |

When using `rename`, Ralph reports the ID mappings:
```
Imported tasks. parsed 5 task(s); imported 5; renamed 2 task(s)
  OLD-001 -> RQ-0042
  OLD-002 -> RQ-0043
```

---

## Round-trip

Export tasks and re-import them without data loss. This is useful for backups, migrations, and external editing.

### Basic Round-trip

```bash
# Export to JSON and re-import
ralph queue export --format json --output backup.json
ralph queue import --format json --input backup.json --dry-run
ralph queue import --format json --input backup.json
```

### Cross-repo Migration

```bash
# In source repo
ralph queue export --format json --output ~/tasks.json

# In target repo
ralph queue import --format json --input ~/tasks.json --on-duplicate rename
```

### External Editing Workflow

```bash
# Export to CSV for spreadsheet editing
ralph queue export --format csv --output tasks.csv

# Edit in spreadsheet (LibreOffice, Excel, etc.)
# ... make changes ...

# Import back with rename policy (safer for edited IDs)
ralph queue import --format csv --input tasks.csv --on-duplicate rename
```

### Round-trip Considerations

**Preserved:**
- All task fields
- Timestamps (if provided)
- Tags, scope, evidence, plan, notes
- Dependencies and relationships

**Normalized:**
- Missing timestamps backfilled
- Empty list items removed
- Whitespace trimmed
- IDs generated for empty/missing IDs

**Not preserved in CSV/TSV:**
- Task order (new tasks are positioned per queue policy)

---

## GitHub Issue Publishing

Publish tasks as GitHub issues directly from the command line.

### Prerequisites

- `gh` CLI installed and authenticated (`gh auth status`)
- GitHub repository (local or specified via `--repo`)

### Command

```bash
ralph queue issue publish <TASK_ID> [OPTIONS]
```

### Examples

```bash
# Publish a task as a new GitHub issue
ralph queue issue publish RQ-0001

# Preview without creating
ralph queue issue publish RQ-0001 --dry-run

# Add labels and assignees
ralph queue issue publish RQ-0001 --label bug --label help-wanted --assignee @me

# Publish to a different repository
ralph queue issue publish RQ-0001 --repo owner/repo

# Combine options
ralph queue issue publish RQ-0001 --label enhancement --assignee alice --assignee bob
```

### How It Works

1. **Create**: If the task has no `github_issue_url` custom field, creates a new issue
2. **Update**: If `github_issue_url` exists, updates the existing issue
3. **Persist**: Stores the issue URL and number in the task's `custom_fields`

The issue body is rendered from the task data:
- Status and priority badges
- Tags as labels
- Plan, evidence, scope, and notes sections
- Original request (if present)
- Ralph task ID marker (for automation/debugging)

### Issue Metadata

After publishing, these custom fields are set:

```json
{
  "custom_fields": {
    "github_issue_url": "https://github.com/owner/repo/issues/42",
    "github_issue_number": "42"
  }
}
```

To re-publish (update) an issue, run the command again. Ralph detects the existing URL and updates rather than creates.

---

## Use Cases

### Bulk Backlog Seeding

Import a large number of tasks from an external source:

```bash
# Prepare tasks in a spreadsheet
# Export to CSV: id,title,status,priority,tags

# Import with rename to ensure clean IDs
ralph queue import --format csv --input backlog.csv --on-duplicate rename
```

### Cross-repo Migration

Move tasks between Ralph-managed repositories:

```bash
# Source repo: export all tasks
ralph queue export --include-archive --format json --output all-tasks.json

# Target repo: import with rename to avoid ID collisions
ralph queue import --format json --input all-tasks.json --on-duplicate rename
```

### Integration with External Tools

```bash
# Export for project management tools
ralph queue export --format csv --status todo --status doing

# Export for documentation
ralph queue export --format md --status done --created-after 2026-01-01

# Export for CI/CD pipelines
ralph queue export --format json --tag ci | jq '.[].id'
```

### Backup and Restore

```bash
# Daily backup
ralph queue export --include-archive --format json --output backup-$(date +%Y%m%d).json

# Restore from backup
ralph queue import --format json --input backup-20260115.json --on-duplicate skip
```

### GitHub Issue Sync

```bash
# Publish high-priority tasks to GitHub
ralph queue export --format json --priority critical | \
  jq -r '.[].id' | \
  xargs -I {} ralph queue issue publish {} --label critical
```

---

## Best Practices

1. **Always use `--dry-run` first** when importing large batches
2. **Use `--on-duplicate rename`** for cross-repo migrations to avoid ID conflicts
3. **Export with `--include-archive`** for complete backups
4. **Use TSV for Unix pipelines** - no quoting issues with commas
5. **Use JSON for programmatic access** - preserves all fields exactly
6. **Use Markdown for human-readable reports** - great for documentation
7. **Check import reports** - review rename mappings and skip counts

---

## Troubleshooting

### Import fails with "title is required"

Ensure your CSV has a `title` column with non-empty values for all rows.

### Duplicate ID errors

Use `--on-duplicate skip` or `--on-duplicate rename`:

```bash
ralph queue import --format csv --input tasks.csv --on-duplicate rename
```

### CSV parsing errors

- Check for proper quoting of fields containing commas
- Ensure headers are present and spelled correctly
- Use TSV format if your data contains many commas

### GitHub publishing fails

- Verify `gh auth status` shows you're logged in
- Check repository permissions
- Use `--dry-run` to preview the issue body
