//! Purpose: Regression coverage for migration orchestration.
//!
//! Responsibilities:
//! - Verify top-level migration applicability behavior stays stable across
//!   refactors.
//! - Exercise orchestration using the public `crate::migration` API.
//!
//! Scope:
//! - Migration root behavior only; type and context tests live in `types.rs`.
//!
//! Usage:
//! - Compiled only for `cargo test` via `migration::tests`.
//!
//! Invariants/Assumptions:
//! - Cleanup migrations remain pending when legacy JSON files still exist beside
//!   their JSONC successors.

use super::{MigrationCheckResult, MigrationContext, check_migrations, history};
use tempfile::TempDir;

fn create_test_context(dir: &TempDir) -> MigrationContext {
    let repo_root = dir.path().to_path_buf();
    let project_config_path = repo_root.join(".ralph/config.jsonc");

    MigrationContext {
        repo_root,
        project_config_path,
        global_config_path: None,
        resolved_config: crate::contracts::Config::default(),
        migration_history: history::MigrationHistory::default(),
    }
}

#[test]
fn cleanup_migration_pending_when_legacy_json_remains_after_rename_migration() {
    let dir = TempDir::new().unwrap();
    let mut ctx = create_test_context(&dir);

    std::fs::create_dir_all(dir.path().join(".ralph")).unwrap();
    std::fs::write(dir.path().join(".ralph/queue.json"), "{}").unwrap();
    std::fs::write(dir.path().join(".ralph/queue.jsonc"), "{}").unwrap();

    ctx.migration_history
        .applied_migrations
        .push(history::AppliedMigration {
            id: "file_rename_queue_json_to_jsonc_2026_02".to_string(),
            applied_at: chrono::Utc::now(),
            migration_type: "FileRename".to_string(),
        });

    let pending = match check_migrations(&ctx).expect("check migrations") {
        MigrationCheckResult::Pending(pending) => pending,
        MigrationCheckResult::Current => panic!("expected pending cleanup migration"),
    };

    let pending_ids: Vec<&str> = pending.iter().map(|migration| migration.id).collect();
    assert!(
        pending_ids.contains(&"file_cleanup_legacy_queue_json_after_jsonc_2026_02"),
        "expected cleanup migration to be pending when legacy queue.json remains"
    );
}
