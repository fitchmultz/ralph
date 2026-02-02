//! Migration registry containing all defined migrations.
//!
//! Responsibilities:
//! - Define the static list of all migrations in chronological order.
//! - Provide a central place to register new migrations.
//!
//! Not handled here:
//! - Migration execution logic (see `super::mod.rs`).
//! - Individual migration implementations (see `config_migrations.rs`, `file_migrations.rs`).
//!
//! Invariants/assumptions:
//! - Migrations are ordered chronologically (oldest first).
//! - Each migration has a unique ID that never changes.
//! - New migrations are appended to the end of the list.

use super::{Migration, MigrationType};

/// The static registry of all migrations.
///
/// Add new migrations to the end of this list. Each migration should have:
/// - A unique ID (convention: `<type>_<description>_<YYYY>_<MM>`)
/// - A clear description of what it does
/// - The appropriate MigrationType
///
/// Example:
/// ```rust,ignore
/// use ralph::migration::{Migration, MigrationType};
///
/// pub static MIGRATIONS: &[Migration] = &[
///     Migration {
///         id: "config_key_rename_2026_02",
///         description: "Rename agent.runner_cli to agent.runner_options",
///         migration_type: MigrationType::ConfigKeyRename {
///             old_key: "agent.runner_cli",
///             new_key: "agent.runner_options",
///         },
///     },
/// ];
/// ```
pub static MIGRATIONS: &[Migration] = &[Migration {
    id: "config_key_rename_parallel_worktree_root_2026_02",
    description: "Rename parallel.worktree_root to parallel.workspace_root",
    migration_type: MigrationType::ConfigKeyRename {
        old_key: "parallel.worktree_root",
        new_key: "parallel.workspace_root",
    },
}];

/// Get a migration by its ID.
pub fn get_migration_by_id(id: &str) -> Option<&'static Migration> {
    MIGRATIONS.iter().find(|m| m.id == id)
}

/// Get all migration IDs.
pub fn get_all_migration_ids() -> Vec<&'static str> {
    MIGRATIONS.iter().map(|m| m.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_ids_are_unique() {
        let ids: Vec<&str> = MIGRATIONS.iter().map(|m| m.id).collect();
        let unique_ids: std::collections::HashSet<&str> = ids.iter().cloned().collect();

        assert_eq!(ids.len(), unique_ids.len(), "Migration IDs must be unique");
    }

    #[test]
    fn get_migration_by_id_finds_existing() {
        assert!(get_migration_by_id("config_key_rename_parallel_worktree_root_2026_02").is_some());
        assert!(get_migration_by_id("nonexistent").is_none());
    }

    #[test]
    fn get_all_migration_ids_returns_correct_count() {
        let ids = get_all_migration_ids();
        assert_eq!(ids.len(), MIGRATIONS.len());
    }
}
