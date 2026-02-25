# Ralph Migration System

The migration system manages breaking changes to configuration keys, file formats, and project structure. It provides automated detection, safe application with backup/rollback capability, and preserves JSONC comments during migrations.

---

## Table of Contents

1. [Overview](#overview)
2. [Migration Types](#migration-types)
3. [Migration Registry](#migration-registry)
4. [History Tracking](#history-tracking)
5. [CLI Commands](#cli-commands)
6. [Automatic Migrations](#automatic-migrations)
7. [Manual Migrations](#manual-migrations)
8. [Breaking Changes Reference](#breaking-changes-reference)
9. [Implementation Details](#implementation-details)

---

## Overview

### What Are Migrations?

Migrations are automated transformations that update your project's configuration and files when Ralph introduces breaking changes. They ensure smooth upgrades without manual editing of config files.

### When Are Migrations Needed?

| Scenario | Migration Type |
|----------|---------------|
| Config key renamed | `ConfigKeyRename` |
| File moved or renamed | `FileRename` |
| README template updated | `ReadmeUpdate` |
| Queue format changes | `FileRename` (JSON → JSONC) |

### Key Features

- **Idempotent**: Running the same migration twice is safe (no-op if already applied)
- **JSONC-Preserving**: Comments and formatting are preserved during config migrations
- **Backup-Capable**: Original files are kept as backups during file migrations
- **History-Tracked**: All applied migrations are recorded in `.ralph/cache/migrations.json`
- **Scoped Renames**: Config key renames are scoped to their parent object (e.g., `parallel.worktree_root` only renames within `parallel` objects)

---

## Migration Types

### ConfigKeyRename

Renames a configuration key while preserving JSONC comments and formatting.

**Structure:**
```rust
MigrationType::ConfigKeyRename {
    old_key: "parallel.worktree_root",
    new_key: "parallel.workspace_root",
}
```

**Behavior:**
- Supports dot notation for nested keys (`agent.runner_cli`)
- Scoped to parent object (only renames within the specified parent)
- Updates both project config (`.ralph/config.jsonc`) and global config (`~/.config/ralph/config.jsonc`)
- Preserves all JSONC comments and formatting

**Example:**
```json
// Before
{
    "parallel": {
        "worktree_root": "/tmp/worktrees"
    }
}

// After
{
    "parallel": {
        "workspace_root": "/tmp/worktrees"
    }
}
```

### FileRename

Moves or renames a file, with optional config reference updates.

**Structure:**
```rust
MigrationType::FileRename {
    old_path: ".ralph/queue.json",
    new_path: ".ralph/queue.jsonc",
}
```

**Behavior:**
- Copies content from old path to new path
- Creates parent directories if needed
- Optionally updates `queue.file` and `queue.done_file` config references
- Keeps original file as backup by default
- Can be configured to remove original after migration

**Options:**
| Option | Default | Description |
|--------|---------|-------------|
| `keep_backup` | `true` | Keep original file as backup |
| `update_config` | `true` | Update config file references |

### ReadmeUpdate

Updates the project README to the latest template version.

**Structure:**
```rust
MigrationType::ReadmeUpdate {
    from_version: 1,
    to_version: 2,
}
```

**Behavior:**
- Regenerates `.ralph/README.md` with the latest template
- Preserves custom content (merge-based update)
- Only applicable if current README version < target version

---

## Migration Registry

All migrations are defined in `crates/ralph/src/migration/registry.rs`.

### Registry Structure

```rust
pub static MIGRATIONS: &[Migration] = &[
    Migration {
        id: "config_key_rename_parallel_worktree_root_2026_02",
        description: "Rename parallel.worktree_root to parallel.workspace_root",
        migration_type: MigrationType::ConfigKeyRename {
            old_key: "parallel.worktree_root",
            new_key: "parallel.workspace_root",
        },
    },
    // Add new migrations to the end of this list
];
```

### Migration ID Convention

Format: `<type>_<description>_<YYYY>_<MM>`

Examples:
- `config_key_rename_parallel_worktree_root_2026_02`
- `file_rename_queue_json_2026_01`
- `readme_update_v2_2026_03`

### Adding New Migrations

1. Open `crates/ralph/src/migration/registry.rs`
2. Add a new `Migration` entry to the end of `MIGRATIONS`
3. Use a unique ID following the naming convention
4. Provide a clear description
5. Choose the appropriate `MigrationType`

**Example:**
```rust
Migration {
    id: "config_key_rename_agent_runner_cli_2026_03",
    description: "Rename agent.runner_cli to agent.runner_options",
    migration_type: MigrationType::ConfigKeyRename {
        old_key: "agent.runner_cli",
        new_key: "agent.runner_options",
    },
},
```

---

## History Tracking

Migration history is stored in `.ralph/cache/migrations.json`.

### File Format

```json
{
    "version": 1,
    "applied_migrations": [
        {
            "id": "config_key_rename_parallel_worktree_root_2026_02",
            "applied_at": "2026-02-07T10:30:00Z",
            "migration_type": "ConfigKeyRename { old_key: \"parallel.worktree_root\", new_key: \"parallel.workspace_root\" }"
        }
    ]
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | `number` | Schema version of the history file |
| `applied_migrations` | `array` | List of applied migrations |
| `applied_migrations[].id` | `string` | Migration unique identifier |
| `applied_migrations[].applied_at` | `string` | ISO 8601 timestamp of when migration was applied |
| `applied_migrations[].migration_type` | `string` | Human-readable migration type info |

### History Behavior

- **Auto-created**: History file is created on first migration
- **Version-checked**: Warns if schema version mismatches (attempts to proceed)
- **Atomic writes**: Uses atomic file writes to prevent corruption
- **Git-ignored**: The entire `.ralph/cache/` directory should be gitignored

---

## CLI Commands

### Check for Pending Migrations

```bash
# Default: Show pending migrations
ralph migrate

# Explicit check (exits with code 1 if pending, for CI)
ralph migrate --check
```

**Output Examples:**

No pending migrations:
```
✓ No pending migrations

Your project is up to date!
```

Pending migrations found:
```
Found 1 pending migration(s):

  • config_key_rename_parallel_worktree_root_2026_02
    Rename parallel.worktree_root to parallel.workspace_root

Run ralph migrate --apply to apply them.
```

CI check with pending migrations (exits 1):
```
✗ 1 pending migration(s) found
  - config_key_rename_parallel_worktree_root_2026_02: Rename parallel.worktree_root to parallel.workspace_root

Run ralph migrate --apply to apply them.
```

### List All Migrations

```bash
ralph migrate --list
```

**Output Example:**
```
Available migrations:

  ✓ config_key_rename_parallel_worktree_root_2026_02 (applied)
    Rename parallel.worktree_root to parallel.workspace_root

1 applied, 0 pending, 0 not applicable
```

Status icons:
- `✓` (green) - Applied
- `○` (yellow) - Pending/applicable
- `-` (dimmed) - Not applicable

### Apply Migrations

```bash
# Apply all pending migrations (interactive confirmation)
ralph migrate --apply

# Force re-apply already applied migrations (dangerous)
ralph migrate --apply --force
```

**Interactive Flow:**
```
Will apply 1 migration(s):

  - config_key_rename_parallel_worktree_root_2026_02: Rename parallel.worktree_root to parallel.workspace_root

Apply these migrations? [y/N]: y

✓ Successfully applied 1 migration(s)
  ✓ config_key_rename_parallel_worktree_root_2026_02
```

### Show Detailed Status

```bash
ralph migrate status
```

**Output Example:**
```
Migration Status

History:
  Location: /path/to/project/.ralph/cache/migrations.json
  Applied migrations: 1

Pending migrations: None
```

### Help

```bash
ralph migrate --help
```

**Output:**
```
Check and apply migrations for config and project files

Usage: ralph migrate [OPTIONS] [COMMAND]

Commands:
  status  Show detailed migration status
  help    Print this message or the help of the given subcommand(s)

Options:
      --check    Check for pending migrations without applying them (exit 1 if any pending)
      --apply    Apply pending migrations
      --list     List all migrations and their status
      --force    Force apply migrations even if already applied (dangerous)
  -h, --help     Print help

Examples:
  ralph migrate              # Check for pending migrations
  ralph migrate --check      # Exit with error code if migrations pending (CI)
  ralph migrate --apply      # Apply all pending migrations
  ralph migrate --list       # List all migrations and their status
  ralph migrate status       # Show detailed migration status
```

---

## Automatic Migrations

### When Do Automatic Migrations Run?

Currently, Ralph **does not** automatically apply migrations. This is by design to prevent unexpected changes during routine operations.

Migrations are only applied:
1. When explicitly requested via `ralph migrate --apply`
2. During project initialization (if applicable)

### Future Considerations

Potential future enhancements may include:
- Prompting for migration during `ralph init`
- `--auto-migrate` flag for CI environments
- Migration warnings on config load failure

### Best Practices

1. **Check after updates**: Run `ralph migrate` after updating Ralph to check for pending migrations
2. **CI integration**: Use `ralph migrate --check` in CI to fail builds if migrations are needed
3. **Version control**: Review migration changes before committing

---

## Manual Migrations

### Step-by-Step Migration

1. **Check current status:**
   ```bash
   ralph migrate
   ```

2. **Review what will change:**
   ```bash
   ralph migrate --list
   ```

3. **Apply migrations:**
   ```bash
   ralph migrate --apply
   ```

4. **Verify changes:**
   ```bash
   git diff  # or your preferred diff tool
   ```

5. **Commit changes:**
   ```bash
   git add -A
   git commit -m "Apply Ralph migrations"
   ```

### Rollback Procedure

If a migration causes issues, you can manually rollback:

**For ConfigKeyRename:**
```bash
# Manually edit the config file to rename the key back
# Or restore from version control
git checkout .ralph/config.jsonc
```

**For FileRename:**
```bash
# Remove the new file
rm .ralph/queue.jsonc

# Restore the original (if backup was kept)
# Original is kept by default, so just remove the new one
```

**Clear migration history (advanced):**
```bash
# Remove the history entry for the migration
# WARNING: Only do this if you understand the implications
rm .ralph/cache/migrations.json
```

### CI Integration

Add to your CI pipeline to ensure migrations are applied:

```yaml
# GitHub Actions example
- name: Check Ralph migrations
  run: ralph migrate --check
```

```bash
# Generic CI script
if ! ralph migrate --check; then
    echo "Error: Pending Ralph migrations detected"
    echo "Run 'ralph migrate --apply' locally and commit the changes"
    exit 1
fi
```

---

## Breaking Changes Reference

### Known Migrations

#### `config_key_rename_parallel_worktree_root_2026_02`

**Breaking Change:** The `parallel.worktree_root` config key was renamed to `parallel.workspace_root`.

**When:** February 2026

**Impact:** Config files using the old key will fail to load correctly.

**Migration:**
```rust
MigrationType::ConfigKeyRename {
    old_key: "parallel.worktree_root",
    new_key: "parallel.workspace_root",
}
```

**Before:**
```json
{
    "version": 1,
    "parallel": {
        "worktree_root": "/path/to/workspaces"
    }
}
```

**After:**
```json
{
    "version": 1,
    "parallel": {
        "workspace_root": "/path/to/workspaces"
    }
}
```

**Documentation Reference:** See `docs/configuration.md` line 298-301:
> Breaking change (2026-02): The `parallel.worktree_root` config key has been renamed to `parallel.workspace_root`. Config files using the old key will fail to load. Run `ralph migrate` to update existing configs.

### State Files Note

> **Important:** State files are not migrated and may need to be deleted if incompatible. For example, parallel mode state files (`.ralph/cache/parallel/state.json`) may contain references to old config keys.

---

## Implementation Details

### Module Structure

```
crates/ralph/src/migration/
├── mod.rs              # Core migration logic, context, apply/check functions
├── registry.rs         # Migration definitions registry
├── history.rs          # Migration history persistence
├── config_migrations.rs # Config key rename implementation
└── file_migrations.rs  # File rename/move implementation
```

### Key Types

**Migration:**
```rust
pub struct Migration {
    pub id: &'static str,
    pub description: &'static str,
    pub migration_type: MigrationType,
}
```

**MigrationType:**
```rust
pub enum MigrationType {
    ConfigKeyRename { old_key: &'static str, new_key: &'static str },
    FileRename { old_path: &'static str, new_path: &'static str },
    ReadmeUpdate { from_version: u32, to_version: u32 },
}
```

**MigrationContext:**
```rust
pub struct MigrationContext {
    pub repo_root: PathBuf,
    pub project_config_path: PathBuf,
    pub global_config_path: Option<PathBuf>,
    pub resolved_config: Config,
    pub migration_history: MigrationHistory,
}
```

### Invariants and Assumptions

1. **Idempotency**: Running a migration twice is safe
2. **Ordering**: Migrations in the registry are ordered chronologically (oldest first)
3. **Uniqueness**: Each migration has a unique ID that never changes
4. **Applicability**: A migration is only applied if it's applicable in the current context
5. **JSONC Preservation**: Config migrations preserve comments using text-based replacement
6. **Scoped Renames**: Config key renames are scoped to their parent object path

### Testing

Migration functionality includes comprehensive tests:

- Unit tests in each module (`#[cfg(test)]`)
- Integration tests for full migration workflows
- CLI tests for command handling

Run tests:
```bash
cargo test -p ralph migration
```

### Safety Features

1. **Atomic writes**: All file modifications use atomic write operations
2. **Backup preservation**: Original files kept as backups during file migrations
3. **Applicability checking**: Migrations only run if they apply to the current state
4. **History validation**: Migration history schema version is checked
5. **Confirmation prompts**: `--apply` requires user confirmation (unless `--force`)

---

## Related Documentation

- [Configuration](../configuration.md) - Config file format and options
- [Parallel Execution](./parallel.md) - Parallel mode (includes `workspace_root` setting)
- [CLI](../cli.md) - General CLI documentation
