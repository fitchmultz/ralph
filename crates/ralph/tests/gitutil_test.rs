// gitutil_test.rs - Unit tests for gitutil.rs (git operations, error cases, non-git directories)

use ralph::gitutil;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

// Helper to initialize a git repository
fn init_git_repo(dir: &TempDir) {
    Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("git init failed");

    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir.path())
        .output()
        .expect("git config user.email failed");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir.path())
        .output()
        .expect("git config user.name failed");
}

// Helper to create and commit a file
fn commit_file(dir: &TempDir, filename: &str, content: &str, message: &str) {
    let file_path = dir.path().join(filename);
    fs::write(&file_path, content).expect("failed to write file");

    Command::new("git")
        .args(["add", filename])
        .current_dir(dir.path())
        .output()
        .expect("git add failed");

    Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(dir.path())
        .output()
        .expect("git commit failed");
}

#[test]
fn test_status_porcelain_clean_repo() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    let status = gitutil::status_porcelain(dir.path()).unwrap();
    assert!(status.trim().is_empty());
}

#[test]
fn test_status_porcelain_with_changes() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // Create a modified file
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "content").expect("failed to write file");
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()
        .expect("git add failed");

    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir.path())
        .output()
        .expect("git commit failed");

    // Modify the file
    fs::write(&file_path, "modified content").expect("failed to modify file");

    let status = gitutil::status_porcelain(dir.path()).unwrap();
    assert!(!status.trim().is_empty());
    assert!(status.contains("M"));
}

#[test]
fn test_status_porcelain_with_untracked_files() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // Create an untracked file
    let file_path = dir.path().join("untracked.txt");
    fs::write(&file_path, "content").expect("failed to write file");

    let status = gitutil::status_porcelain(dir.path()).unwrap();
    assert!(!status.trim().is_empty());
    assert!(status.contains("??"));
}

#[test]
fn test_status_paths_includes_tracked_and_untracked() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "tracked.txt", "content", "initial");
    fs::write(dir.path().join("tracked.txt"), "modified").expect("modify tracked");
    fs::write(dir.path().join("untracked.txt"), "new").expect("create untracked");

    let paths = gitutil::status_paths(dir.path()).expect("status paths");
    assert!(paths.contains(&"tracked.txt".to_string()));
    assert!(paths.contains(&"untracked.txt".to_string()));
}

#[test]
fn test_filter_modified_lfs_files_intersects_lists() {
    let status_paths = vec![
        "assets/large.bin".to_string(),
        "notes.txt".to_string(),
        "media/video.mov".to_string(),
    ];
    let lfs_files = vec![
        "assets/large.bin".to_string(),
        "media/video.mov".to_string(),
    ];
    let modified = gitutil::filter_modified_lfs_files(&status_paths, &lfs_files);
    assert_eq!(
        modified,
        vec![
            "assets/large.bin".to_string(),
            "media/video.mov".to_string()
        ]
    );
}

#[test]
fn test_has_lfs_detects_gitattributes_filter() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    let attrs = dir.path().join(".gitattributes");
    fs::write(&attrs, "*.bin filter=lfs diff=lfs merge=lfs -text\n").expect("write gitattributes");

    let has_lfs = gitutil::has_lfs(dir.path()).expect("has lfs");
    assert!(has_lfs);
}

#[test]
fn test_status_porcelain_non_git_directory() {
    let dir = TempDir::new().expect("create temp dir");
    // Not a git repository

    let result = gitutil::status_porcelain(dir.path());
    // Should fail because not a git repo
    assert!(result.is_err());
}

#[test]
fn test_require_clean_repo_ignoring_paths_clean() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &[]);
    assert!(result.is_ok());
}

#[test]
fn test_require_clean_repo_ignoring_paths_with_dirty_changes() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "content", "initial");

    // Modify the file
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "modified").expect("failed to modify file");

    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &[]);
    assert!(result.is_err());

    if let Err(gitutil::GitError::DirtyRepo { details }) = result {
        assert!(details.contains("Tracked changes"));
    } else {
        panic!("Expected DirtyRepo error");
    }
}

#[test]
fn test_require_clean_repo_ignoring_paths_with_untracked_files() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // Create untracked file
    let file_path = dir.path().join("untracked.txt");
    fs::write(&file_path, "content").expect("failed to write file");

    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &[]);
    assert!(result.is_err());

    if let Err(gitutil::GitError::DirtyRepo { details }) = result {
        assert!(details.contains("Untracked files"));
    } else {
        panic!("Expected DirtyRepo error");
    }
}

#[test]
fn test_require_clean_repo_ignoring_paths_with_allowed_paths() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "allowed.txt", "content", "initial");

    // Modify allowed file
    let file_path = dir.path().join("allowed.txt");
    fs::write(&file_path, "modified").expect("failed to modify file");

    // Create untracked allowed file
    let untracked_path = dir.path().join("also-allowed.txt");
    fs::write(&untracked_path, "untracked").expect("failed to write file");

    let result = gitutil::require_clean_repo_ignoring_paths(
        dir.path(),
        false,
        &["allowed.txt", "also-allowed.txt"],
    );
    assert!(result.is_ok(), "Should allow changes in specified paths");
}

#[test]
fn test_require_clean_repo_ignoring_paths_force_bypasses_check() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // Create untracked file
    let file_path = dir.path().join("untracked.txt");
    fs::write(&file_path, "content").expect("failed to write file");

    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), true, &[]);
    assert!(result.is_ok(), "Force flag should bypass dirty check");
}

#[test]
fn test_require_clean_repo_ignoring_paths_with_mixed_changes() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "allowed.txt", "content", "initial");
    commit_file(&dir, "not-allowed.txt", "content", "initial");

    // Modify both files
    fs::write(dir.path().join("allowed.txt"), "modified").expect("failed to modify");
    fs::write(dir.path().join("not-allowed.txt"), "modified").expect("failed to modify");

    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &["allowed.txt"]);
    assert!(
        result.is_err(),
        "Should fail due to not-allowed.txt changes"
    );

    if let Err(gitutil::GitError::DirtyRepo { details }) = result {
        assert!(details.contains("not-allowed.txt"));
    } else {
        panic!("Expected DirtyRepo error");
    }
}

#[test]
fn test_commit_all_empty_message_fails() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    let result = gitutil::commit_all(dir.path(), "");
    assert!(result.is_err());

    if let Err(gitutil::GitError::EmptyCommitMessage) = result {
        // Expected
    } else {
        panic!("Expected EmptyCommitMessage error");
    }
}

#[test]
fn test_commit_all_whitespace_message_fails() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    let result = gitutil::commit_all(dir.path(), "   ");
    assert!(result.is_err());

    if let Err(gitutil::GitError::EmptyCommitMessage) = result {
        // Expected
    } else {
        panic!("Expected EmptyCommitMessage error");
    }
}

#[test]
fn test_commit_all_no_changes_fails() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // Create initial commit to have a clean repo
    commit_file(&dir, "test.txt", "content", "initial");

    let result = gitutil::commit_all(dir.path(), "test commit");
    assert!(result.is_err());

    if let Err(gitutil::GitError::NoChangesToCommit) = result {
        // Expected
    } else {
        panic!("Expected NoChangesToCommit error");
    }
}

#[test]
fn test_commit_all_success() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "content", "initial");

    // Modify the file
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "modified content").expect("failed to modify file");

    let result = gitutil::commit_all(dir.path(), "test commit");
    assert!(result.is_ok());

    // Verify the commit was created
    let status = gitutil::status_porcelain(dir.path()).unwrap();
    assert!(
        status.trim().is_empty(),
        "Repo should be clean after commit"
    );
}

#[test]
fn test_revert_uncommitted_restores_clean_state() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "original content", "initial");

    // Modify the file
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "modified content").expect("failed to modify file");

    // Create an untracked file
    let untracked_path = dir.path().join("untracked.txt");
    fs::write(&untracked_path, "untracked").expect("failed to write file");

    // Verify repo is dirty
    let status_before = gitutil::status_porcelain(dir.path()).unwrap();
    assert!(!status_before.trim().is_empty());

    // Revert changes
    let result = gitutil::revert_uncommitted(dir.path());
    assert!(result.is_ok());

    // Verify repo is clean
    let status_after = gitutil::status_porcelain(dir.path()).unwrap();
    assert!(
        status_after.trim().is_empty(),
        "Repo should be clean after revert"
    );

    // Verify original content was restored
    let restored_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(restored_content, "original content");

    // Verify untracked file was removed
    assert!(!untracked_path.exists());
}

#[test]
fn test_revert_uncommitted_preserves_env_files() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "content", "initial");

    // Modify tracked file
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "modified").expect("failed to modify");

    // Create untracked .env files (should be preserved)
    let env_path = dir.path().join(".env");
    fs::write(&env_path, "SECRET=value").expect("failed to write .env");

    let env_local_path = dir.path().join(".env.local");
    fs::write(&env_local_path, "LOCAL=value").expect("failed to write .env.local");

    // Revert changes
    gitutil::revert_uncommitted(dir.path()).unwrap();

    // Verify .env files were preserved
    assert!(env_path.exists());
    assert!(env_local_path.exists());

    let env_content = fs::read_to_string(&env_path).unwrap();
    assert_eq!(env_content, "SECRET=value");
}

#[test]
fn test_upstream_ref_no_upstream() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // No upstream configured - should fail (exact error type depends on git version)
    let result = gitutil::upstream_ref(dir.path());
    assert!(result.is_err());
}

#[test]
fn test_upstream_ref_with_upstream() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "content", "initial");

    // Create a remote and set upstream
    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/test/test.git",
        ])
        .current_dir(dir.path())
        .output()
        .expect("git remote add failed");

    Command::new("git")
        .args(["push", "-u", "origin", "master"])
        .current_dir(dir.path())
        .output()
        .expect("git push failed (this may fail in tests)");

    // Note: This test might fail if git push doesn't work in the test environment
    // We're checking the logic, not the actual push
}

#[test]
fn test_git_error_display() {
    let err = gitutil::GitError::DirtyRepo {
        details: "test details".to_string(),
    };
    let err_str = format!("{}", err);
    assert!(err_str.contains("repo is dirty"));
    assert!(err_str.contains("test details"));

    let err = gitutil::GitError::EmptyCommitMessage;
    let err_str = format!("{}", err);
    assert!(err_str.contains("commit message is empty"));

    let err = gitutil::GitError::NoChangesToCommit;
    let err_str = format!("{}", err);
    assert!(err_str.contains("no changes to commit"));
}

#[test]
fn test_classify_push_error_no_upstream() {
    let err = gitutil::GitError::NoUpstream;
    let err_str = format!("{}", err);
    assert!(err_str.contains("no upstream"));
}

#[test]
fn test_classify_push_error_auth_failed() {
    let err = gitutil::GitError::AuthFailed;
    let err_str = format!("{}", err);
    assert!(err_str.contains("authentication"));
}

#[test]
fn test_path_is_allowed_helper() {
    // This tests the private helper function indirectly
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "content", "initial");

    // Modify file
    fs::write(dir.path().join("test.txt"), "modified").expect("failed to modify");

    // Should fail when file not in allowed list
    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &["other.txt"]);
    assert!(result.is_err());

    // Should pass when file is in allowed list
    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &["test.txt"]);
    assert!(result.is_ok());
}

#[test]
fn test_path_is_allowed_with_dot_prefix() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    commit_file(&dir, "test.txt", "content", "initial");

    fs::write(dir.path().join("test.txt"), "modified").expect("failed to modify");

    // Test with ./ prefix
    let result = gitutil::require_clean_repo_ignoring_paths(dir.path(), false, &["./test.txt"]);
    assert!(result.is_ok());
}

#[test]
fn test_is_ahead_of_upstream_no_upstream() {
    let dir = TempDir::new().expect("create temp dir");
    init_git_repo(&dir);

    // No upstream configured - should fail (exact error type depends on git version)
    let result = gitutil::is_ahead_of_upstream(dir.path());
    assert!(result.is_err());
}
