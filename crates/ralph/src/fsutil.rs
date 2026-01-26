//! Filesystem helpers for locks, atomic writes, and temp cleanup.

use crate::timeutil;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug)]
pub struct DirLock {
    lock_dir: PathBuf,
    owner_path: PathBuf,
}

impl Drop for DirLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.owner_path);

        // Best-effort: remove the lock directory if it's empty.
        // - For standard locks, removing the owner file above should leave the directory empty.
        // - For shared "task" locks under supervision, the directory still contains the supervisor's
        //   `owner` file, so this removal fails and the supervisor cleans up when it exits.
        let _ = fs::remove_dir(&self.lock_dir);
    }
}

struct LockOwner {
    pid: u32,
    started_at: String,
    command: String,
    label: String,
}

impl LockOwner {
    fn render(&self) -> String {
        format!(
            "pid: {}\nstarted_at: {}\ncommand: {}\nlabel: {}\n",
            self.pid, self.started_at, self.command, self.label
        )
    }
}

const RALPH_TEMP_DIR_NAME: &str = "ralph";
const LEGACY_PROMPT_PREFIX: &str = "ralph_prompt_";
pub const RALPH_TEMP_PREFIX: &str = "ralph_";

pub fn queue_lock_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph").join("lock")
}

pub fn ralph_temp_root() -> PathBuf {
    std::env::temp_dir().join(RALPH_TEMP_DIR_NAME)
}

fn is_supervising_label(label: &str) -> bool {
    matches!(label, "run one" | "run loop" | "tui")
}

/// Check if the queue lock is currently held by a supervising process
/// (run one or run loop), which means the caller is running under
/// ralph's supervision and should not attempt to acquire the lock.
pub fn is_supervising_process(lock_dir: &Path) -> Result<bool> {
    let owner_path = lock_dir.join("owner");

    let raw = match fs::read_to_string(&owner_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("read lock owner {}", owner_path.display()))
        }
    };

    let owner = match parse_lock_owner(&raw) {
        Some(owner) => owner,
        None => return Ok(false),
    };

    Ok(is_supervising_label(&owner.label))
}

pub fn cleanup_stale_temp_entries(
    base: &Path,
    prefixes: &[&str],
    retention: Duration,
) -> Result<usize> {
    if !base.exists() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut removed = 0usize;

    for entry in fs::read_dir(base).with_context(|| format!("read temp dir {}", base.display()))? {
        let entry = entry.with_context(|| format!("read temp dir entry in {}", base.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                log::warn!(
                    "unable to read temp metadata for {}: {}",
                    path.display(),
                    err
                );
                continue;
            }
        };

        let modified = match metadata.modified() {
            Ok(time) => time,
            Err(err) => {
                log::warn!(
                    "unable to read temp modified time for {}: {}",
                    path.display(),
                    err
                );
                continue;
            }
        };

        let age = match now.duration_since(modified) {
            Ok(age) => age,
            Err(_) => continue,
        };

        if age < retention {
            continue;
        }

        if metadata.is_dir() {
            if fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            } else {
                log::warn!("failed to remove temp dir {}", path.display());
            }
        } else if fs::remove_file(&path).is_ok() {
            removed += 1;
        } else {
            log::warn!("failed to remove temp file {}", path.display());
        }
    }

    Ok(removed)
}

pub fn cleanup_stale_temp_dirs(base: &Path, retention: Duration) -> Result<usize> {
    cleanup_stale_temp_entries(base, &[RALPH_TEMP_PREFIX], retention)
}

pub fn cleanup_default_temp_dirs(retention: Duration) -> Result<usize> {
    let mut removed = 0usize;
    removed += cleanup_stale_temp_dirs(&ralph_temp_root(), retention)?;
    removed +=
        cleanup_stale_temp_entries(&std::env::temp_dir(), &[LEGACY_PROMPT_PREFIX], retention)?;
    Ok(removed)
}

pub fn create_ralph_temp_dir(label: &str) -> Result<tempfile::TempDir> {
    let base = ralph_temp_root();
    fs::create_dir_all(&base).with_context(|| format!("create temp dir {}", base.display()))?;
    let prefix = format!(
        "{prefix}{label}_",
        prefix = RALPH_TEMP_PREFIX,
        label = label.trim()
    );
    let dir = tempfile::Builder::new()
        .prefix(&prefix)
        .tempdir_in(&base)
        .with_context(|| format!("create temp dir in {}", base.display()))?;
    Ok(dir)
}

pub fn safeguard_text_dump(label: &str, content: &str) -> Result<PathBuf> {
    let temp_dir = create_ralph_temp_dir(label)?;
    let output_path = temp_dir.path().join("output.txt");
    fs::write(&output_path, content)
        .with_context(|| format!("write safeguard dump to {}", output_path.display()))?;

    // Persist the temp dir so it's not deleted when the TempDir object is dropped.
    let dir_path = temp_dir.keep();
    Ok(dir_path.join("output.txt"))
}

pub fn acquire_dir_lock(lock_dir: &Path, label: &str, force: bool) -> Result<DirLock> {
    log::debug!(
        "acquiring dir lock: {} (label: {})",
        lock_dir.display(),
        label
    );
    if let Some(parent) = lock_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create lock parent {}", parent.display()))?;
    }

    let trimmed_label = label.trim();
    let is_task_label = trimmed_label == "task";

    match fs::create_dir(lock_dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut owner_unreadable = false;
            let owner = match read_lock_owner(lock_dir) {
                Ok(owner) => owner,
                Err(_) => {
                    owner_unreadable = true;
                    None
                }
            };

            let is_stale = owner
                .as_ref()
                .is_some_and(|o| pid_is_running(o.pid) == Some(false));

            if force && is_stale {
                let _ = fs::remove_dir_all(lock_dir);
                // Retry once
                return acquire_dir_lock(lock_dir, label, false);
            }

            // Shared lock mode: "task" label can coexist with supervising lock
            if is_task_label
                && owner
                    .as_ref()
                    .is_some_and(|o| is_supervising_label(&o.label))
            {
                // Proceed to create sidecar owner file below
            } else {
                let msg = format_lock_error(lock_dir, owner.as_ref(), is_stale, owner_unreadable);
                return Err(anyhow!(msg));
            }
        }
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("create lock dir {}", lock_dir.display()));
        }
    }

    let effective_label = if trimmed_label.is_empty() {
        "unspecified"
    } else {
        trimmed_label
    };
    let owner = LockOwner {
        pid: std::process::id(),
        started_at: timeutil::now_utc_rfc3339()?,
        command: command_line(),
        label: effective_label.to_string(),
    };

    // For "task" label in shared lock mode, create sidecar owner file
    let owner_path = if is_task_label && lock_dir.exists() {
        lock_dir.join(format!("owner_task_{}", std::process::id()))
    } else {
        lock_dir.join("owner")
    };

    if let Err(err) = write_lock_owner(&owner_path, &owner) {
        let _ = fs::remove_file(&owner_path);

        // Best-effort cleanup: if the lock directory is empty, remove it.
        // This prevents task lock attempts from leaving an empty `.ralph/lock` behind on errors.
        let _ = fs::remove_dir(lock_dir);

        return Err(err);
    }

    Ok(DirLock {
        lock_dir: lock_dir.to_path_buf(),
        owner_path,
    })
}

fn format_lock_error(
    lock_dir: &Path,
    owner: Option<&LockOwner>,
    is_stale: bool,
    owner_unreadable: bool,
) -> String {
    let mut msg = format!("Queue lock already held at: {}", lock_dir.display());
    if is_stale {
        msg.push_str(" (STALE PID)");
    }
    if owner_unreadable {
        msg.push_str(" (owner metadata unreadable)");
    }

    msg.push_str("\n\nLock Holder:");
    if let Some(owner) = owner {
        msg.push_str(&format!(
            "\n  PID: {}{}\n  Label: {}\n  Started At: {}\n  Command: {}",
            owner.pid,
            if is_stale { " (not running)" } else { "" },
            owner.label,
            owner.started_at,
            owner.command
        ));
    } else {
        msg.push_str("\n  (owner metadata missing)");
    }

    msg.push_str("\n\nSuggested Action:");
    if is_stale {
        msg.push_str(&format!(
            "\n  The process that held this lock is no longer running.\n  Use --force to automatically clear it, or remove the directory manually:\n  rm -rf {}",
            lock_dir.display()
        ));
    } else {
        msg.push_str(&format!(
            "\n  If you are sure no other ralph process is running, remove the lock directory:\n  rm -rf {}",
            lock_dir.display()
        ));
    }
    msg
}

fn write_lock_owner(owner_path: &Path, owner: &LockOwner) -> Result<()> {
    log::debug!("writing lock owner: {}", owner_path.display());
    let mut file = fs::File::create(owner_path)
        .with_context(|| format!("create lock owner {}", owner_path.display()))?;
    file.write_all(owner.render().as_bytes())
        .context("write lock owner")?;
    file.flush().context("flush lock owner")?;
    file.sync_all().context("sync lock owner")?;
    if let Some(parent) = owner_path.parent() {
        sync_dir_best_effort(parent);
    }
    Ok(())
}

fn read_lock_owner(lock_dir: &Path) -> Result<Option<LockOwner>> {
    let owner_path = lock_dir.join("owner");
    let raw = match fs::read_to_string(&owner_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(anyhow!(err))
                .with_context(|| format!("read lock owner {}", owner_path.display()))
        }
    };
    Ok(parse_lock_owner(&raw))
}

fn parse_lock_owner(raw: &str) -> Option<LockOwner> {
    let mut pid = None;
    let mut started_at = None;
    let mut command = None;
    let mut label = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim().to_string();
            match key.trim() {
                "pid" => pid = value.parse::<u32>().ok(),
                "started_at" => started_at = Some(value),
                "command" => command = Some(value),
                "label" => label = Some(value),
                _ => {}
            }
        }
    }

    let pid = pid?;
    Some(LockOwner {
        pid,
        started_at: started_at.unwrap_or_else(|| "unknown".to_string()),
        command: command.unwrap_or_else(|| "unknown".to_string()),
        label: label.unwrap_or_else(|| "unknown".to_string()),
    })
}

fn pid_is_running(pid: u32) -> Option<bool> {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            return Some(true);
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Some(false);
        }
        None
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

fn command_line() -> String {
    let args: Vec<String> = std::env::args().collect();
    let joined = args.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    log::debug!("atomic write: {}", path.display());
    let dir = path
        .parent()
        .context("atomic write requires a parent directory")?;
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
        log::debug!("syncing directory: {}", dir.display());
        if let Ok(file) = fs::File::open(dir) {
            let _ = file.sync_all();
        }
    }

    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}
