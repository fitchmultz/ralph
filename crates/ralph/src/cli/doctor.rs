//! `ralph doctor` command: handler.

use anyhow::Result;

use crate::{config, doctor_cmd};

pub fn handle_doctor() -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    doctor_cmd::run_doctor(&resolved)
}
