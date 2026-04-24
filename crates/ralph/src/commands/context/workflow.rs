//! AGENTS.md command workflows.
//!
//! Purpose:
//! - AGENTS.md command workflows.
//!
//! Responsibilities:
//! - Implement init, update, and validate command behavior.
//! - Coordinate detection, wizard prompting, rendering, merging, and persistence.
//!
//! Not handled here:
//! - CLI parsing.
//! - Low-level markdown section parsing details.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::detect::{detect_project_type, detected_type_to_hint, hint_to_detected, is_tty};
use super::markdown::parse_markdown_sections;
use super::render::{generate_agents_md, generate_agents_md_with_hints};
use super::types::{
    ContextInitOptions, ContextUpdateOptions, ContextValidateOptions, FileInitStatus, InitReport,
    UpdateReport, ValidateReport,
};
use super::wizard::ContextPrompter;
use super::{merge, validate, wizard};
use crate::config;
use crate::fsutil;
use anyhow::{Context, Result};
use std::fs;

pub fn run_context_init(
    resolved: &config::Resolved,
    opts: ContextInitOptions,
) -> Result<InitReport> {
    if !opts.interactive && opts.output_path.exists() && !opts.force {
        let detected = opts
            .project_type_hint
            .map(hint_to_detected)
            .unwrap_or_else(|| detect_project_type(&resolved.repo_root));
        return Ok(InitReport {
            status: FileInitStatus::Valid,
            detected_project_type: detected,
            output_path: opts.output_path,
        });
    }

    let detected_type = opts
        .project_type_hint
        .map(hint_to_detected)
        .unwrap_or_else(|| detect_project_type(&resolved.repo_root));

    let (project_type, output_path, content) = if opts.interactive {
        if !is_tty() {
            anyhow::bail!("Interactive mode requires a TTY terminal");
        }

        let prompter = wizard::DialoguerPrompter;
        let wizard_result = wizard::run_init_wizard(
            &prompter,
            detected_type_to_hint(detected_type),
            &opts.output_path,
        )
        .context("interactive wizard failed")?;

        let project_type = hint_to_detected(wizard_result.project_type);
        let output_path = wizard_result
            .output_path
            .unwrap_or_else(|| opts.output_path.clone());
        let content = generate_agents_md_with_hints(
            resolved,
            project_type,
            Some(&wizard_result.config_hints),
        )?;

        if wizard_result.confirm_write {
            println!("\n{}", "─".repeat(60));
            println!(
                "{}",
                colored::Colorize::bold("Preview of generated AGENTS.md:")
            );
            println!("{}", "─".repeat(60));
            println!("{}", content);
            println!("{}", "─".repeat(60));

            let proceed = prompter
                .confirm("Write this AGENTS.md?", true)
                .context("failed to get confirmation")?;

            if !proceed {
                anyhow::bail!("AGENTS.md creation cancelled by user");
            }
        }

        (project_type, output_path, content)
    } else {
        let content = generate_agents_md(resolved, detected_type)?;
        (detected_type, opts.output_path.clone(), content)
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    fsutil::write_atomic(&output_path, content.as_bytes())
        .with_context(|| format!("write AGENTS.md {}", output_path.display()))?;

    Ok(InitReport {
        status: FileInitStatus::Created,
        detected_project_type: project_type,
        output_path,
    })
}

pub fn run_context_update(
    _resolved: &config::Resolved,
    opts: ContextUpdateOptions,
) -> Result<UpdateReport> {
    if !opts.output_path.exists() {
        anyhow::bail!(
            "AGENTS.md does not exist at {}. Run `ralph context init` first.",
            opts.output_path.display()
        );
    }

    let existing_content =
        fs::read_to_string(&opts.output_path).context("read existing AGENTS.md")?;
    let existing_doc = merge::parse_markdown_document(&existing_content);
    let existing_sections = existing_doc
        .section_titles()
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

    let mut updates: Vec<(String, String)> = Vec::new();

    if opts.interactive {
        if !is_tty() {
            anyhow::bail!("Interactive mode requires a TTY terminal");
        }

        let prompter = wizard::DialoguerPrompter;
        updates = wizard::run_update_wizard(&prompter, &existing_sections, &existing_content)
            .context("interactive wizard failed")?;
    } else if let Some(file_path) = &opts.file {
        let new_content = fs::read_to_string(file_path).context("read update file")?;
        let parsed = parse_markdown_sections(&new_content);

        for (section_name, section_content) in parsed {
            if opts.sections.is_empty() || opts.sections.contains(&section_name) {
                updates.push((section_name, section_content));
            }
        }
    } else {
        anyhow::bail!(
            "No update source specified. Use --interactive, --file, or specify sections with content."
        );
    }

    if updates.is_empty() {
        return Ok(UpdateReport {
            sections_updated: Vec::new(),
            dry_run: opts.dry_run,
        });
    }

    let (merged_doc, sections_updated) = merge::merge_section_updates(&existing_doc, &updates);

    if opts.dry_run {
        println!("\n{}", "─".repeat(60));
        println!(
            "{}",
            colored::Colorize::bold("Dry run - changes that would be made:")
        );
        println!("{}", "─".repeat(60));
        for section in &sections_updated {
            println!("  • Update section: {}", section);
        }
        println!("{}", "─".repeat(60));
        return Ok(UpdateReport {
            sections_updated,
            dry_run: true,
        });
    }

    let merged_content = merged_doc.to_content();
    fsutil::write_atomic(&opts.output_path, merged_content.as_bytes())
        .with_context(|| format!("write AGENTS.md {}", opts.output_path.display()))?;

    Ok(UpdateReport {
        sections_updated,
        dry_run: false,
    })
}

pub fn run_context_validate(
    _resolved: &config::Resolved,
    opts: ContextValidateOptions,
) -> Result<ValidateReport> {
    validate::run_context_validate_impl(opts)
}
