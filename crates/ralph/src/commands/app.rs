//! macOS app integration command implementations.
//!
//! Responsibilities:
//! - Implement `ralph app open` by launching the installed SwiftUI app via the macOS `open`
//!   command.
//! - Pass workspace context via custom URL scheme `ralph://open?workspace=<path>`
//! - Keep the invocation logic testable by separating "plan" from execution.
//!
//! Not handled here:
//! - Building or installing the SwiftUI app (see `apps/RalphMac/`).
//! - Any in-app IPC; the app drives Ralph by executing the CLI as a subprocess.
//!
//! Invariants/assumptions:
//! - The default bundle identifier is `com.mitchfultz.ralph`.
//! - Non-macOS platforms reject `ralph app open` with a clear error and non-zero exit.
//! - URL scheme `ralph://` must be registered in the app's Info.plist.

use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

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

/// Plan the URL command to send workspace context.
fn plan_url_command(workspace: &Path) -> Result<OpenCommandSpec> {
    let encoded_path = percent_encode_path(workspace);
    let url = format!("ralph://open?workspace={}", encoded_path);

    Ok(OpenCommandSpec {
        program: OsString::from("open"),
        args: vec![OsString::from(url)],
    })
}

/// Percent-encode a path for use in URL query parameters.
#[cfg(unix)]
fn percent_encode_path(path: &Path) -> String {
    percent_encode(path.as_os_str().as_bytes())
}

/// Percent-encode a path for use in URL query parameters (non-Unix fallback).
#[cfg(not(unix))]
fn percent_encode_path(path: &Path) -> String {
    // On non-Unix platforms, convert to UTF-8 string and encode
    percent_encode(path.to_string_lossy().as_bytes())
}

/// Percent-encode a byte sequence for use in URL query parameters.
fn percent_encode(input: &[u8]) -> String {
    let mut result = String::with_capacity(input.len() * 3);
    for &byte in input {
        // Unreserved characters per RFC 3986
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push_str(&format!("{:02X}", byte));
        }
    }
    result
}

/// Open the Ralph macOS app.
///
/// On macOS, this:
/// 1. Launches the installed app via the system `open` command
/// 2. Sends a URL with workspace context via `ralph://open?workspace=<path>`
///
/// The URL is delivered to the running app (or launches it if not running).
pub fn open(args: AppOpenArgs) -> Result<()> {
    // Step 1: Open the app
    let spec = plan_open_command(cfg!(target_os = "macos"), &args)?;
    let status = spec
        .to_command()
        .status()
        .context("spawn macOS `open` command for app launch")?;

    if !status.success() {
        bail!("Failed to launch app (exit status: {status}).");
    }

    // Step 2: Send URL with workspace context
    let workspace = if let Some(ref ws) = args.workspace {
        // User explicitly provided workspace - validate it exists
        if !ws.exists() {
            bail!("Workspace path does not exist: {}", ws.display());
        }
        Some(ws.clone())
    } else {
        // Use current directory
        std::env::current_dir().ok().filter(|p| p.exists())
    };

    if let Some(workspace_path) = workspace {
        let url_spec = plan_url_command(&workspace_path)?;

        // Small delay to ensure app has started registering for URL events
        // This is a best-effort approach; macOS handles URL delivery to running apps
        std::thread::sleep(std::time::Duration::from_millis(100));

        let url_status = url_spec
            .to_command()
            .status()
            .context("spawn macOS `open` command for URL")?;

        if !url_status.success() {
            // Log but don't fail - the app launched, which is the primary goal
            eprintln!("Warning: Failed to send workspace URL to app (exit status: {url_status})");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_BUNDLE_ID, percent_encode, percent_encode_path, plan_open_command, plan_url_command,
    };
    use crate::cli::app::AppOpenArgs;
    use std::ffi::{OsStr, OsString};
    use std::path::PathBuf;

    #[test]
    fn plan_open_command_non_macos_errors() {
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
            workspace: None,
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
            workspace: None,
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
            workspace: None,
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
            workspace: None,
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
            workspace: None,
        };

        let err = plan_open_command(true, &args).expect_err("expected error");
        assert!(
            err.to_string().to_lowercase().contains("does not exist"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn plan_url_command_encodes_workspace() -> anyhow::Result<()> {
        let workspace = PathBuf::from("/Users/test/my project");
        let spec = plan_url_command(&workspace)?;

        assert_eq!(spec.program, OsString::from("open"));
        assert_eq!(spec.args.len(), 1);

        let url = spec.args[0].to_str().unwrap();
        assert!(url.starts_with("ralph://open?workspace="));
        assert!(
            url.contains("my%20project"),
            "space should be percent-encoded"
        );
        Ok(())
    }

    #[test]
    fn plan_url_command_handles_special_chars() -> anyhow::Result<()> {
        let workspace = PathBuf::from("/path/with&special=chars");
        let spec = plan_url_command(&workspace)?;

        let url = spec.args[0].to_str().unwrap();
        assert!(url.contains("%26"), "& should be encoded as %26");
        assert!(url.contains("%3D"), "= should be encoded as %3D");
        Ok(())
    }

    #[test]
    fn percent_encode_preserves_unreserved_chars() {
        let input = b"abc-_.~/123";
        let encoded = percent_encode(input);
        assert_eq!(encoded, "abc-_.~/123");
    }

    #[test]
    fn percent_encode_encodes_reserved_chars() {
        let input = b"hello world";
        let encoded = percent_encode(input);
        assert_eq!(encoded, "hello%20world");
    }

    #[test]
    fn percent_encode_encodes_unicode() {
        let input = "test/文件".as_bytes();
        let encoded = percent_encode(input);
        assert!(encoded.starts_with("test/"));
        assert!(encoded.len() > "test/文件".len()); // Should be encoded
    }

    #[test]
    fn percent_encode_path_handles_spaces() {
        let path = PathBuf::from("/Users/test/my project");
        let encoded = percent_encode_path(&path);
        assert!(encoded.contains("%20"), "spaces should be encoded as %20");
        assert!(
            !encoded.contains(' '),
            "result should not contain literal spaces"
        );
    }

    #[test]
    fn percent_encode_path_preserves_path_structure() {
        let path = PathBuf::from("/path/to/directory");
        let encoded = percent_encode_path(&path);
        assert!(encoded.starts_with("/path/to/"));
        assert!(encoded.contains('/'));
    }
}
