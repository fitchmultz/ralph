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
const GUI_CLI_BIN_ENV: &str = "RALPH_BIN_PATH";

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

fn plan_open_command(
    is_macos: bool,
    args: &AppOpenArgs,
    cli_executable: Option<&Path>,
) -> Result<OpenCommandSpec> {
    if !is_macos {
        bail!("`ralph app open` is macOS-only.");
    }

    if args.path.is_some() && args.bundle_id.is_some() {
        bail!("--path and --bundle-id cannot be used together.");
    }

    let mut args_out: Vec<OsString> = Vec::new();
    if let Some(cli_executable) = cli_executable {
        args_out.push(OsString::from("--env"));
        args_out.push(env_assignment_for_path(cli_executable));
    }

    if let Some(path) = args.path.as_deref() {
        ensure_exists(path)?;
        args_out.push(OsString::from(path));
        return Ok(OpenCommandSpec {
            program: OsString::from("open"),
            args: args_out,
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

    args_out.push(OsString::from("-b"));
    args_out.push(OsString::from(bundle_id));

    Ok(OpenCommandSpec {
        program: OsString::from("open"),
        args: args_out,
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

fn current_executable_for_gui() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if exe.exists() { Some(exe) } else { None }
}

#[cfg(unix)]
fn env_assignment_for_path(path: &Path) -> OsString {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let mut bytes = Vec::from(format!("{GUI_CLI_BIN_ENV}=").as_bytes());
    bytes.extend_from_slice(path.as_os_str().as_bytes());
    OsString::from_vec(bytes)
}

#[cfg(not(unix))]
fn env_assignment_for_path(path: &Path) -> OsString {
    OsString::from(format!("{GUI_CLI_BIN_ENV}={}", path.to_string_lossy()))
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

fn resolve_workspace_path(args: &AppOpenArgs) -> Result<Option<std::path::PathBuf>> {
    if let Some(ref workspace) = args.workspace {
        if !workspace.exists() {
            bail!("Workspace path does not exist: {}", workspace.display());
        }
        return Ok(Some(workspace.clone()));
    }

    Ok(std::env::current_dir().ok().filter(|path| path.exists()))
}

/// Open the Ralph macOS app.
///
/// On macOS, this prefers a single URL launch (`ralph://open?...`) when workspace
/// context is available. That lets LaunchServices both launch the app and deliver
/// the workspace in one step, which avoids SwiftUI opening a second scene for a
/// follow-up external-event dispatch.
pub fn open(args: AppOpenArgs) -> Result<()> {
    let cli_executable = current_executable_for_gui();

    let spec = if let Some(workspace_path) = resolve_workspace_path(&args)? {
        plan_url_command(&workspace_path)?
    } else {
        plan_open_command(cfg!(target_os = "macos"), &args, cli_executable.as_deref())?
    };

    let output = spec
        .to_command()
        .output()
        .context("spawn macOS `open` command for app launch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to launch app (exit status: {}). {}",
            output.status,
            stderr.trim()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_BUNDLE_ID, GUI_CLI_BIN_ENV, env_assignment_for_path, percent_encode,
        percent_encode_path, plan_open_command, plan_url_command, resolve_workspace_path,
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

        let err = plan_open_command(false, &args, None).expect_err("expected error");
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

        let spec = plan_open_command(true, &args, None)?;
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

        let spec = plan_open_command(true, &args, None)?;
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

        let spec = plan_open_command(true, &args, None)?;
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

        let err = plan_open_command(true, &args, None).expect_err("expected error");
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

    #[test]
    fn plan_open_command_includes_cli_env_when_provided() -> anyhow::Result<()> {
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
            workspace: None,
        };
        let cli = PathBuf::from("/tmp/ralph-bin");

        let spec = plan_open_command(true, &args, Some(&cli))?;
        assert_eq!(spec.program, OsString::from("open"));
        assert!(spec.args.len() >= 4);
        assert_eq!(spec.args[0], OsString::from("--env"));
        assert_eq!(spec.args[1], env_assignment_for_path(&cli));
        assert_eq!(spec.args[2], OsString::from("-b"));
        assert_eq!(spec.args[3], OsString::from(DEFAULT_BUNDLE_ID));
        Ok(())
    }

    #[test]
    fn plan_url_command_never_includes_cli_param() -> anyhow::Result<()> {
        let workspace = PathBuf::from("/Users/test/workspace");
        let spec = plan_url_command(&workspace)?;

        let url = spec.args[0].to_string_lossy();
        assert!(url.starts_with("ralph://open?workspace="));
        assert!(!url.contains("&cli="));
        Ok(())
    }

    #[test]
    fn env_assignment_prefixes_variable_name() {
        let cli = PathBuf::from("/tmp/ralph");
        let assignment = env_assignment_for_path(&cli);
        let text = assignment.to_string_lossy();
        assert!(text.starts_with(&format!("{GUI_CLI_BIN_ENV}=")));
        assert!(text.ends_with("/tmp/ralph"));
    }

    #[test]
    fn resolve_workspace_path_prefers_explicit_workspace() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
            workspace: Some(temp.path().to_path_buf()),
        };

        let resolved = resolve_workspace_path(&args)?;
        assert_eq!(resolved.as_deref(), Some(temp.path()));
        Ok(())
    }

    #[test]
    fn resolve_workspace_path_errors_for_missing_workspace() {
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
            workspace: Some(PathBuf::from("/definitely/not/a/real/workspace")),
        };

        let err = resolve_workspace_path(&args).expect_err("expected error");
        assert!(
            err.to_string().contains("Workspace path does not exist"),
            "unexpected error: {err:#}"
        );
    }
}
