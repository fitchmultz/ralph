//! Prompt template management: export, sync, and version tracking.
//!
//! Responsibilities: provide mechanisms to export embedded prompts to `.ralph/prompts/`,
//! track versions, detect modifications, and sync with updated embedded defaults.
//!
//! Not handled:
//! - Prompt rendering or variable expansion (see `util.rs`).
//! - CLI argument parsing (see `cli/prompt.rs`).
//! - Direct file I/O outside of prompt management operations.
//!
//! Invariants/assumptions:
//! - Exported prompts are written to `.ralph/prompts/<name>.md`.
//! - Version tracking is stored in `.ralph/cache/prompt_versions.json`.
//! - Hash computation uses a simple hash of normalized content (trimmed trailing whitespace).

use crate::prompts_internal::registry::{prompt_template, PromptTemplateId};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Version tracking information for exported prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PromptVersionInfo {
    /// Ralph version at time of export.
    pub ralph_version: String,
    /// Timestamp of export (RFC3339).
    pub exported_at: String,
    /// Per-template version information.
    pub templates: HashMap<String, TemplateVersion>,
}

/// Version information for a single template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TemplateVersion {
    /// SHA-256 hash of the exported content.
    pub hash: String,
    /// Timestamp of export (RFC3339).
    pub exported_at: String,
}

/// Sync status for a prompt template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    /// File matches embedded default (no action needed).
    UpToDate,
    /// File matches stored hash but embedded default has changed (can safely update).
    Outdated,
    /// File differs from both embedded and stored hash (user modified).
    UserModified,
    /// File exists but no version info stored (treat as user modified).
    Unknown,
    /// File does not exist in `.ralph/prompts/`.
    Missing,
}

/// Information about a template for display/listing.
#[derive(Debug, Clone)]
pub(crate) struct TemplateInfo {
    #[allow(dead_code)]
    pub id: PromptTemplateId,
    pub name: String,
    #[allow(dead_code)]
    pub label: String,
    pub description: String,
    pub has_override: bool,
}

/// Compute hash of normalized content (trimmed trailing whitespace).
pub(crate) fn compute_hash(content: &str) -> String {
    let normalized = content.trim_end();
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("hash:{:x}", hasher.finish())
}

/// Get the version tracking file path.
fn version_file_path(repo_root: &Path) -> std::path::PathBuf {
    repo_root.join(".ralph/cache/prompt_versions.json")
}

/// Load version tracking info if it exists.
pub(crate) fn load_version_info(repo_root: &Path) -> Result<Option<PromptVersionInfo>> {
    let path = version_file_path(repo_root);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("read version file {}", path.display()))?;
    let info: PromptVersionInfo = serde_json::from_str(&content)
        .with_context(|| format!("parse version file {}", path.display()))?;
    Ok(Some(info))
}

/// Save version tracking info.
pub(crate) fn save_version_info(repo_root: &Path, info: &PromptVersionInfo) -> Result<()> {
    let path = version_file_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(info).context("serialize version info")?;
    fs::write(&path, content).with_context(|| format!("write version file {}", path.display()))?;
    Ok(())
}

/// Get all available template IDs.
pub(crate) fn all_template_ids() -> Vec<PromptTemplateId> {
    vec![
        PromptTemplateId::Worker,
        PromptTemplateId::WorkerPhase1,
        PromptTemplateId::WorkerPhase2,
        PromptTemplateId::WorkerPhase2Handoff,
        PromptTemplateId::WorkerPhase3,
        PromptTemplateId::WorkerSinglePhase,
        PromptTemplateId::TaskBuilder,
        PromptTemplateId::TaskUpdater,
        PromptTemplateId::Scan,
        PromptTemplateId::CodeReview,
        PromptTemplateId::CompletionChecklist,
        PromptTemplateId::Phase2HandoffChecklist,
        PromptTemplateId::IterationChecklist,
    ]
}

/// Get a user-friendly description for a template.
pub(crate) fn template_description(id: PromptTemplateId) -> &'static str {
    match id {
        PromptTemplateId::Worker => "Base worker prompt with mission, context, and operating rules",
        PromptTemplateId::WorkerPhase1 => "Phase 1 planning wrapper (creates implementation plan)",
        PromptTemplateId::WorkerPhase2 => "Phase 2 implementation wrapper (2-phase workflow)",
        PromptTemplateId::WorkerPhase2Handoff => {
            "Phase 2 handoff wrapper (3-phase workflow, includes handoff checklist)"
        }
        PromptTemplateId::WorkerPhase3 => "Phase 3 code review wrapper (reviews implementation)",
        PromptTemplateId::WorkerSinglePhase => {
            "Single-phase wrapper (plan + implement in one pass)"
        }
        PromptTemplateId::TaskBuilder => "Task creation prompt (generates tasks from requests)",
        PromptTemplateId::TaskUpdater => {
            "Task update prompt (refreshes task fields from repo state)"
        }
        PromptTemplateId::Scan => "Repository scan prompt (discovers improvement opportunities)",
        PromptTemplateId::CodeReview => "Code review body content (used in Phase 3)",
        PromptTemplateId::CompletionChecklist => {
            "Implementation completion checklist (validates done criteria)"
        }
        PromptTemplateId::Phase2HandoffChecklist => {
            "Phase 2 handoff checklist (for 3-phase workflow handoff)"
        }
        PromptTemplateId::IterationChecklist => {
            "Refinement mode checklist (for follow-up iterations)"
        }
    }
}

/// Convert template name (snake_case or kebab-case) to PromptTemplateId.
pub(crate) fn parse_template_name(name: &str) -> Option<PromptTemplateId> {
    let normalized = name.replace('-', "_").to_lowercase();
    match normalized.as_str() {
        "worker" => Some(PromptTemplateId::Worker),
        "worker_phase1" | "worker_phase_1" => Some(PromptTemplateId::WorkerPhase1),
        "worker_phase2" | "worker_phase_2" => Some(PromptTemplateId::WorkerPhase2),
        "worker_phase2_handoff" | "worker_phase_2_handoff" => {
            Some(PromptTemplateId::WorkerPhase2Handoff)
        }
        "worker_phase3" | "worker_phase_3" => Some(PromptTemplateId::WorkerPhase3),
        "worker_single_phase" => Some(PromptTemplateId::WorkerSinglePhase),
        "task_builder" => Some(PromptTemplateId::TaskBuilder),
        "task_updater" => Some(PromptTemplateId::TaskUpdater),
        "scan" => Some(PromptTemplateId::Scan),
        "code_review" => Some(PromptTemplateId::CodeReview),
        "completion_checklist" => Some(PromptTemplateId::CompletionChecklist),
        "phase2_handoff_checklist" | "phase_2_handoff_checklist" => {
            Some(PromptTemplateId::Phase2HandoffChecklist)
        }
        "iteration_checklist" => Some(PromptTemplateId::IterationChecklist),
        _ => None,
    }
}

/// Get the file name for a template (without extension).
pub(crate) fn template_file_name(id: PromptTemplateId) -> &'static str {
    match id {
        PromptTemplateId::Worker => "worker",
        PromptTemplateId::WorkerPhase1 => "worker_phase1",
        PromptTemplateId::WorkerPhase2 => "worker_phase2",
        PromptTemplateId::WorkerPhase2Handoff => "worker_phase2_handoff",
        PromptTemplateId::WorkerPhase3 => "worker_phase3",
        PromptTemplateId::WorkerSinglePhase => "worker_single_phase",
        PromptTemplateId::TaskBuilder => "task_builder",
        PromptTemplateId::TaskUpdater => "task_updater",
        PromptTemplateId::Scan => "scan",
        PromptTemplateId::CodeReview => "code_review",
        PromptTemplateId::CompletionChecklist => "completion_checklist",
        PromptTemplateId::Phase2HandoffChecklist => "phase2_handoff_checklist",
        PromptTemplateId::IterationChecklist => "iteration_checklist",
    }
}

/// Get information about all templates.
pub(crate) fn list_templates(repo_root: &Path) -> Vec<TemplateInfo> {
    let prompts_dir = repo_root.join(".ralph/prompts");
    all_template_ids()
        .into_iter()
        .map(|id| {
            let template = prompt_template(id);
            let file_name = template_file_name(id);
            let override_path = prompts_dir.join(format!("{}.md", file_name));
            TemplateInfo {
                id,
                name: file_name.to_string(),
                label: template.label.to_string(),
                description: template_description(id).to_string(),
                has_override: override_path.exists(),
            }
        })
        .collect()
}

/// Get the embedded default content for a template.
pub(crate) fn get_embedded_content(id: PromptTemplateId) -> &'static str {
    let template = prompt_template(id);
    template.embedded_default
}

/// Get the effective content (override if exists, else embedded).
pub(crate) fn get_effective_content(repo_root: &Path, id: PromptTemplateId) -> Result<String> {
    let template = prompt_template(id);
    let override_path = repo_root
        .join(".ralph/prompts")
        .join(format!("{}.md", template_file_name(id)));

    if override_path.exists() {
        fs::read_to_string(&override_path)
            .with_context(|| format!("read override file {}", override_path.display()))
    } else {
        Ok(template.embedded_default.to_string())
    }
}

/// Export a single template to `.ralph/prompts/`.
///
/// Returns true if the file was written, false if it already existed and force was false.
pub(crate) fn export_template(
    repo_root: &Path,
    id: PromptTemplateId,
    force: bool,
    ralph_version: &str,
) -> Result<bool> {
    let template = prompt_template(id);
    let file_name = template_file_name(id);
    let prompts_dir = repo_root.join(".ralph/prompts");
    let file_path = prompts_dir.join(format!("{}.md", file_name));

    // Create prompts directory if needed
    if !prompts_dir.exists() {
        fs::create_dir_all(&prompts_dir)
            .with_context(|| format!("create directory {}", prompts_dir.display()))?;
    }

    // Check if file already exists
    if file_path.exists() && !force {
        return Ok(false);
    }

    // Build content with header
    let embedded_content = template.embedded_default;
    let hash = compute_hash(embedded_content);
    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let header = format!(
        "<!-- Exported from Ralph embedded defaults -->\n\
         <!-- Template: {} -->\n\
         <!-- Version: {} -->\n\
         <!-- Hash: {} -->\n\
         <!-- Exported at: {} -->\n\
         <!-- WARNING: This file may be overwritten by 'ralph prompt sync' unless you rename it -->\n\n",
        file_name, ralph_version, hash, timestamp
    );

    let full_content = format!("{}{}", header, embedded_content);

    // Write file
    fs::write(&file_path, full_content)
        .with_context(|| format!("write prompt file {}", file_path.display()))?;

    // Update version tracking
    let mut version_info = load_version_info(repo_root)?.unwrap_or_else(|| PromptVersionInfo {
        ralph_version: ralph_version.to_string(),
        exported_at: timestamp.clone(),
        templates: HashMap::new(),
    });

    version_info.templates.insert(
        file_name.to_string(),
        TemplateVersion {
            hash,
            exported_at: timestamp,
        },
    );

    save_version_info(repo_root, &version_info)?;

    Ok(true)
}

/// Extract the content portion from an exported file (after the header).
/// Returns None if the file doesn't have an export header.
fn extract_content_from_exported(file_content: &str) -> Option<&str> {
    // Check if this is an exported file with our header
    if !file_content.starts_with("<!-- Exported from Ralph embedded defaults -->") {
        // No header - treat entire file as content
        return Some(file_content);
    }

    // Find the end of the header (first blank line after header comments)
    let in_header = true;
    let mut content_start = 0;

    for (idx, line) in file_content.lines().enumerate() {
        if in_header {
            if line.is_empty() {
                // End of header, next line starts content
                content_start = file_content
                    .lines()
                    .take(idx + 1)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                break;
            }
            // Still in header if it's a comment
            if !line.starts_with("<!--") {
                // Unexpected non-comment in header
                return Some(file_content);
            }
        }
    }

    if content_start > 0 && content_start < file_content.len() {
        Some(&file_content[content_start..])
    } else {
        Some(file_content)
    }
}

/// Check the sync status of a template.
pub(crate) fn check_sync_status(repo_root: &Path, id: PromptTemplateId) -> Result<SyncStatus> {
    let file_name = template_file_name(id);
    let file_path = repo_root
        .join(".ralph/prompts")
        .join(format!("{}.md", file_name));

    if !file_path.exists() {
        return Ok(SyncStatus::Missing);
    }

    let file_content = fs::read_to_string(&file_path)
        .with_context(|| format!("read prompt file {}", file_path.display()))?;

    // Extract content portion (without header) for comparison
    let content_portion = extract_content_from_exported(&file_content).unwrap_or(&file_content);
    let content_hash = compute_hash(content_portion);

    let embedded_content = get_embedded_content(id);
    let embedded_hash = compute_hash(embedded_content);

    // If content matches embedded, it's up to date
    if content_hash == embedded_hash {
        return Ok(SyncStatus::UpToDate);
    }

    // Check version tracking
    let version_info = match load_version_info(repo_root)? {
        Some(info) => info,
        None => return Ok(SyncStatus::Unknown),
    };

    let stored_hash = match version_info.templates.get(file_name) {
        Some(tv) => &tv.hash,
        None => return Ok(SyncStatus::Unknown),
    };

    // If content matches what we exported, but embedded is different, it's outdated
    if &content_hash == stored_hash {
        return Ok(SyncStatus::Outdated);
    }

    // File differs from both embedded and what we exported
    Ok(SyncStatus::UserModified)
}

/// Sync a single template.
///
/// Returns true if the file was updated, false otherwise.
#[allow(dead_code)]
pub(crate) fn sync_template(
    repo_root: &Path,
    id: PromptTemplateId,
    force: bool,
    ralph_version: &str,
) -> Result<(bool, SyncStatus)> {
    let status = check_sync_status(repo_root, id)?;

    match status {
        SyncStatus::UpToDate => Ok((false, status)),
        SyncStatus::Missing => {
            let written = export_template(repo_root, id, force, ralph_version)?;
            Ok((written, SyncStatus::Missing))
        }
        SyncStatus::Outdated | SyncStatus::Unknown if force => {
            let file_name = template_file_name(id);
            let file_path = repo_root
                .join(".ralph/prompts")
                .join(format!("{}.md", file_name));

            // Backup existing file
            if file_path.exists() {
                let backup_path = file_path.with_extension("md.backup");
                fs::copy(&file_path, &backup_path)
                    .with_context(|| format!("create backup {}", backup_path.display()))?;
            }

            let written = export_template(repo_root, id, true, ralph_version)?;
            Ok((written, status))
        }
        SyncStatus::Outdated => {
            // Can safely update
            let written = export_template(repo_root, id, true, ralph_version)?;
            Ok((written, status))
        }
        SyncStatus::UserModified | SyncStatus::Unknown => {
            // Don't overwrite without force
            Ok((false, status))
        }
    }
}

/// Generate a diff between user override and embedded default.
pub(crate) fn generate_diff(repo_root: &Path, id: PromptTemplateId) -> Result<Option<String>> {
    let file_name = template_file_name(id);
    let file_path = repo_root
        .join(".ralph/prompts")
        .join(format!("{}.md", file_name));

    if !file_path.exists() {
        return Ok(None);
    }

    let user_content = fs::read_to_string(&file_path)
        .with_context(|| format!("read prompt file {}", file_path.display()))?;
    let embedded_content = get_embedded_content(id);

    // Simple line-based diff
    let diff = create_unified_diff(&user_content, embedded_content, file_name, "embedded");
    Ok(Some(diff))
}

/// Create a simple unified diff between two texts.
fn create_unified_diff(old: &str, new: &str, old_name: &str, new_name: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut result = String::new();
    result.push_str(&format!("--- {}\n", old_name));
    result.push_str(&format!("+++ {}\n", new_name));

    // Simple LCS-based diff would be ideal, but for now use a simple approach
    // highlighting changed lines
    let max_lines = old_lines.len().max(new_lines.len());
    let mut has_changes = false;

    for i in 0..max_lines {
        let old_line = old_lines.get(i);
        let new_line = new_lines.get(i);

        match (old_line, new_line) {
            (Some(o), Some(n)) if o != n => {
                has_changes = true;
                result.push_str(&format!("-{}\n", o));
                result.push_str(&format!("+{}\n", n));
            }
            (Some(o), None) => {
                has_changes = true;
                result.push_str(&format!("-{}\n", o));
            }
            (None, Some(n)) => {
                has_changes = true;
                result.push_str(&format!("+{}\n", n));
            }
            _ => {}
        }
    }

    if !has_changes {
        result.push_str("No changes\n");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn compute_hash_consistency() {
        let content = "Hello, World!";
        let hash1 = compute_hash(content);
        let hash2 = compute_hash(content);
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("hash:"));
    }

    #[test]
    fn compute_hash_trims_trailing_whitespace() {
        let hash1 = compute_hash("Hello");
        let hash2 = compute_hash("Hello\n\n  \n");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn compute_hash_different_content() {
        let hash1 = compute_hash("Hello");
        let hash2 = compute_hash("World");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn parse_template_name_snake_case() {
        assert_eq!(
            parse_template_name("worker"),
            Some(PromptTemplateId::Worker)
        );
        assert_eq!(
            parse_template_name("worker_phase1"),
            Some(PromptTemplateId::WorkerPhase1)
        );
        assert_eq!(
            parse_template_name("task_builder"),
            Some(PromptTemplateId::TaskBuilder)
        );
    }

    #[test]
    fn parse_template_name_kebab_case() {
        assert_eq!(
            parse_template_name("worker-phase1"),
            Some(PromptTemplateId::WorkerPhase1)
        );
        assert_eq!(
            parse_template_name("task-builder"),
            Some(PromptTemplateId::TaskBuilder)
        );
    }

    #[test]
    fn parse_template_name_case_insensitive() {
        assert_eq!(
            parse_template_name("WORKER"),
            Some(PromptTemplateId::Worker)
        );
        assert_eq!(
            parse_template_name("Worker_Phase1"),
            Some(PromptTemplateId::WorkerPhase1)
        );
    }

    #[test]
    fn parse_template_name_invalid() {
        assert_eq!(parse_template_name("invalid"), None);
        assert_eq!(parse_template_name(""), None);
    }

    #[test]
    fn all_template_ids_count() {
        let ids = all_template_ids();
        assert_eq!(ids.len(), 13);
    }

    #[test]
    fn export_template_creates_file() {
        let temp = TempDir::new().unwrap();
        let written =
            export_template(temp.path(), PromptTemplateId::Worker, false, "0.5.0").unwrap();
        assert!(written);

        let file_path = temp.path().join(".ralph/prompts/worker.md");
        assert!(file_path.exists());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Exported from Ralph embedded defaults"));
        assert!(content.contains("Template: worker"));
        assert!(content.contains("Version: 0.5.0"));
    }

    #[test]
    fn export_template_respects_force() {
        let temp = TempDir::new().unwrap();

        // First export
        let written =
            export_template(temp.path(), PromptTemplateId::Worker, false, "0.5.0").unwrap();
        assert!(written);

        // Second export without force should fail
        let written =
            export_template(temp.path(), PromptTemplateId::Worker, false, "0.5.0").unwrap();
        assert!(!written);

        // Second export with force should succeed
        let written =
            export_template(temp.path(), PromptTemplateId::Worker, true, "0.5.0").unwrap();
        assert!(written);
    }

    #[test]
    fn check_sync_status_missing() {
        let temp = TempDir::new().unwrap();
        let status = check_sync_status(temp.path(), PromptTemplateId::Worker).unwrap();
        assert_eq!(status, SyncStatus::Missing);
    }

    #[test]
    fn check_sync_status_up_to_date() {
        let temp = TempDir::new().unwrap();

        // Export the template
        export_template(temp.path(), PromptTemplateId::Worker, false, "0.5.0").unwrap();

        // Check status - should be up to date (file matches embedded)
        let status = check_sync_status(temp.path(), PromptTemplateId::Worker).unwrap();
        assert_eq!(status, SyncStatus::UpToDate);
    }

    #[test]
    fn version_info_roundtrip() {
        let temp = TempDir::new().unwrap();

        let info = PromptVersionInfo {
            ralph_version: "0.5.0".to_string(),
            exported_at: "2026-01-28T22:30:00Z".to_string(),
            templates: {
                let mut map = HashMap::new();
                map.insert(
                    "worker".to_string(),
                    TemplateVersion {
                        hash: "hash:abc123".to_string(),
                        exported_at: "2026-01-28T22:30:00Z".to_string(),
                    },
                );
                map
            },
        };

        save_version_info(temp.path(), &info).unwrap();
        let loaded = load_version_info(temp.path()).unwrap().unwrap();

        assert_eq!(loaded.ralph_version, info.ralph_version);
        assert_eq!(loaded.templates.len(), 1);
        assert!(loaded.templates.contains_key("worker"));
    }

    #[test]
    fn list_templates_shows_all() {
        let temp = TempDir::new().unwrap();
        let templates = list_templates(temp.path());
        assert_eq!(templates.len(), 13);

        // Check that worker is in the list
        let worker = templates.iter().find(|t| t.name == "worker").unwrap();
        assert_eq!(worker.label, "worker");
        assert!(!worker.has_override);
    }

    #[test]
    fn list_templates_detects_override() {
        let temp = TempDir::new().unwrap();

        // Create an override file
        let prompts_dir = temp.path().join(".ralph/prompts");
        fs::create_dir_all(&prompts_dir).unwrap();
        fs::write(prompts_dir.join("worker.md"), "custom content").unwrap();

        let templates = list_templates(temp.path());
        let worker = templates.iter().find(|t| t.name == "worker").unwrap();
        assert!(worker.has_override);
    }
}
