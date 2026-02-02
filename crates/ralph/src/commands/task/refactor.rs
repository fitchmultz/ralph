//! Refactor task generation for large files exceeding LOC thresholds.
//!
//! Responsibilities:
//! - Scan directories for Rust files exceeding LOC thresholds.
//! - Count lines of code (excluding comments and empty lines).
//! - Group related files based on batch mode strategy.
//! - Generate refactoring tasks using the task builder.
//! - Build request text and scope strings for task creation.
//!
//! Not handled here:
//! - Task building (delegates to build.rs via build_task).
//! - Task updating (see update.rs).
//! - CLI argument parsing or command routing.
//! - Non-Rust file scanning.
//!
//! Invariants/assumptions:
//! - LOC counting excludes comments and empty lines for accurate measurement.
//! - Hidden directories, target/, and .ralph/cache/ are skipped during scanning.
//! - File grouping uses test file naming conventions for relationship detection.
//! - Batch mode determines grouping strategy (Auto, Never, Aggressive).

use super::{BatchMode, TaskBuildOptions, TaskBuildRefactorOptions};
use crate::config;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Build refactoring tasks for large files exceeding the LOC threshold.
///
/// Scans the specified directory for Rust files, identifies those exceeding
/// the threshold, groups them based on batch mode, and creates tasks using
/// the task builder.
pub fn build_refactor_tasks(
    resolved: &config::Resolved,
    opts: TaskBuildRefactorOptions,
) -> Result<()> {
    // Determine scan path (default to repo root for generic usage)
    let scan_path = opts
        .path
        .clone()
        .unwrap_or_else(|| resolved.repo_root.clone());

    // Scan for large .rs files
    let large_files = scan_for_large_files(&scan_path, opts.threshold)?;

    if large_files.is_empty() {
        println!(
            "No files found exceeding {} LOC threshold in {}.",
            opts.threshold,
            scan_path.display()
        );
        return Ok(());
    }

    println!(
        "Found {} file(s) exceeding {} LOC:",
        large_files.len(),
        opts.threshold
    );
    for (path, loc) in &large_files {
        println!("  {} ({} LOC)", path.display(), loc);
    }

    // Group files based on batch mode
    let groups = group_files(&large_files, opts.batch);

    println!("\nWill create {} task(s):", groups.len());
    for (i, group) in groups.iter().enumerate() {
        match &group[..] {
            [(path, loc)] => {
                println!("  {}. {} ({} LOC)", i + 1, path.display(), loc);
            }
            multiple => {
                let total_loc: usize = multiple.iter().map(|(_, loc)| loc).sum();
                println!(
                    "  {}. {} files in {} ({} total LOC)",
                    i + 1,
                    multiple.len(),
                    multiple[0].0.parent().unwrap_or(&multiple[0].0).display(),
                    total_loc
                );
            }
        }
    }

    if opts.dry_run {
        println!("\nDry run - no tasks created.");
        return Ok(());
    }

    // Create tasks for each group
    let mut created_count = 0;
    for group in groups {
        let request = build_refactor_request(&group);
        let scope = build_scope(&group);

        let mut hint_tags = "refactor,large-file".to_string();
        if !opts.extra_tags.is_empty() {
            hint_tags.push(',');
            hint_tags.push_str(&opts.extra_tags);
        }

        super::build_task(
            resolved,
            TaskBuildOptions {
                request,
                hint_tags,
                hint_scope: scope,
                runner_override: opts.runner_override,
                model_override: opts.model_override.clone(),
                reasoning_effort_override: opts.reasoning_effort_override,
                runner_cli_overrides: opts.runner_cli_overrides.clone(),
                force: opts.force,
                repoprompt_tool_injection: opts.repoprompt_tool_injection,
                template_hint: Some("refactor".to_string()),
                template_target: None,
                strict_templates: false,
            },
        )?;
        created_count += 1;
    }

    println!("\nCreated {} refactoring task(s).", created_count);
    Ok(())
}

/// Scan directory for .rs files exceeding threshold.
/// Returns Vec of (path, loc_count) sorted by loc descending.
fn scan_for_large_files(root: &Path, threshold: usize) -> Result<Vec<(PathBuf, usize)>> {
    let mut results = Vec::new();
    scan_directory_recursive(root, root, threshold, &mut results)?;

    // Sort by LOC descending (largest first)
    results.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(results)
}

/// Recursively scan directory for Rust files.
#[allow(clippy::only_used_in_recursion)]
fn scan_directory_recursive(
    root: &Path,
    current: &Path,
    threshold: usize,
    results: &mut Vec<(PathBuf, usize)>,
) -> Result<()> {
    let entries = std::fs::read_dir(current)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden dirs, target/, and .ralph/cache/
        if path.is_dir() {
            if name_str.starts_with('.') || name_str == "target" {
                continue;
            }
            // Skip .ralph/cache/ to avoid scanning generated/temp files
            if path
                .components()
                .any(|c| c.as_os_str() == ".ralph" || c.as_os_str() == "cache")
            {
                continue;
            }
            scan_directory_recursive(root, &path, threshold, results)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let loc = count_lines_of_code(&path)?;
            if loc > threshold {
                results.push((path.to_path_buf(), loc));
            }
        }
    }

    Ok(())
}

/// Count non-empty, non-comment lines in a Rust file.
fn count_lines_of_code(path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let mut count = 0;
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.starts_with("//") {
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        count += 1;
    }

    Ok(count)
}

/// Group files based on batch mode strategy.
fn group_files(files: &[(PathBuf, usize)], mode: BatchMode) -> Vec<Vec<(PathBuf, usize)>> {
    match mode {
        BatchMode::Never => files.iter().map(|f| vec![f.clone()]).collect(),
        BatchMode::Aggressive => {
            // Group by parent directory
            let mut groups: std::collections::HashMap<PathBuf, Vec<(PathBuf, usize)>> =
                std::collections::HashMap::new();
            for (path, loc) in files {
                let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                groups.entry(parent).or_default().push((path.clone(), *loc));
            }
            groups.into_values().collect()
        }
        BatchMode::Auto => {
            // Group files with similar names in same directory
            // (e.g., test_*.rs, *_tests.rs)
            let mut groups: Vec<Vec<(PathBuf, usize)>> = Vec::new();
            let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();

            for (i, (path, loc)) in files.iter().enumerate() {
                if used.contains(&i) {
                    continue;
                }

                let parent = path.parent();
                let stem = path.file_stem().and_then(|s| s.to_str());

                let mut group = vec![(path.clone(), *loc)];
                used.insert(i);

                // Look for related files
                for (j, (other_path, other_loc)) in files.iter().enumerate().skip(i + 1) {
                    if used.contains(&j) {
                        continue;
                    }

                    if other_path.parent() != parent {
                        continue;
                    }

                    let other_stem = other_path.file_stem().and_then(|s| s.to_str());

                    // Check for test file relationships
                    if let (Some(s), Some(os)) = (stem, other_stem)
                        && is_related_file(s, os)
                    {
                        group.push((other_path.clone(), *other_loc));
                        used.insert(j);
                    }
                }

                groups.push(group);
            }

            groups
        }
    }
}

/// Check if two file stems are related (e.g., "foo" and "foo_tests").
fn is_related_file(a: &str, b: &str) -> bool {
    let test_suffixes = ["_test", "_tests", "test_"];

    for suffix in &test_suffixes {
        if a.starts_with(suffix) && b == &a[suffix.len()..] {
            return true;
        }
        if b.starts_with(suffix) && a == &b[suffix.len()..] {
            return true;
        }
        if a.ends_with(suffix) && b == &a[..a.len() - suffix.len()] {
            return true;
        }
        if b.ends_with(suffix) && a == &b[..b.len() - suffix.len()] {
            return true;
        }
    }

    false
}

/// Build the request text for a refactoring task.
fn build_refactor_request(group: &[(PathBuf, usize)]) -> String {
    match group {
        [(path, loc)] => {
            format!(
                "Refactor {} ({} LOC) to improve maintainability by splitting it into smaller, cohesive modules per AGENTS.md guidelines.",
                path.display(),
                loc
            )
        }
        files => {
            let total_loc: usize = files.iter().map(|(_, loc)| loc).sum();
            let paths: Vec<String> = files.iter().map(|(p, _)| p.display().to_string()).collect();
            format!(
                "Refactor {} related files ({} total LOC) to improve maintainability by splitting them into smaller, cohesive modules per AGENTS.md guidelines. Files: {}",
                files.len(),
                total_loc,
                paths.join(", ")
            )
        }
    }
}

/// Build the scope string for a group of files.
fn build_scope(group: &[(PathBuf, usize)]) -> String {
    group
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::{build_refactor_request, build_scope, count_lines_of_code, is_related_file};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn count_lines_of_code_skips_comments_and_empty() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "// comment").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "fn main() {{").unwrap();
        writeln!(f, "    println!(\"hello\");").unwrap();
        writeln!(f, "}}").unwrap();

        let loc = count_lines_of_code(&file).unwrap();
        assert_eq!(loc, 3); // fn main, println, closing brace
    }

    #[test]
    fn count_lines_of_code_handles_block_comments() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "/* block comment start").unwrap();
        writeln!(f, "   continues here */").unwrap();
        writeln!(f, "fn main() {{").unwrap();
        writeln!(f, "    /* inline */ println!(\"hello\");").unwrap();
        writeln!(f, "}}").unwrap();

        let loc = count_lines_of_code(&file).unwrap();
        assert_eq!(loc, 2); // fn main, println
    }

    #[test]
    fn is_related_file_detects_test_pairs() {
        assert!(is_related_file("foo", "foo_test"));
        assert!(is_related_file("foo_test", "foo"));
        assert!(is_related_file("test_foo", "foo"));
        assert!(is_related_file("foo", "test_foo"));
        assert!(is_related_file("foo_tests", "foo"));
        assert!(is_related_file("foo", "foo_tests"));
        assert!(!is_related_file("foo", "bar"));
        assert!(!is_related_file("foo_test", "bar"));
    }

    #[test]
    fn build_refactor_request_single_file() {
        let group = vec![(PathBuf::from("src/main.rs"), 1200)];
        let request = build_refactor_request(&group);
        assert!(request.contains("src/main.rs"));
        assert!(request.contains("1200 LOC"));
        assert!(request.contains("AGENTS.md"));
    }

    #[test]
    fn build_refactor_request_multiple_files() {
        let group = vec![
            (PathBuf::from("src/foo.rs"), 800),
            (PathBuf::from("src/foo_test.rs"), 500),
        ];
        let request = build_refactor_request(&group);
        assert!(request.contains("2 related files"));
        assert!(request.contains("1300 total LOC"));
        assert!(request.contains("src/foo.rs"));
        assert!(request.contains("src/foo_test.rs"));
    }

    #[test]
    fn build_scope_single_file() {
        let group = vec![(PathBuf::from("src/main.rs"), 1200)];
        let scope = build_scope(&group);
        assert_eq!(scope, "src/main.rs");
    }

    #[test]
    fn build_scope_multiple_files() {
        let group = vec![
            (PathBuf::from("src/foo.rs"), 800),
            (PathBuf::from("src/bar.rs"), 500),
        ];
        let scope = build_scope(&group);
        assert_eq!(scope, "src/foo.rs,src/bar.rs");
    }
}
