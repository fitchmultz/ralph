//! macOS app integration command implementations.
//!
//! Responsibilities:
//! - Implement `ralph app open` by launching the installed SwiftUI app via the macOS `open`
//!   command.
//! - Keep the invocation logic testable by separating "plan" (what command would run) from
//!   execution (spawning the process).
//!
//! Not handled here:
//! - Building or installing the SwiftUI app (see `apps/RalphMac/`).
//! - Any in-app IPC; the app drives Ralph by executing the CLI as a subprocess.
//!
//! Invariants/assumptions:
//! - The default bundle identifier is `com.mitchfultz.ralph`.
//! - Non-macOS platforms reject `ralph app open` with a clear error and non-zero exit.

use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::cli::app::AppOpenArgs;

const DEFAULT_BUNDLE_ID: &str = "com.mitchfultz.ralph";

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenCommandSpec {
    program: OsString,
    args: Vec<OsString>,
}

impl OpenCommandSpec {
    fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }
}

fn plan_open_command(is_macos: bool, args: &AppOpenArgs) -> Result<OpenCommandSpec> {
    if !is_macos {
        bail!("`ralph app open` is macOS-only.");
    }

    if args.path.is_some() && args.bundle_id.is_some() {
        bail!("--path and --bundle-id cannot be used together.");
    }

    if let Some(path) = args.path.as_deref() {
        ensure_exists(path)?;
        return Ok(OpenCommandSpec {
            program: OsString::from("open"),
            args: vec![OsString::from(path)],
        });
    }

    let bundle_id = args
        .bundle_id
        .as_deref()
        .unwrap_or(DEFAULT_BUNDLE_ID)
        .trim();
    if bundle_id.is_empty() {
        bail!("Bundle id is empty.");
    }

    Ok(OpenCommandSpec {
        program: OsString::from("open"),
        args: vec![OsString::from("-b"), OsString::from(bundle_id)],
    })
}

fn ensure_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    bail!("Path does not exist: {}", path.display());
}

/// Open the Ralph macOS app.
///
/// On macOS, this launches the installed app via the system `open` command.
/// On non-macOS platforms, this returns an error.
pub fn open(args: AppOpenArgs) -> Result<()> {
    let spec = plan_open_command(cfg!(target_os = "macos"), &args)?;
    let status = spec
        .to_command()
        .status()
        .context("spawn macOS `open` command")?;

    if status.success() {
        return Ok(());
    }

    bail!("Failed to launch app (exit status: {status}).");
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_BUNDLE_ID, plan_open_command};
    use crate::cli::app::AppOpenArgs;
    use std::ffi::{OsStr, OsString};
    use std::path::PathBuf;

    #[test]
    fn plan_open_command_non_macos_errors() {
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
        };

        let err = plan_open_command(false, &args).expect_err("expected error");
        assert!(
            err.to_string().to_lowercase().contains("macos-only"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn plan_open_command_default_bundle_id_uses_open_b() -> anyhow::Result<()> {
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
        };

        let spec = plan_open_command(true, &args)?;
        assert_eq!(spec.program, OsString::from("open"));
        assert_eq!(
            spec.args,
            vec![
                OsStr::new("-b").to_os_string(),
                OsStr::new(DEFAULT_BUNDLE_ID).to_os_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn plan_open_command_bundle_id_override_uses_open_b() -> anyhow::Result<()> {
        let args = AppOpenArgs {
            bundle_id: Some("com.example.override".to_string()),
            path: None,
        };

        let spec = plan_open_command(true, &args)?;
        assert_eq!(spec.program, OsString::from("open"));
        assert_eq!(
            spec.args,
            vec![
                OsStr::new("-b").to_os_string(),
                OsStr::new("com.example.override").to_os_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn plan_open_command_path_uses_open_path() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let app_dir = temp.path().join("Ralph.app");
        std::fs::create_dir_all(&app_dir)?;

        let args = AppOpenArgs {
            bundle_id: None,
            path: Some(app_dir.clone()),
        };

        let spec = plan_open_command(true, &args)?;
        assert_eq!(spec.program, OsString::from("open"));
        assert_eq!(spec.args, vec![app_dir.as_os_str().to_os_string()]);
        Ok(())
    }

    #[test]
    fn plan_open_command_path_missing_errors() {
        let args = AppOpenArgs {
            bundle_id: None,
            path: Some(PathBuf::from("/definitely/not/a/real/path/Ralph.app")),
        };

        let err = plan_open_command(true, &args).expect_err("expected error");
        assert!(
            err.to_string().to_lowercase().contains("does not exist"),
            "unexpected error: {err:#}"
        );
    }
}
