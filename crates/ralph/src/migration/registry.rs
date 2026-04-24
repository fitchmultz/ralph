//! Migration registry containing all defined migrations.
//!
//! Purpose:
//! - Migration registry containing all defined migrations.
//!
//! Responsibilities:
//! - Define the static list of all migrations in chronological order.
//! - Provide a central place to register new migrations.
//!
//! Not handled here:
//! - Migration execution logic (see `super::mod.rs`).
//! - Individual migration implementations (see `config_migrations/`, `file_migrations/`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
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
pub static MIGRATIONS: &[Migration] = &[
    Migration {
        id: "config_key_rename_parallel_worktree_root_2026_02",
        description: "Rename parallel.worktree_root to parallel.workspace_root",
        migration_type: MigrationType::ConfigKeyRename {
            old_key: "parallel.worktree_root",
            new_key: "parallel.workspace_root",
        },
    },
    Migration {
        id: "config_key_remove_agent_update_task_before_run_2026_02",
        description: "Remove deprecated agent.update_task_before_run key",
        migration_type: MigrationType::ConfigKeyRemove {
            key: "agent.update_task_before_run",
        },
    },
    Migration {
        id: "config_key_remove_agent_fail_on_prerun_update_error_2026_02",
        description: "Remove deprecated agent.fail_on_prerun_update_error key",
        migration_type: MigrationType::ConfigKeyRemove {
            key: "agent.fail_on_prerun_update_error",
        },
    },
    Migration {
        id: "file_rename_queue_json_to_jsonc_2026_02",
        description: "Migrate queue.json to queue.jsonc for JSONC comment support and remove legacy queue.json",
        migration_type: MigrationType::FileRename {
            old_path: ".ralph/queue.json",
            new_path: ".ralph/queue.jsonc",
        },
    },
    Migration {
        id: "file_rename_done_json_to_jsonc_2026_02",
        description: "Migrate done.json to done.jsonc for JSONC comment support and remove legacy done.json",
        migration_type: MigrationType::FileRename {
            old_path: ".ralph/done.json",
            new_path: ".ralph/done.jsonc",
        },
    },
    Migration {
        id: "file_rename_config_json_to_jsonc_2026_02",
        description: "Migrate config.json to config.jsonc for JSONC comment support and remove legacy config.json",
        migration_type: MigrationType::FileRename {
            old_path: ".ralph/config.json",
            new_path: ".ralph/config.jsonc",
        },
    },
    Migration {
        id: "file_cleanup_legacy_queue_json_after_jsonc_2026_02",
        description: "Remove legacy queue.json when queue.jsonc already exists",
        migration_type: MigrationType::FileRename {
            old_path: ".ralph/queue.json",
            new_path: ".ralph/queue.jsonc",
        },
    },
    Migration {
        id: "file_cleanup_legacy_done_json_after_jsonc_2026_02",
        description: "Remove legacy done.json when done.jsonc already exists",
        migration_type: MigrationType::FileRename {
            old_path: ".ralph/done.json",
            new_path: ".ralph/done.jsonc",
        },
    },
    Migration {
        id: "config_ci_gate_rewrite_2026_03",
        description: "Rewrite legacy agent.ci_gate_command/ci_gate_enabled into structured agent.ci_gate config",
        migration_type: MigrationType::ConfigCiGateRewrite,
    },
    Migration {
        id: "config_legacy_contract_upgrade_2026_03",
        description: "Upgrade legacy config version markers and git_commit_push_enabled to the 0.3 contract",
        migration_type: MigrationType::ConfigLegacyContractUpgrade,
    },
    Migration {
        id: "file_cleanup_legacy_config_json_after_jsonc_2026_02",
        description: "Remove legacy config.json when config.jsonc already exists",
        migration_type: MigrationType::FileRename {
            old_path: ".ralph/config.json",
            new_path: ".ralph/config.jsonc",
        },
    },
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
        assert!(get_migration_by_id("config_key_rename_parallel_worktree_root_2026_02").is_some());
        assert!(get_migration_by_id("nonexistent").is_none());
    }

    #[test]
    fn get_all_migration_ids_returns_correct_count() {
        let ids = get_all_migration_ids();
        assert_eq!(ids.len(), MIGRATIONS.len());
    }
}
