//! `ralph init` command: Clap types and handler.

use anyhow::Result;
use clap::Args;

use crate::{config, init_cmd};

pub fn handle_init(args: InitArgs, force_lock: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let report = init_cmd::run_init(
        &resolved,
        init_cmd::InitOptions {
            force: args.force,
            force_lock,
        },
    )?;

    fn report_status(label: &str, status: init_cmd::FileInitStatus, path: &std::path::Path) {
        match status {
            init_cmd::FileInitStatus::Created => {
                log::info!("{}: created ({})", label, path.display())
            }
            init_cmd::FileInitStatus::Valid => {
                log::info!("{}: exists (valid) ({})", label, path.display())
            }
        }
    }

    report_status("queue", report.queue_status, &resolved.queue_path);
    report_status("done", report.done_status, &resolved.done_path);
    if let Some(status) = report.readme_status {
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        report_status("readme", status, &readme_path);
    }
    if let Some(path) = resolved.project_config_path.as_ref() {
        report_status("config", report.config_status, path);
    } else {
        log::info!("config: unavailable");
    }
    Ok(())
}

#[derive(Args)]
#[command(
    about = "Bootstrap Ralph files in the current repository",
    after_long_help = "Examples:\n  ralph init\n  ralph init --force"
)]
pub struct InitArgs {
    /// Overwrite existing files if they already exist.
    #[arg(long)]
    pub force: bool,
}
