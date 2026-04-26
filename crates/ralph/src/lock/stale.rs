//! Stale-lock policy.
//!
//! Purpose:
//! - Stale-lock policy.
//!
//! Responsibilities:
//! - Inspect existing lock directories for owner readability and stale ownership.
//! - Format actionable lock contention error messages.
//!
//! Not handled here:
//! - Lock directory cleanup or owner-file writes.
//! - PID liveness implementation details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - A lock is automatically stale only when owner metadata exists and the
//!   owner PID is definitively dead.
//! - A live or indeterminate PID is preserved even when `started_at` looks
//!   suspicious; PID reuse means numeric liveness cannot prove process identity.
//! - Suspicious owner timestamps produce operator-visible review guidance
//!   instead of automatic cleanup.

use super::owner::LockOwner;
use super::pid::{PidLiveness, pid_liveness};
use crate::timeutil;
use anyhow::Result;
use std::path::Path;
use time::{Duration, OffsetDateTime};

const PID_REUSE_REVIEW_AFTER_SECONDS: i64 = 7 * 24 * 60 * 60;
const FUTURE_STARTED_AT_GRACE_SECONDS: i64 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockStalenessAdvisory {
    None,
    InvalidStartedAt,
    FutureStartedAt,
    AgedLivePid,
}

impl LockStalenessAdvisory {
    pub(crate) fn marker(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::InvalidStartedAt | Self::FutureStartedAt => Some("OWNER TIME REVIEW"),
            Self::AgedLivePid => Some("PID REUSE REVIEW"),
        }
    }

    fn operator_note(self, liveness: PidLiveness) -> Option<String> {
        let liveness_text = match liveness {
            PidLiveness::Running => "appears to be running",
            PidLiveness::Indeterminate => "could not be checked conclusively",
            PidLiveness::NotRunning => "is no longer running",
        };

        match self {
            Self::None => None,
            Self::InvalidStartedAt => Some(format!(
                "  The owner `started_at` value is missing or invalid, so Ralph cannot use lock age as a PID-reuse signal. The owner PID {liveness_text}; Ralph preserves the lock until an operator verifies it."
            )),
            Self::FutureStartedAt => Some(format!(
                "  The owner `started_at` value is more than {} minutes in the future. The owner PID {liveness_text}; Ralph preserves the lock and requires operator verification before unlock.",
                FUTURE_STARTED_AT_GRACE_SECONDS / 60
            )),
            Self::AgedLivePid => Some(format!(
                "  The owner `started_at` value is older than {} days while the owner PID {liveness_text}. This can be a long-running Ralph process or a reused PID, so Ralph does not auto-clear it; verify the PID, command, and timestamp before using `ralph queue unlock`.",
                PID_REUSE_REVIEW_AFTER_SECONDS / 60 / 60 / 24
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LockStaleness {
    pub(crate) liveness: PidLiveness,
    pub(crate) advisory: LockStalenessAdvisory,
}

impl LockStaleness {
    pub(crate) fn is_stale(self) -> bool {
        self.liveness.is_definitely_not_running()
    }

    pub(crate) fn advisory_note(self) -> Option<String> {
        self.advisory.operator_note(self.liveness)
    }
}

pub(crate) struct ExistingLock {
    pub(crate) owner: Option<LockOwner>,
    pub(crate) owner_unreadable: bool,
    pub(crate) is_stale: bool,
    pub(crate) staleness: Option<LockStaleness>,
}

pub(crate) fn inspect_existing_lock(
    lock_dir: &Path,
    read_owner: impl FnOnce(&Path) -> Result<Option<LockOwner>>,
) -> ExistingLock {
    match read_owner(lock_dir) {
        Ok(owner) => {
            let staleness = owner.as_ref().map(classify_lock_owner);
            let is_stale = staleness.is_some_and(LockStaleness::is_stale);
            ExistingLock {
                owner,
                owner_unreadable: false,
                is_stale,
                staleness,
            }
        }
        Err(_) => ExistingLock {
            owner: None,
            owner_unreadable: true,
            is_stale: false,
            staleness: None,
        },
    }
}

pub(crate) fn classify_lock_owner(owner: &LockOwner) -> LockStaleness {
    classify_lock_owner_at(owner, OffsetDateTime::now_utc(), pid_liveness(owner.pid))
}

pub(crate) fn classify_lock_owner_at(
    owner: &LockOwner,
    now: OffsetDateTime,
    liveness: PidLiveness,
) -> LockStaleness {
    if liveness.is_definitely_not_running() {
        return LockStaleness {
            liveness,
            advisory: LockStalenessAdvisory::None,
        };
    }

    let advisory = match timeutil::parse_rfc3339_opt(&owner.started_at) {
        None => LockStalenessAdvisory::InvalidStartedAt,
        Some(started_at) if started_at - now > future_started_at_grace() => {
            LockStalenessAdvisory::FutureStartedAt
        }
        Some(started_at) if now - started_at > pid_reuse_review_after() => {
            LockStalenessAdvisory::AgedLivePid
        }
        Some(_) => LockStalenessAdvisory::None,
    };

    LockStaleness { liveness, advisory }
}

fn pid_reuse_review_after() -> Duration {
    Duration::seconds(PID_REUSE_REVIEW_AFTER_SECONDS)
}

fn future_started_at_grace() -> Duration {
    Duration::seconds(FUTURE_STARTED_AT_GRACE_SECONDS)
}

pub(crate) fn format_lock_error(
    lock_dir: &Path,
    owner: Option<&LockOwner>,
    is_stale: bool,
    owner_unreadable: bool,
    staleness: Option<LockStaleness>,
) -> String {
    let mut message = format!("Queue lock already held at: {}", lock_dir.display());
    if is_stale {
        message.push_str(" (STALE PID)");
    } else if let Some(marker) = staleness.and_then(|staleness| staleness.advisory.marker()) {
        message.push_str(&format!(" ({marker})"));
    }
    if owner_unreadable {
        message.push_str(" (owner metadata unreadable)");
    }

    message.push_str("\n\nLock Holder:");
    if let Some(owner) = owner {
        message.push_str(&format!(
            "\n  PID: {}\n  Label: {}\n  Started At: {}\n  Command: {}",
            owner.pid, owner.label, owner.started_at, owner.command
        ));
    } else {
        message.push_str("\n  (owner metadata missing)");
    }

    if is_stale {
        message.push_str(
            "\n\nStaleness Policy:\n  Ralph automatically treats and clears a PID lock as stale only when the owner PID is definitely not running.",
        );
    } else if let Some(note) = staleness.and_then(LockStaleness::advisory_note) {
        message.push_str("\n\nStaleness Policy:\n");
        message.push_str(&note);
    }

    message.push_str("\n\nSuggested Action:");
    if is_stale {
        message.push_str(
            "\n  The process that held this lock is no longer running. Ralph normally auto-clears this verified stale lock before acquiring the queue lock. If this message persists, use the built-in unlock command:\n  ralph queue unlock --yes",
        );
    } else {
        message.push_str(
            "\n  If you are sure no other ralph process is running, use the built-in unlock command:\n  ralph queue unlock",
        );
    }

    message
}
