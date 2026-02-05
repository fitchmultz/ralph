//! External tool integrations for the TUI.
//!
//! Responsibilities:
//! - Launch an external editor for a set of scope paths.
//! - Copy plain text to the system clipboard.
//!
//! Does NOT handle:
//! - Selecting tasks or extracting refs from task content (done elsewhere).
//! - Suspending/restoring terminal raw mode for interactive editors.
//!
//! Invariants/assumptions:
//! - Best-effort: failures return errors for user-visible status messages.
//! - Clipboard copy prefers platform tools; if none exist, returns a clear error.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) fn repo_root_from_queue_path(queue_path: &Path) -> Option<PathBuf> {
    queue_path.parent()?.parent().map(|p| p.to_path_buf())
}

pub(crate) fn resolve_scope_paths(repo_root: Option<&Path>, scope: &[String]) -> Vec<String> {
    scope
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let p = Path::new(s);
            if p.is_absolute() {
                s.to_string()
            } else if let Some(root) = repo_root {
                root.join(p).to_string_lossy().to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}

pub(crate) fn open_paths_in_editor(paths: &[String]) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| "code".to_string());

    // Explicitly do NOT invoke a shell. Treat the env var as a program name.
    // If users want args, they should wrap via a small script and set $EDITOR to that script.
    let mut cmd = Command::new(&editor);
    cmd.args(paths)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn()
        .with_context(|| format!("spawn editor `{}`", editor))?;

    Ok(())
}

fn run_clipboard_cmd(mut cmd: Command, text: &str) -> Result<()> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = cmd.spawn().context("spawn clipboard command")?;
    {
        let mut stdin = child.stdin.take().context("open clipboard stdin")?;
        use std::io::Write;
        stdin
            .write_all(text.as_bytes())
            .context("write clipboard text")?;
    }
    let status = child.wait().context("wait clipboard command")?;
    if !status.success() {
        bail!("clipboard command failed with status {}", status);
    }
    Ok(())
}

pub(crate) fn copy_text_to_clipboard(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        bail!("refusing to copy empty text");
    }

    #[cfg(target_os = "macos")]
    {
        run_clipboard_cmd(Command::new("pbcopy"), text).context("copy via pbcopy")
    }

    #[cfg(target_os = "windows")]
    {
        // `clip` is typically available; use cmd for consistent resolution.
        return run_clipboard_cmd(Command::new("cmd").args(["/C", "clip"]), text)
            .context("copy via clip");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Wayland first if present.
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            if run_clipboard_cmd(Command::new("wl-copy"), text).is_ok() {
                return Ok(());
            }
        }

        if run_clipboard_cmd(
            Command::new("xclip").args(["-selection", "clipboard"]),
            text,
        )
        .is_ok()
        {
            return Ok(());
        }

        if run_clipboard_cmd(Command::new("xsel").args(["-i", "-b"]), text).is_ok() {
            return Ok(());
        }

        bail!("no clipboard command available (tried wl-copy, xclip, xsel)");
    }
}

/// Open a URL in the user's default browser.
///
/// Responsibilities:
/// - Best-effort spawn of platform browser opener commands.
///
/// Does NOT handle:
/// - Validating URL reachability.
/// - Complex URL normalization (callers should provide a usable URL).
///
/// Invariants/assumptions:
/// - Refuses empty/whitespace URLs.
/// - Avoids invoking a shell except where required on Windows (`cmd /C start`).
pub(crate) fn open_url_in_browser(url: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty() {
        bail!("refusing to open empty url");
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn `open`")?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        // `start` is a cmd builtin; the empty string is the window title.
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn `cmd /C start`")?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn `xdg-open`")?;
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_scope_paths_relative_to_root() {
        let root = Path::new("/repo");
        let scope = vec!["src/main.rs".to_string(), "/abs/file.rs".to_string()];
        let paths = resolve_scope_paths(Some(root), &scope);
        assert_eq!(paths, vec!["/repo/src/main.rs", "/abs/file.rs"]);
    }

    #[test]
    fn resolves_scope_paths_without_root() {
        let scope = vec!["src/main.rs".to_string()];
        let paths = resolve_scope_paths(None, &scope);
        assert_eq!(paths, vec!["src/main.rs"]);
    }

    #[test]
    fn filters_empty_scope_paths() {
        let root = Path::new("/repo");
        let scope = vec!["src/main.rs".to_string(), "".to_string(), "  ".to_string()];
        let paths = resolve_scope_paths(Some(root), &scope);
        assert_eq!(paths, vec!["/repo/src/main.rs"]);
    }

    #[test]
    fn repo_root_from_queue_path_works() {
        let queue_path = Path::new("/repo/.ralph/queue.json");
        let root = repo_root_from_queue_path(queue_path);
        assert_eq!(root, Some(PathBuf::from("/repo")));
    }

    #[test]
    fn repo_root_from_queue_path_none_for_short() {
        let queue_path = Path::new("queue.json");
        let root = repo_root_from_queue_path(queue_path);
        assert_eq!(root, None);
    }

    #[test]
    fn refuses_empty_clipboard_text() {
        let err = copy_text_to_clipboard("   ").unwrap_err().to_string();
        assert!(err.contains("empty"));
    }

    #[test]
    fn refuses_empty_url() {
        let err = open_url_in_browser("   ").unwrap_err().to_string();
        assert!(err.contains("empty url"));
    }
}
