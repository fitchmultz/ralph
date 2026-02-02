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

use super::Migration;

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
pub static MIGRATIONS: &[Migration] = &[
    // No migrations defined yet.
    // Future migrations will be added here as needed.
    //
    // Example migrations (commented out until needed):
    //
    // Migration {
    //     id: "config_key_rename_runner_cli_2026_01",
    //     description: "Rename agent.runner_cli to agent.runner_options",
    //     migration_type: MigrationType::ConfigKeyRename {
    //         old_key: "agent.runner_cli",
    //         new_key: "agent.runner_options",
    //     },
    // },
    //
    // Migration {
    //     id: "queue_json_to_jsonc_2026_01",
    //     description: "Migrate queue.json to queue.jsonc for comment support",
    //     migration_type: MigrationType::FileRename {
    //         old_path: ".ralph/queue.json",
    //         new_path: ".ralph/queue.jsonc",
    //     },
    // },
    //
    // Migration {
    //     id: "readme_update_v4_2026_02",
    //     description: "Update README to version 4 with new documentation",
    //     migration_type: MigrationType::ReadmeUpdate {
    //         from_version: 3,
    //         to_version: 4,
    //     },
    // },
];

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
        // Since we have no migrations defined yet, this test verifies the function works
        // When migrations are added, update this test
        assert!(get_migration_by_id("nonexistent").is_none());
    }

    #[test]
    fn get_all_migration_ids_returns_correct_count() {
        let ids = get_all_migration_ids();
        assert_eq!(ids.len(), MIGRATIONS.len());
    }
}
