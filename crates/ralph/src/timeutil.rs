use anyhow::{Context, Result};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn now_utc_rfc3339() -> Result<String> {
	OffsetDateTime::now_utc()
		.format(&Rfc3339)
		.context("format RFC3339 timestamp")
}