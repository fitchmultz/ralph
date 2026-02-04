//! `ralph doctor` command: handler.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::sanity::{self, SanityOptions};
use crate::{commands::doctor, config};

/// Output format for doctor command.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum DoctorFormat {
    /// Human-readable text output (default).
    #[default]
    Text,
    /// Machine-readable JSON output for scripting/CI.
    Json,
}

/// Arguments for the `ralph doctor` command.
#[derive(Args)]
pub struct DoctorArgs {
    /// Automatically fix all issues without prompting.
    #[arg(long, conflicts_with = "no_sanity_checks")]
    pub auto_fix: bool,

    /// Skip sanity checks and only run doctor diagnostics.
    #[arg(long, conflicts_with = "auto_fix")]
    pub no_sanity_checks: bool,

    /// Output format (text or json).
    #[arg(long, value_enum, default_value = "text")]
    pub format: DoctorFormat,
}

pub fn handle_doctor(args: DoctorArgs) -> Result<()> {
    // Use resolve_from_cwd_for_doctor to skip instruction_files validation,
    // allowing doctor to diagnose and warn about missing files without failing early.
    let resolved = config::resolve_from_cwd_for_doctor()?;

    // Run sanity checks first (unless skipped)
    if !args.no_sanity_checks {
        let options = SanityOptions {
            auto_fix: args.auto_fix,
            skip: false,
            non_interactive: false, // doctor is always interactive by default
        };
        let sanity_result = sanity::run_sanity_checks(&resolved, &options)?;

        // Report sanity check results
        if !sanity_result.auto_fixes.is_empty() {
            log::info!(
                "Sanity checks applied {} fix(es):",
                sanity_result.auto_fixes.len()
            );
            for fix in &sanity_result.auto_fixes {
                log::info!("  - {}", fix);
            }
        }

        if !sanity_result.needs_attention.is_empty() {
            log::warn!(
                "Sanity checks found {} issue(s) needing attention:",
                sanity_result.needs_attention.len()
            );
            for issue in &sanity_result.needs_attention {
                match issue.severity {
                    sanity::IssueSeverity::Warning => log::warn!("  - {}", issue.message),
                    sanity::IssueSeverity::Error => log::error!("  - {}", issue.message),
                }
            }
        }
    }

    // Run doctor checks with auto_fix flag and format
    let report = doctor::run_doctor(&resolved, args.auto_fix)?;

    // Output based on format
    match args.format {
        DoctorFormat::Text => {
            doctor::print_doctor_report_text(&report);
        }
        DoctorFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    // Return appropriate exit code based on report success
    if report.success {
        Ok(())
    } else {
        anyhow::bail!("Doctor check failed: one or more critical issues found")
    }
}
