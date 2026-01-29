//! `ralph doctor` command: handler.

use anyhow::Result;

use crate::{commands::doctor, config};

pub fn handle_doctor() -> Result<()> {
    // Use resolve_from_cwd_for_doctor to skip instruction_files validation,
    // allowing doctor to diagnose and warn about missing files without failing early.
    let resolved = config::resolve_from_cwd_for_doctor()?;
    doctor::run_doctor(&resolved)
}
