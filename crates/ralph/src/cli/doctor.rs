//! `ralph doctor` command: handler.

use anyhow::Result;
use clap::Args;

use crate::sanity::{self, SanityOptions};
use crate::{commands::doctor, config};

/// Arguments for the `ralph doctor` command.
#[derive(Args)]
pub struct DoctorArgs {
    /// Automatically fix all issues without prompting.
    #[arg(long, conflicts_with = "no_sanity_checks")]
    pub auto_fix: bool,

    /// Skip sanity checks and only run doctor diagnostics.
    #[arg(long, conflicts_with = "auto_fix")]
    pub no_sanity_checks: bool,
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

    // Run existing doctor checks
    doctor::run_doctor(&resolved)
}
