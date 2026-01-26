//! `ralph doctor` command: handler.

use anyhow::Result;

use crate::{commands::doctor, config};

pub fn handle_doctor() -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    doctor::run_doctor(&resolved)
}
