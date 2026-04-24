//! Prompt export and sync workflows.
//!
//! Purpose:
//! - Prompt export and sync workflows.
//!
//! Responsibilities:
//! - Export prompt templates with version-tracking metadata.
//! - Classify sync state and generate diffs between overrides and embedded defaults.
//!
//! Not handled here:
//! - Prompt rendering or placeholder expansion.
//! - Template inventory definitions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Existing user-modified files are preserved unless force sync/export is requested.
//! - A missing or legacy version file yields `SyncStatus::Unknown` instead of trusting stale data.

use super::{
    compute_hash,
    storage::{
        PROMPT_VERSION_SCHEMA, PromptVersionInfo, TemplateVersion, load_version_info,
        save_version_info,
    },
    templates::{SyncStatus, get_embedded_content, template_file_name},
};
use crate::prompts_internal::registry::{PromptTemplateId, prompt_template};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

    if !prompts_dir.exists() {
        fs::create_dir_all(&prompts_dir)
            .with_context(|| format!("create directory {}", prompts_dir.display()))?;
    }

    if file_path.exists() && !force {
        return Ok(false);
    }

    let embedded_content = template.embedded_default;
    let digest = compute_hash(embedded_content);
    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    let header = format!(
        "<!-- Exported from Ralph embedded defaults -->\n\
         <!-- Template: {} -->\n\
         <!-- Version: {} -->\n\
         <!-- Digest: {} -->\n\
         <!-- Exported at: {} -->\n\
         <!-- WARNING: This file may be overwritten by 'ralph prompt sync' unless you rename it -->\n\n",
        file_name, ralph_version, digest, timestamp
    );
    fs::write(&file_path, format!("{}{}", header, embedded_content))
        .with_context(|| format!("write prompt file {}", file_path.display()))?;

    let mut version_info = load_version_info(repo_root)?.unwrap_or_else(|| PromptVersionInfo {
        schema_version: PROMPT_VERSION_SCHEMA,
        ralph_version: ralph_version.to_string(),
        exported_at: timestamp.clone(),
        templates: HashMap::new(),
    });
    version_info.schema_version = PROMPT_VERSION_SCHEMA;
    version_info.ralph_version = ralph_version.to_string();
    version_info.exported_at = timestamp.clone();
    version_info.templates.insert(
        file_name.to_string(),
        TemplateVersion {
            digest,
            exported_at: timestamp,
        },
    );
    save_version_info(repo_root, &version_info)?;

    Ok(true)
}

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
    let content = extract_content_from_exported(&file_content).unwrap_or(&file_content);
    let content_digest = compute_hash(content);
    let embedded_digest = compute_hash(get_embedded_content(id));

    if content_digest == embedded_digest {
        return Ok(SyncStatus::UpToDate);
    }

    let Some(version_info) = load_version_info(repo_root)? else {
        return Ok(SyncStatus::Unknown);
    };
    let Some(template_version) = version_info.templates.get(file_name) else {
        return Ok(SyncStatus::Unknown);
    };

    if content_digest == template_version.digest {
        return Ok(SyncStatus::Outdated);
    }

    Ok(SyncStatus::UserModified)
}

pub(crate) fn sync_template(
    repo_root: &Path,
    id: PromptTemplateId,
    force: bool,
    ralph_version: &str,
) -> Result<(bool, SyncStatus)> {
    let status = check_sync_status(repo_root, id)?;
    match status {
        SyncStatus::UpToDate => Ok((false, status)),
        SyncStatus::Missing => Ok((
            export_template(repo_root, id, force, ralph_version)?,
            status,
        )),
        SyncStatus::Outdated => Ok((export_template(repo_root, id, true, ralph_version)?, status)),
        SyncStatus::Unknown | SyncStatus::UserModified if force => {
            let file_path = repo_root
                .join(".ralph/prompts")
                .join(format!("{}.md", template_file_name(id)));
            if file_path.exists() {
                let backup_path = file_path.with_extension("md.backup");
                fs::copy(&file_path, &backup_path)
                    .with_context(|| format!("create backup {}", backup_path.display()))?;
            }
            Ok((export_template(repo_root, id, true, ralph_version)?, status))
        }
        SyncStatus::Unknown | SyncStatus::UserModified => Ok((false, status)),
    }
}

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
    Ok(Some(create_unified_diff(
        &user_content,
        get_embedded_content(id),
        file_name,
        "embedded",
    )))
}

fn extract_content_from_exported(file_content: &str) -> Option<&str> {
    if !file_content.starts_with("<!-- Exported from Ralph embedded defaults -->") {
        return Some(file_content);
    }

    let mut content_start = 0;
    for (idx, line) in file_content.lines().enumerate() {
        if line.is_empty() {
            content_start = file_content
                .lines()
                .take(idx + 1)
                .map(|line| line.len() + 1)
                .sum::<usize>();
            break;
        }
        if !line.starts_with("<!--") {
            return Some(file_content);
        }
    }

    if content_start > 0 && content_start <= file_content.len() {
        Some(&file_content[content_start..])
    } else {
        Some(file_content)
    }
}

fn create_unified_diff(old: &str, new: &str, old_name: &str, new_name: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut result = String::new();
    result.push_str(&format!("--- {}\n", old_name));
    result.push_str(&format!("+++ {}\n", new_name));

    let mut has_changes = false;
    for index in 0..old_lines.len().max(new_lines.len()) {
        match (old_lines.get(index), new_lines.get(index)) {
            (Some(old_line), Some(new_line)) if old_line != new_line => {
                has_changes = true;
                result.push_str(&format!("-{}\n", old_line));
                result.push_str(&format!("+{}\n", new_line));
            }
            (Some(old_line), None) => {
                has_changes = true;
                result.push_str(&format!("-{}\n", old_line));
            }
            (None, Some(new_line)) => {
                has_changes = true;
                result.push_str(&format!("+{}\n", new_line));
            }
            _ => {}
        }
    }

    if !has_changes {
        result.push_str("No changes\n");
    }

    result
}
