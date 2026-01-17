use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
	let dir = path.parent().context("atomic write requires a parent directory")?;
	fs::create_dir_all(dir).with_context(|| format!("create directory {}", dir.display()))?;

	let mut tmp = tempfile::NamedTempFile::new_in(dir)
		.with_context(|| format!("create temp file in {}", dir.display()))?;
	tmp.write_all(contents).context("write temp file")?;
	tmp.flush().context("flush temp file")?;
	tmp.as_file().sync_all().context("sync temp file")?;

	tmp.persist(path)
		.map_err(|err| err.error)
		.with_context(|| format!("persist {}", path.display()))?;

	sync_dir_best_effort(dir);
	Ok(())
}

fn sync_dir_best_effort(dir: &Path) {
	#[cfg(unix)]
	{
		if let Ok(file) = fs::File::open(dir) {
			let _ = file.sync_all();
		}
	}

	#[cfg(not(unix))]
	{
		let _ = dir;
	}
}