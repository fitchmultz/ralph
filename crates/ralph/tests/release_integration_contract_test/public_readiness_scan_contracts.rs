//! Public-readiness scan helper contracts (Python + shell).
//!
//! Purpose:
//! - Public-readiness scan helper contracts (Python + shell).
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::process::Command;

use super::support::{
    assert_output_redacts_secret, copy_repo_file, public_readiness_scan_python_path,
    public_readiness_scan_shell_helper_path, read_repo_file, write_file,
};

#[test]
fn public_readiness_scan_rejects_missing_repo_root() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let missing_repo_root = temp_dir.path().join("missing-repo-root");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&missing_repo_root)
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(2),
        "public-readiness scan scanner should reject a missing repo root"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("repository root does not exist or is not a directory"),
        "scanner should explain why the provided repo root was rejected"
    );
}

#[test]
fn public_readiness_scan_rejects_markdown_targets_outside_repo() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    std::fs::write(repo_root.join("README.md"), "[outside](../outside.md)\n")
        .expect("write markdown fixture");
    std::fs::write(temp_dir.path().join("outside.md"), "outside\n")
        .expect("write escaped target fixture");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should reject markdown targets that escape the repo root"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("target escapes repo root"),
        "scanner should explain why escaped markdown targets are invalid"
    );
}

#[cfg(unix)]
#[test]
fn public_readiness_scan_ignores_symlinked_repo_files_that_escape_repo() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    let outside_markdown = temp_dir.path().join("outside.md");
    std::fs::write(&outside_markdown, "[outside](../outside.md)\n")
        .expect("write symlink target fixture");
    symlink(&outside_markdown, repo_root.join("README.md")).expect("create markdown symlink");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(0),
        "public-readiness scan should skip symlinked files instead of following them outside the repo"
    );
    assert!(
        output.stdout.is_empty(),
        "skipped symlinked files should not produce findings"
    );
}

#[cfg(unix)]
#[test]
fn public_readiness_scan_skips_symlinks_into_excluded_repo_paths() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    let excluded_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&excluded_dir).expect("create excluded dir");
    let secret_value = ["sk_live_", "abcdefghijklmnop"].concat();
    std::fs::write(
        excluded_dir.join("secret.md"),
        format!("{}\n", secret_value),
    )
    .expect("write excluded secret fixture");
    symlink(excluded_dir.join("secret.md"), repo_root.join("README.md"))
        .expect("create excluded-path symlink");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", ".ralph/cache/")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(0),
        "public-readiness scan should not follow symlinks into excluded repo paths"
    );
    assert!(
        output.stdout.is_empty(),
        "excluded symlink targets should not produce findings"
    );
}

#[cfg(unix)]
#[test]
fn public_readiness_scan_scans_symlinked_repo_files_that_resolve_within_repo() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    let docs_dir = repo_root.join("docs");
    std::fs::create_dir(&docs_dir).expect("create docs dir");
    std::fs::write(docs_dir.join("source.txt"), "[broken](missing.md)\n")
        .expect("write symlinked markdown source");
    std::fs::write(repo_root.join("missing.md"), "present\n")
        .expect("write misleading repo-root target");
    symlink(docs_dir.join("source.txt"), repo_root.join("README.md"))
        .expect("create in-repo markdown symlink");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should still inspect symlinked files that resolve within the repo"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "README.md: missing target -> missing.md",
        "scanner should resolve symlinked markdown links from the file's canonical location"
    );
}

#[test]
fn public_readiness_scan_scans_allowlisted_ralph_markdown_links() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    for relative_path in [
        "scripts/lib/public_readiness_scan.sh",
        "scripts/lib/public_readiness_scan.py",
        "scripts/lib/release_policy.sh",
        "scripts/lib/ralph-shell.sh",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
    write_file(
        &repo_root.join(".ralph/README.md"),
        "[broken](./definitely-missing-file.md)\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("links")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness link scan over allowlisted .ralph file");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should inspect allowlisted .ralph markdown files"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(".ralph/README.md: missing target -> ./definitely-missing-file.md"),
        "link scan should report missing targets inside allowlisted .ralph files\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_scans_allowlisted_ralph_files_for_secrets() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();
    let secret_token = ["gh", "p_12345678901234567890"].concat();

    for relative_path in [
        "scripts/lib/public_readiness_scan.sh",
        "scripts/lib/public_readiness_scan.py",
        "scripts/lib/release_policy.sh",
        "scripts/lib/ralph-shell.sh",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
    write_file(
        &repo_root.join(".ralph/config.jsonc"),
        &format!("token: {secret_token}\n"),
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("secrets")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness secret scan over allowlisted .ralph file");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should inspect allowlisted .ralph files for secrets"
    );
    assert_output_redacts_secret(&output, &secret_token);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(".ralph/config.jsonc:1: github_classic_token: [REDACTED length=24]"),
        "secret scan should report secrets inside allowlisted .ralph files\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_injected_secret_in_scan_helper_source() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let secret_token = ["gh", "p_12345678901234567890"].concat();
    std::fs::create_dir_all(repo_root.join("scripts/lib")).expect("create scripts/lib dir");
    let scan_source = read_repo_file("scripts/lib/public_readiness_scan.py");
    std::fs::write(
        repo_root.join("scripts/lib/public_readiness_scan.py"),
        format!("# {secret_token}\n{scan_source}"),
    )
    .expect("write injected scan helper source");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.py"))
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper against injected source");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should not file-wide allowlist the scan helper source"
    );
    assert_output_redacts_secret(&output, &secret_token);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/lib/public_readiness_scan.py:")
            && stdout.contains("github_classic_token: [REDACTED length=24]"),
        "injected helper secret should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_same_line_secret_in_security_docs_allowlist() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let secret_token = ["gh", "p_12345678901234567890"].concat();
    std::fs::create_dir_all(repo_root.join("docs/features")).expect("create docs/features dir");
    let aws_example = ["AKIA", "IOSFODNN7EXAMPLE"].concat();
    let exact_allowlisted_line =
        format!("| **AWS Keys** | AKIA-prefixed access keys | `{aws_example}` → `[REDACTED]` |");
    std::fs::write(
        repo_root.join("docs/features/security.md"),
        format!("{exact_allowlisted_line} {secret_token}\n"),
    )
    .expect("write security.md fixture");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan over same-line injected security docs secret");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should reject same-line injected secrets in allowlisted docs lines"
    );
    assert_output_redacts_secret(&output, &secret_token);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("docs/features/security.md:")
            && stdout.contains("github_classic_token: [REDACTED length=24]"),
        "same-line injected docs secret should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_same_line_secret_in_scan_helper_source() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let secret_token = ["gh", "p_12345678901234567890"].concat();
    std::fs::create_dir_all(repo_root.join("scripts/lib")).expect("create scripts/lib dir");

    let scan_source = read_repo_file("scripts/lib/public_readiness_scan.py");
    let target_line = "AWS_DOCS_ALLOWLIST_LINE = (";
    let injected_line = format!("{target_line}  # {secret_token}");
    let injected_source = scan_source.replacen(target_line, &injected_line, 1);
    assert_ne!(
        injected_source, scan_source,
        "fixture should replace the targeted scan-helper source line"
    );
    std::fs::write(
        repo_root.join("scripts/lib/public_readiness_scan.py"),
        injected_source,
    )
    .expect("write injected scan helper source");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.py"))
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper against same-line injected source");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should reject same-line injected secrets in scan-helper source"
    );
    assert_output_redacts_secret(&output, &secret_token);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/lib/public_readiness_scan.py:")
            && stdout.contains("github_classic_token: [REDACTED length=24]"),
        "same-line injected scan-helper secret should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_private_key_in_pre_public_check_script() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let private_key_body = "MIIEpAIBAAKCAQEA75abcdef1234567890";
    std::fs::create_dir_all(repo_root.join("scripts")).expect("create scripts dir");
    std::fs::write(
        repo_root.join("scripts/pre-public-check.sh"),
        format!(
            "-----BEGIN {} PRIVATE KEY-----\n{}\n-----END {} PRIVATE KEY-----\n",
            "RSA", private_key_body, "RSA"
        ),
    )
    .expect("write pre-public-check fixture");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan over pre-public-check fixture");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should reject private keys in pre-public-check.sh"
    );
    let private_key_header = ["BEGIN", " RSA PRIVATE KEY"].concat();
    assert_output_redacts_secret(&output, &private_key_header);
    assert_output_redacts_secret(&output, private_key_body);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/pre-public-check.sh:1: private_key: [REDACTED length=21]"),
        "private key in pre-public-check.sh should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_help_with_extra_args() {
    let output = Command::new("bash")
        .arg(public_readiness_scan_shell_helper_path())
        .arg("--help")
        .arg("extra")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(2),
        "public-readiness scan helper should reject unexpected positional arguments"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage:"),
        "helper should print usage for invalid argument combinations"
    );
}

#[test]
fn public_readiness_scan_rejects_links_with_extra_args() {
    let output = Command::new("bash")
        .arg(public_readiness_scan_shell_helper_path())
        .arg("links")
        .arg("extra")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(2),
        "public-readiness scan helper should reject extra args for normal modes"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage:"),
        "helper should print usage for invalid argument combinations"
    );
}
