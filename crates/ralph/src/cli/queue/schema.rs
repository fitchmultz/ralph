//! Queue schema subcommand.

use anyhow::Result;

use crate::contracts;

pub(crate) fn handle() -> Result<()> {
    let schema = schemars::schema_for!(contracts::QueueFile);
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
