//! PID liveness detection.
//!
//! Purpose:
//! - PID liveness detection.
//!
//! Responsibilities:
//! - Provide tri-state PID liveness helpers.
//! - Encapsulate platform-specific process existence checks.
//!
//! Not handled here:
//! - Lock acquisition or cleanup policy.
//! - Owner metadata parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Indeterminate liveness is treated conservatively by callers.
//! - `Running` means the numeric PID exists now; it does not prove that the
//!   process is the same owner that originally wrote a lock file.

/// Tri-state PID liveness result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidLiveness {
    Running,
    NotRunning,
    Indeterminate,
}

impl PidLiveness {
    pub fn is_definitely_not_running(self) -> bool {
        matches!(self, Self::NotRunning)
    }

    pub fn is_running_or_indeterminate(self) -> bool {
        matches!(self, Self::Running | Self::Indeterminate)
    }
}

pub fn pid_liveness(pid: u32) -> PidLiveness {
    match pid_is_running(pid) {
        Some(true) => PidLiveness::Running,
        Some(false) => PidLiveness::NotRunning,
        None => PidLiveness::Indeterminate,
    }
}

#[cfg(windows)]
fn pid_exists_via_toolhelp(pid: u32) -> Option<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
    };

    // SAFETY: The ToolHelp snapshot APIs return OS-owned handles; we initialize
    // the documented structure size, check each return value, and close the
    // snapshot handle before returning.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            log::debug!(
                "CreateToolhelp32Snapshot failed for PID existence check, error: {}",
                windows_sys::Win32::Foundation::GetLastError()
            );
            return None;
        }

        let result = {
            let mut entry: PROCESSENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

            if Process32First(snapshot, &mut entry) == 0 {
                log::debug!(
                    "Process32First failed, error: {}",
                    windows_sys::Win32::Foundation::GetLastError()
                );
                None
            } else {
                let mut found = false;
                loop {
                    if entry.th32ProcessID == pid {
                        found = true;
                        break;
                    }
                    if Process32Next(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
                Some(found)
            }
        };

        CloseHandle(snapshot);
        result
    }
}

pub fn pid_is_running(pid: u32) -> Option<bool> {
    #[cfg(unix)]
    {
        // SAFETY: `kill(pid, 0)` is a read-only liveness probe that does not
        // dereference pointers or mutate Rust-managed memory.
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            return Some(true);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Some(false);
        }
        None
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER,
        };
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};

        // SAFETY: `OpenProcess` returns an OS handle for the queried PID; we
        // check the handle for zero and close it immediately on success.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
            if handle != 0 {
                CloseHandle(handle);
                Some(true)
            } else {
                let error = windows_sys::Win32::Foundation::GetLastError();
                if error == ERROR_INVALID_PARAMETER {
                    Some(false)
                } else if error == ERROR_ACCESS_DENIED {
                    log::debug!(
                        "OpenProcess({}) failed with ERROR_ACCESS_DENIED, falling back to ToolHelp enumeration",
                        pid
                    );
                    pid_exists_via_toolhelp(pid)
                } else {
                    log::debug!(
                        "OpenProcess({}) failed with unexpected error: {}",
                        pid,
                        error
                    );
                    None
                }
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        None
    }
}
