//! Release policy shell contracts (`release_policy.sh` and related scripts).
//!
//! Responsibilities:
//! - Dirty-path collection/validation and release cleanliness script structure.

use std::process::Command;

use super::support::{
    break_git_index, copy_pre_public_check_fixture, copy_repo_file, init_git_repo, read_repo_file,
};

#[test]
fn release_policy_rejects_git_status_collection_failures() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    init_git_repo(repo_root);
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");
    break_git_index(repo_root);

    let shell = format!(
        "SCRIPT_DIR={script_dir:?}\nREPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\nrelease_collect_dirty_lines \"$REPO_ROOT\"\n",
        script_dir = repo_root.join("scripts"),
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run release dirty collection with broken git status");

    assert!(
        !output.status.success(),
        "release dirty collection should fail closed when git status fails\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("git status --porcelain=v1 -z failed"),
        "git status collection failure should be reported\noutput:\n{}",
        combined
    );
}

#[test]
fn release_policy_rejects_path_validator_failures() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);

    let shell = format!(
        "SCRIPT_DIR={script_dir:?}\nREPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\nrelease_path_has_control_characters() {{ return 7; }}\nrelease_require_safe_publication_path 'Fixture' 'safe-path.txt'\n",
        script_dir = repo_root.join("scripts"),
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run release path validation with injected validator failure");

    assert!(
        !output.status.success(),
        "release path validation should fail closed when the validator helper errors\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("path validation failed") && combined.contains("safe-path.txt"),
        "validator failure rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn release_policy_rejects_rename_from_disallowed_path_to_release_metadata() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    for relative_path in [
        "scripts/lib/ralph-shell.sh",
        "scripts/lib/release_policy.sh",
        "scripts/pre-public-check.sh",
        "CHANGELOG.md",
    ] {
        copy_repo_file(relative_path, repo_root);
    }

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_root)
        .output()
        .expect("init git repo");
    Command::new("git")
        .args(["config", "user.name", "Pi Tests"])
        .current_dir(repo_root)
        .output()
        .expect("configure git user.name");
    Command::new("git")
        .args(["config", "user.email", "pi-tests@example.com"])
        .current_dir(repo_root)
        .output()
        .expect("configure git user.email");
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    std::fs::remove_file(repo_root.join("CHANGELOG.md")).expect("remove changelog destination");
    Command::new("git")
        .args(["mv", "scripts/pre-public-check.sh", "CHANGELOG.md"])
        .current_dir(repo_root)
        .output()
        .expect("rename script into changelog path");

    let shell = format!(
        "REPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\ndirty=$(release_collect_dirty_lines {root:?})\nrelease_assert_dirty_paths_allowed \"$dirty\"\n",
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run release metadata assertion over rename");

    assert!(
        !output.status.success(),
        "release metadata assertion should reject renames from disallowed paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("scripts/pre-public-check.sh"),
        "rename rejection should keep the disallowed source path visible\noutput:\n{}",
        combined
    );
}

#[test]
fn release_policy_keeps_rename_into_ignored_dirty_paths_visible() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    for relative_path in [
        "scripts/lib/ralph-shell.sh",
        "scripts/lib/release_policy.sh",
        "scripts/pre-public-check.sh",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
    std::fs::create_dir_all(repo_root.join(".ralph")).expect("create .ralph dir");

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_root)
        .output()
        .expect("init git repo");
    Command::new("git")
        .args(["config", "user.name", "Pi Tests"])
        .current_dir(repo_root)
        .output()
        .expect("configure git user.name");
    Command::new("git")
        .args(["config", "user.email", "pi-tests@example.com"])
        .current_dir(repo_root)
        .output()
        .expect("configure git user.email");
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    Command::new("git")
        .args(["mv", "scripts/pre-public-check.sh", ".ralph/trust.json"])
        .current_dir(repo_root)
        .output()
        .expect("rename script into ignored dirty path");

    let shell = format!(
        "REPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\ndirty=$(release_collect_dirty_lines {root:?})\nrelease_filter_dirty_lines \"$dirty\"\n",
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run dirty-line filter over rename into ignored path");

    assert!(
        output.status.success(),
        "dirty-line filter command should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/pre-public-check.sh"),
        "rename filtering should keep the disallowed source path visible even when destination is ignored\nstdout:\n{}",
        stdout
    );
}

#[test]
fn release_scripts_do_not_blanket_ignore_all_ralph_paths_in_cleanliness_checks() {
    let verify_pipeline = read_repo_file("scripts/lib/release_verify_pipeline.sh");
    let release_pipeline = read_repo_file("scripts/lib/release_pipeline.sh");

    for script in [&verify_pipeline, &release_pipeline] {
        assert!(
            !script.contains("grep -vE '^..[[:space:]]+\\.ralph/'"),
            "release cleanliness checks should not blanket-ignore all .ralph paths"
        );
        assert!(
            script.contains("release_filter_dirty_lines"),
            "release cleanliness checks should reuse the shared dirty-path filter"
        );
    }
}
