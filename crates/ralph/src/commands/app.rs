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
//! - Standard installed app locations are `/Applications/RalphMac.app` and
//!   `~/Applications/RalphMac.app`.
//! - Non-macOS platforms reject `ralph app open` with a clear error and non-zero exit.
//! - URL scheme `ralph://` must be registered in the app's Info.plist.

use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use crate::cli::app::AppOpenArgs;
use crate::runutil::{ManagedCommand, TimeoutClass, execute_checked_command};

const DEFAULT_BUNDLE_ID: &str = "com.mitchfultz.ralph";
const DEFAULT_APP_NAME: &str = "RalphMac.app";
const GUI_CLI_BIN_ENV: &str = "RALPH_BIN_PATH";

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenCommandSpec {
    program: OsString,
    args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchTarget {
    AppPath(PathBuf),
    BundleId(String),
}

impl OpenCommandSpec {
    fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }
}

fn execute_launch_command(spec: &OpenCommandSpec) -> Result<()> {
    execute_checked_command(ManagedCommand::new(
        spec.to_command(),
        "launch macOS app",
        TimeoutClass::AppLaunch,
    ))
    .context("spawn macOS app launch command")?;
    Ok(())
}

fn plan_open_command(
    is_macos: bool,
    args: &AppOpenArgs,
    cli_executable: Option<&Path>,
) -> Result<OpenCommandSpec> {
    plan_open_command_with_installed_path(
        is_macos,
        args,
        cli_executable,
        default_installed_app_path(),
    )
}

fn plan_open_command_with_installed_path(
    is_macos: bool,
    args: &AppOpenArgs,
    cli_executable: Option<&Path>,
    installed_app_path: Option<PathBuf>,
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

    let launch_target = resolve_launch_target(args, installed_app_path)?;
    append_open_launch_target_args(&mut args_out, &launch_target);

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
fn plan_url_command(workspace: &Path, args: &AppOpenArgs) -> Result<OpenCommandSpec> {
    plan_url_command_with_installed_path(workspace, args, default_installed_app_path())
}

fn plan_url_command_with_installed_path(
    workspace: &Path,
    args: &AppOpenArgs,
    installed_app_path: Option<PathBuf>,
) -> Result<OpenCommandSpec> {
    let encoded_path = percent_encode_path(workspace);
    let url = format!("ralph://open?workspace={}", encoded_path);
    let launch_target = resolve_launch_target(args, installed_app_path)?;

    Ok(match launch_target {
        LaunchTarget::AppPath(path) => plan_applescript_url_command(&path, &url),
        LaunchTarget::BundleId(bundle_id) => OpenCommandSpec {
            program: OsString::from("open"),
            args: vec![
                OsString::from("-b"),
                OsString::from(bundle_id),
                OsString::from(url),
            ],
        },
    })
}

fn resolve_launch_target(
    args: &AppOpenArgs,
    installed_app_path: Option<PathBuf>,
) -> Result<LaunchTarget> {
    if let Some(path) = args.path.as_deref() {
        ensure_exists(path)?;
        return Ok(LaunchTarget::AppPath(path.to_path_buf()));
    }

    if let Some(bundle_id) = args.bundle_id.as_deref() {
        let bundle_id = bundle_id.trim();
        if bundle_id.is_empty() {
            bail!("Bundle id is empty.");
        }

        return Ok(LaunchTarget::BundleId(bundle_id.to_string()));
    }

    if let Some(path) = installed_app_path {
        return Ok(LaunchTarget::AppPath(path));
    }

    let bundle_id = DEFAULT_BUNDLE_ID.trim();
    if bundle_id.is_empty() {
        bail!("Bundle id is empty.");
    }

    Ok(LaunchTarget::BundleId(bundle_id.to_string()))
}

fn append_open_launch_target_args(args_out: &mut Vec<OsString>, launch_target: &LaunchTarget) {
    match launch_target {
        LaunchTarget::AppPath(path) => {
            args_out.push(OsString::from("-a"));
            args_out.push(path.as_os_str().to_os_string());
        }
        LaunchTarget::BundleId(bundle_id) => {
            args_out.push(OsString::from("-b"));
            args_out.push(OsString::from(bundle_id));
        }
    }
}

fn plan_applescript_url_command(app_path: &Path, url: &str) -> OpenCommandSpec {
    OpenCommandSpec {
        program: OsString::from("osascript"),
        args: vec![
            OsString::from("-e"),
            OsString::from("on run argv"),
            OsString::from("-e"),
            OsString::from("tell application (item 1 of argv) to open location (item 2 of argv)"),
            OsString::from("-e"),
            OsString::from("end run"),
            app_path.as_os_str().to_os_string(),
            OsString::from(url),
        ],
    }
}

fn default_installed_app_path() -> Option<PathBuf> {
    installed_app_candidates()
        .into_iter()
        .find(|candidate| candidate.exists())
}

fn installed_app_candidates() -> Vec<PathBuf> {
    installed_app_candidates_for_home(std::env::var_os("HOME").map(PathBuf::from))
}

fn installed_app_candidates_for_home(home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications").join(DEFAULT_APP_NAME)];
    if let Some(home) = home {
        candidates.push(home.join("Applications").join(DEFAULT_APP_NAME));
    }
    candidates
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
/// On macOS, this always launches the app bundle first so the primary workspace
/// window is guaranteed to exist on cold start. If workspace context is available,
/// a follow-up URL handoff repurposes that bootstrap window for the requested
/// workspace.
pub fn open(args: AppOpenArgs) -> Result<()> {
    let cli_executable = current_executable_for_gui();
    let open_spec = plan_open_command(cfg!(target_os = "macos"), &args, cli_executable.as_deref())?;
    execute_launch_command(&open_spec)?;

    let Some(workspace_path) = resolve_workspace_path(&args)? else {
        return Ok(());
    };

    let url_spec = plan_url_command(&workspace_path, &args)?;
    let mut last_error = None;
    for attempt in 0..10 {
        match execute_launch_command(&url_spec) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < 9 {
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
    }

    Err(last_error.expect("url launch attempts should record an error"))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn installed_app_candidates_prioritize_system_then_home() {
        let home = PathBuf::from("/Users/tester");
        let candidates = installed_app_candidates_for_home(Some(home.clone()));

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/Applications").join(DEFAULT_APP_NAME),
                home.join("Applications").join(DEFAULT_APP_NAME),
            ]
        );
    }

    #[test]
    fn plan_open_command_bundle_id_override_uses_open_b_when_no_installed_app() -> anyhow::Result<()>
    {
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
    fn plan_open_command_path_uses_open_a() -> anyhow::Result<()> {
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
        assert_eq!(
            spec.args,
            vec![
                OsStr::new("-a").to_os_string(),
                app_dir.as_os_str().to_os_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn plan_open_command_default_prefers_injected_installed_app_path() -> anyhow::Result<()> {
        let app_dir = PathBuf::from("/tmp/test/Applications").join(DEFAULT_APP_NAME);
        let args = AppOpenArgs {
            bundle_id: None,
            path: None,
            workspace: None,
        };

        let spec = plan_open_command_with_installed_path(true, &args, None, Some(app_dir.clone()))?;

        assert_eq!(spec.program, OsString::from("open"));
        assert_eq!(
            spec.args,
            vec![
                OsStr::new("-a").to_os_string(),
                app_dir.as_os_str().to_os_string()
            ]
        );
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
        let spec = plan_url_command_with_installed_path(
            &workspace,
            &AppOpenArgs {
                bundle_id: None,
                path: None,
                workspace: None,
            },
            Some(PathBuf::from("/Applications").join(DEFAULT_APP_NAME)),
        )?;

        assert_eq!(spec.program, OsString::from("osascript"));
        assert_eq!(spec.args.len(), 8);

        let url = spec.args[7].to_str().unwrap();
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
        let spec = plan_url_command_with_installed_path(
            &workspace,
            &AppOpenArgs {
                bundle_id: None,
                path: None,
                workspace: None,
            },
            Some(PathBuf::from("/Applications").join(DEFAULT_APP_NAME)),
        )?;

        let url = spec.args[7].to_str().unwrap();
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
        let cli = crate::testsupport::path::portable_abs_path("ralph-bin");

        let spec = plan_open_command(true, &args, Some(&cli))?;
        assert_eq!(spec.program, OsString::from("open"));
        assert!(spec.args.len() >= 4);
        assert_eq!(spec.args[0], OsString::from("--env"));
        assert_eq!(spec.args[1], env_assignment_for_path(&cli));
        assert!(
            spec.args[2] == "-a" || spec.args[2] == "-b",
            "unexpected launch args: {:?}",
            spec.args
        );
        Ok(())
    }

    #[test]
    fn plan_url_command_never_includes_cli_param() -> anyhow::Result<()> {
        let workspace = PathBuf::from("/Users/test/workspace");
        let spec = plan_url_command_with_installed_path(
            &workspace,
            &AppOpenArgs {
                bundle_id: None,
                path: None,
                workspace: None,
            },
            Some(PathBuf::from("/Applications").join(DEFAULT_APP_NAME)),
        )?;

        let url = spec.args.last().unwrap().to_string_lossy();
        assert!(url.starts_with("ralph://open?workspace="));
        assert!(!url.contains("&cli="));
        Ok(())
    }

    #[test]
    fn plan_url_command_prefers_installed_app_path_over_bundle_lookup() -> anyhow::Result<()> {
        let app_dir = PathBuf::from("/tmp/test/Applications").join(DEFAULT_APP_NAME);
        let workspace = PathBuf::from("/Users/test/workspace");
        let spec = plan_url_command_with_installed_path(
            &workspace,
            &AppOpenArgs {
                bundle_id: None,
                path: None,
                workspace: None,
            },
            Some(app_dir.clone()),
        )?;

        assert_eq!(spec.program, OsString::from("osascript"));
        assert_eq!(spec.args[6], app_dir.as_os_str().to_os_string());
        assert!(
            spec.args[7]
                .to_string_lossy()
                .starts_with("ralph://open?workspace=")
        );
        Ok(())
    }

    #[test]
    fn plan_url_command_bundle_id_uses_open_launcher() -> anyhow::Result<()> {
        let workspace = PathBuf::from("/Users/test/workspace");
        let spec = plan_url_command_with_installed_path(
            &workspace,
            &AppOpenArgs {
                bundle_id: Some("com.example.override".to_string()),
                path: None,
                workspace: None,
            },
            Some(PathBuf::from("/Applications").join(DEFAULT_APP_NAME)),
        )?;

        assert_eq!(spec.program, OsString::from("open"));
        assert_eq!(spec.args[0], OsString::from("-b"));
        assert_eq!(spec.args[1], OsString::from("com.example.override"));
        assert!(
            spec.args[2]
                .to_string_lossy()
                .starts_with("ralph://open?workspace=")
        );
        Ok(())
    }

    #[test]
    fn env_assignment_prefixes_variable_name() {
        let cli = crate::testsupport::path::portable_abs_path("ralph");
        let assignment = env_assignment_for_path(&cli);
        let text = assignment.to_string_lossy();
        assert!(text.starts_with(&format!("{GUI_CLI_BIN_ENV}=")));
        assert!(text.ends_with(&*cli.to_string_lossy()));
    }

    #[cfg(unix)]
    #[test]
    fn execute_launch_command_surfaces_launcher_failure() {
        let spec = OpenCommandSpec {
            program: OsString::from("/bin/sh"),
            args: vec![
                OsString::from("-c"),
                OsString::from("printf 'launch failed' >&2; exit 9"),
            ],
        };

        let err = execute_launch_command(&spec).expect_err("expected launcher failure");
        let text = format!("{err:#}");
        assert!(text.contains("spawn macOS app launch command"));
        assert!(text.contains("launch failed"));
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
