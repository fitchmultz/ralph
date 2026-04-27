//! Bookkeeping path sync tests for parallel workspace state synchronization.
//!
//! Purpose:
//! - Bookkeeping path sync tests for parallel workspace state synchronization.
//!
//! Responsibilities:
//! - Verify custom queue/done paths are mapped into worker workspaces.
//! - Verify missing bookkeeping files remain a no-op when allowed.
//! - Verify `.jsonc` migration seeding behavior for uncommitted local config states.
//!
//! Non-scope:
//! - `.ralph` runtime-tree traversal coverage.
//! - Gitignored allowlist filtering rules.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Test names and assertions match the prior flat suite exactly.
//! - Queue/done expectations are asserted from on-disk workspace state.

use super::*;

#[test]
fn sync_ralph_state_custom_queue_done_paths_are_synced() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    let queue_path = repo_root.join("queue/active.json");
    let done_path = repo_root.join("archive/done.json");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::create_dir_all(done_path.parent().unwrap())?;
    fs::write(&queue_path, "{custom_queue}")?;
    fs::write(&done_path, "{custom_done}")?;

    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
    fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;
    fs::create_dir_all(&workspace_root)?;

    let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("queue/active.json"))?,
        "{custom_queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join("archive/done.json"))?,
        "{custom_done}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/config.json"))?,
        "{config}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
        "prompt"
    );
    Ok(())
}

#[test]
fn sync_ralph_state_custom_queue_done_paths_inside_ralph_are_synced() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    let queue_path = repo_root.join(".ralph/data/queue.jsonc");
    let done_path = repo_root.join(".ralph/data/done.json");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::write(&queue_path, "{custom_queue}")?;
    fs::write(&done_path, "{custom_done}")?;

    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    fs::write(repo_root.join(".ralph/config.jsonc"), "{config}")?;
    fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;
    fs::create_dir_all(&workspace_root)?;

    let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/data/queue.jsonc"))?,
        "{custom_queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/data/done.json"))?,
        "{custom_done}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/config.jsonc"))?,
        "{config}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
        "prompt"
    );
    Ok(())
}

#[test]
fn sync_ralph_state_missing_done_file_allowed() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    let queue_path = repo_root.join("queue/active.json");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::write(&queue_path, "{queue}")?;

    fs::create_dir_all(repo_root.join(".ralph"))?;
    fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
    fs::create_dir_all(&workspace_root)?;

    let done_path = repo_root.join("archive/done.json");
    let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("queue/active.json"))?,
        "{queue}"
    );
    assert!(!workspace_root.join("archive/done.json").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_seeds_jsonc_bookkeeping_for_migrated_uncommitted_config() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;

    fs::create_dir_all(repo_root.join(".ralph"))?;
    fs::write(repo_root.join(".ralph/queue.json"), "{legacy_queue}")?;
    fs::write(repo_root.join(".ralph/done.json"), "{legacy_done}")?;
    fs::write(repo_root.join(".ralph/queue.jsonc"), "{migrated_queue}")?;
    fs::write(repo_root.join(".ralph/done.jsonc"), "{migrated_done}")?;
    fs::write(repo_root.join(".ralph/config.jsonc"), "{config}")?;

    fs::create_dir_all(workspace_root.join(".ralph"))?;
    fs::write(
        workspace_root.join(".ralph/queue.json"),
        "{legacy_workspace_queue}",
    )?;
    fs::write(
        workspace_root.join(".ralph/done.json"),
        "{legacy_workspace_done}",
    )?;

    let resolved = build_test_resolved(
        &repo_root,
        Some(repo_root.join(".ralph/queue.jsonc")),
        Some(repo_root.join(".ralph/done.jsonc")),
    );
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/queue.jsonc"))?,
        "{migrated_queue}"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join(".ralph/done.jsonc"))?,
        "{migrated_done}"
    );
    assert!(workspace_root.join(".ralph/queue.json").exists());
    assert!(workspace_root.join(".ralph/done.json").exists());

    Ok(())
}

#[test]
fn sync_worker_bookkeeping_back_to_source_mirrors_ignored_default_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::write(
        repo_root.join(".gitignore"),
        ".ralph/queue.jsonc\n.ralph/done.jsonc\n",
    )?;
    git_test::git_run(&repo_root, &["add", ".gitignore"])?;
    git_test::git_run(&repo_root, &["commit", "-m", "ignore bookkeeping"])?;

    fs::write(repo_root.join(".ralph/queue.jsonc"), "{stale_queue}")?;
    fs::write(repo_root.join(".ralph/done.jsonc"), "{stale_done}")?;
    fs::create_dir_all(workspace_root.join(".ralph"))?;
    fs::write(workspace_root.join(".ralph/queue.jsonc"), "{fresh_queue}")?;
    fs::write(workspace_root.join(".ralph/done.jsonc"), "{fresh_done}")?;

    let resolved = build_test_resolved(
        &repo_root,
        Some(repo_root.join(".ralph/queue.jsonc")),
        Some(repo_root.join(".ralph/done.jsonc")),
    );
    sync_worker_bookkeeping_back_to_source(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(repo_root.join(".ralph/queue.jsonc"))?,
        "{fresh_queue}"
    );
    assert_eq!(
        fs::read_to_string(repo_root.join(".ralph/done.jsonc"))?,
        "{fresh_done}"
    );
    Ok(())
}

#[test]
fn sync_worker_bookkeeping_back_to_source_mirrors_untracked_custom_paths() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::write(repo_root.join("README.md"), "tracked baseline")?;
    git_test::git_run(&repo_root, &["add", "README.md"])?;
    git_test::git_run(&repo_root, &["commit", "-m", "baseline"])?;

    let queue_path = repo_root.join("queue/active.jsonc");
    let done_path = repo_root.join("archive/done.jsonc");
    fs::create_dir_all(queue_path.parent().unwrap())?;
    fs::create_dir_all(done_path.parent().unwrap())?;
    fs::write(&queue_path, "{stale_queue}")?;
    fs::write(&done_path, "{stale_done}")?;
    fs::create_dir_all(workspace_root.join("queue"))?;
    fs::create_dir_all(workspace_root.join("archive"))?;
    fs::write(workspace_root.join("queue/active.jsonc"), "{fresh_queue}")?;
    fs::write(workspace_root.join("archive/done.jsonc"), "{fresh_done}")?;

    let resolved = build_test_resolved(
        &repo_root,
        Some(queue_path.clone()),
        Some(done_path.clone()),
    );
    sync_worker_bookkeeping_back_to_source(&resolved, &workspace_root)?;

    assert_eq!(fs::read_to_string(queue_path)?, "{fresh_queue}");
    assert_eq!(fs::read_to_string(done_path)?, "{fresh_done}");
    Ok(())
}

#[test]
fn sync_worker_bookkeeping_back_to_source_keeps_tracked_paths_git_authoritative() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::write(repo_root.join(".ralph/queue.jsonc"), "{tracked_queue}")?;
    fs::write(repo_root.join(".ralph/done.jsonc"), "{tracked_done}")?;
    git_test::git_run(
        &repo_root,
        &["add", "-f", ".ralph/queue.jsonc", ".ralph/done.jsonc"],
    )?;
    git_test::git_run(&repo_root, &["commit", "-m", "track bookkeeping"])?;

    fs::create_dir_all(workspace_root.join(".ralph"))?;
    fs::write(workspace_root.join(".ralph/queue.jsonc"), "{worker_queue}")?;
    fs::write(workspace_root.join(".ralph/done.jsonc"), "{worker_done}")?;

    let resolved = build_test_resolved(
        &repo_root,
        Some(repo_root.join(".ralph/queue.jsonc")),
        Some(repo_root.join(".ralph/done.jsonc")),
    );
    sync_worker_bookkeeping_back_to_source(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(repo_root.join(".ralph/queue.jsonc"))?,
        "{tracked_queue}"
    );
    assert_eq!(
        fs::read_to_string(repo_root.join(".ralph/done.jsonc"))?,
        "{tracked_done}"
    );
    Ok(())
}
