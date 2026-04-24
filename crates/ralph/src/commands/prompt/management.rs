//! Prompt template management commands.
//!
//! Purpose:
//! - Prompt template management commands.
//!
//! Responsibilities:
//! - List, show, export, sync, and diff prompt templates.
//! - Delegate actual template inventory/storage work to `prompts_internal::management`.
//!
//! Not handled here:
//! - Worker/scan/task-builder preview rendering.
//! - CLI argument parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Export/sync operations target `.ralph/prompts/`.
//! - Errors are surfaced per-template without aborting unrelated exports.

use std::path::Path;

use anyhow::Result;

use crate::prompts_internal::management as prompt_mgmt;

pub fn list_prompts(repo_root: &Path) -> Result<()> {
    let templates = prompt_mgmt::list_templates(repo_root);
    println!("Available prompt templates ({} total):\n", templates.len());

    let max_name_len = templates
        .iter()
        .map(|template| template.name.len())
        .max()
        .unwrap_or(0);
    for template in templates {
        let status = if template.has_override {
            " [override]"
        } else {
            ""
        };
        println!(
            "  {:width$}  {}{}",
            template.name,
            template.description,
            status,
            width = max_name_len
        );
    }

    println!("\nOverride paths: .ralph/prompts/<name>.md");
    println!("Use 'ralph prompt show <name> --raw' to view raw embedded content");
    Ok(())
}

pub fn show_prompt(repo_root: &Path, name: &str, raw: bool) -> Result<()> {
    let id = prompt_mgmt::parse_template_name(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown template name: '{}'", name))?;

    let content = if raw {
        prompt_mgmt::get_embedded_content(id).to_string()
    } else {
        prompt_mgmt::get_effective_content(repo_root, id)?
    };

    print!("{}", content);
    Ok(())
}

pub fn export_prompts(repo_root: &Path, name: Option<&str>, force: bool) -> Result<()> {
    let ralph_version = env!("CARGO_PKG_VERSION");

    if let Some(name) = name {
        let id = prompt_mgmt::parse_template_name(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown template name: '{}'", name))?;
        let file_name = prompt_mgmt::template_file_name(id);
        let written = prompt_mgmt::export_template(repo_root, id, force, ralph_version)?;
        if written {
            println!("Exported {} to .ralph/prompts/{}.md", file_name, file_name);
        } else {
            println!(
                "Skipped {}: file already exists (use --force to overwrite)",
                file_name
            );
        }
        return Ok(());
    }

    let mut exported = 0;
    let mut skipped = 0;
    for id in prompt_mgmt::all_template_ids() {
        let file_name = prompt_mgmt::template_file_name(id);
        match prompt_mgmt::export_template(repo_root, id, force, ralph_version) {
            Ok(true) => {
                exported += 1;
                println!("Exported {}", file_name);
            }
            Ok(false) => {
                skipped += 1;
                println!("Skipped {}: already exists", file_name);
            }
            Err(error) => eprintln!("Error exporting {}: {}", file_name, error),
        }
    }

    println!("\nExported {} templates, skipped {}", exported, skipped);
    if skipped > 0 && !force {
        println!("Use --force to overwrite existing files");
    }
    Ok(())
}

pub fn sync_prompts(repo_root: &Path, dry_run: bool, force: bool) -> Result<()> {
    let ralph_version = env!("CARGO_PKG_VERSION");
    let templates = prompt_mgmt::all_template_ids();
    let mut up_to_date = Vec::new();
    let mut outdated = Vec::new();
    let mut user_modified = Vec::new();
    let mut missing = Vec::new();

    for id in &templates {
        let file_name = prompt_mgmt::template_file_name(*id);
        match prompt_mgmt::check_sync_status(repo_root, *id)? {
            prompt_mgmt::SyncStatus::UpToDate => up_to_date.push(file_name),
            prompt_mgmt::SyncStatus::Outdated => outdated.push((file_name.to_string(), *id)),
            prompt_mgmt::SyncStatus::UserModified | prompt_mgmt::SyncStatus::Unknown => {
                user_modified.push((file_name.to_string(), *id))
            }
            prompt_mgmt::SyncStatus::Missing => missing.push((file_name.to_string(), *id)),
        }
    }

    if dry_run {
        println!("Dry run - no changes will be made:\n");
        print_group("Would update", &outdated);
        print_group("Would create", &missing);
        print_group("Would skip (user modified)", &user_modified);
        if !up_to_date.is_empty() {
            println!("Up to date ({}):", up_to_date.len());
            for name in &up_to_date {
                println!("  {}", name);
            }
        }
        return Ok(());
    }

    let mut updated = 0;
    let mut skipped = 0;
    let mut created = 0;

    for (name, id) in outdated {
        match prompt_mgmt::sync_template(repo_root, id, false, ralph_version) {
            Ok((true, _)) => {
                updated += 1;
                println!("Updated {} (outdated)", name);
            }
            Ok((false, _)) => {
                skipped += 1;
                println!("Skipped {} (outdated)", name);
            }
            Err(error) => {
                skipped += 1;
                eprintln!("Error updating {}: {}", name, error);
            }
        }
    }

    for (name, id) in missing {
        match prompt_mgmt::sync_template(repo_root, id, false, ralph_version) {
            Ok((true, _)) => {
                created += 1;
                println!("Created {}", name);
            }
            Ok((false, _)) => {
                skipped += 1;
                println!("Skipped {}: already exists", name);
            }
            Err(error) => {
                skipped += 1;
                eprintln!("Error creating {}: {}", name, error);
            }
        }
    }

    for (name, id) in user_modified {
        match prompt_mgmt::sync_template(repo_root, id, force, ralph_version) {
            Ok((true, _)) => {
                updated += 1;
                println!("Overwrote {} (user modified, --force)", name);
            }
            Ok((false, _)) => {
                skipped += 1;
                println!("Skipped {} (user modified, use --force to overwrite)", name);
            }
            Err(error) => {
                skipped += 1;
                eprintln!("Error overwriting {}: {}", name, error);
            }
        }
    }

    println!(
        "\nSync complete: {} updated, {} created, {} skipped",
        updated, created, skipped
    );
    Ok(())
}

pub fn diff_prompt(repo_root: &Path, name: &str) -> Result<()> {
    let id = prompt_mgmt::parse_template_name(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown template name: '{}'", name))?;
    match prompt_mgmt::generate_diff(repo_root, id)? {
        Some(diff) => print!("{}", diff),
        None => println!("No local override for '{}' - using embedded default", name),
    }
    Ok(())
}

fn print_group<T>(label: &str, items: &[(String, T)]) {
    if items.is_empty() {
        return;
    }
    println!("{} ({}):", label, items.len());
    for (name, _) in items {
        println!("  {}", name);
    }
}
