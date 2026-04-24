//! Aging threshold configuration helpers.
//!
//! Purpose:
//! - Aging threshold configuration helpers.
//!
//! Responsibilities:
//! - Resolve aging thresholds from queue config with defaults.
//! - Enforce strict threshold ordering before report computation.
//!
//! Not handled here:
//! - Per-task aging computation.
//! - Report assembly or rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Threshold ordering must remain `warning < stale < rotten`.

use anyhow::{Result, bail};
use time::Duration;

use crate::contracts::QueueConfig;

pub(crate) const DEFAULT_WARNING_DAYS: u32 = 7;
pub(crate) const DEFAULT_STALE_DAYS: u32 = 14;
pub(crate) const DEFAULT_ROTTEN_DAYS: u32 = 30;

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct AgingThresholds {
    pub warning_days: u32,
    pub stale_days: u32,
    pub rotten_days: u32,
}

impl AgingThresholds {
    pub(crate) fn from_queue_config(cfg: &QueueConfig) -> Result<Self> {
        let thresholds = cfg.aging_thresholds.clone();
        let warning = thresholds
            .as_ref()
            .and_then(|value| value.warning_days)
            .unwrap_or(DEFAULT_WARNING_DAYS);
        let stale = thresholds
            .as_ref()
            .and_then(|value| value.stale_days)
            .unwrap_or(DEFAULT_STALE_DAYS);
        let rotten = thresholds
            .as_ref()
            .and_then(|value| value.rotten_days)
            .unwrap_or(DEFAULT_ROTTEN_DAYS);

        if !(warning < stale && stale < rotten) {
            bail!(
                "Invalid queue.aging_thresholds ordering: require warning_days < stale_days < rotten_days (got warning_days={}, stale_days={}, rotten_days={})",
                warning,
                stale,
                rotten
            );
        }

        Ok(Self {
            warning_days: warning,
            stale_days: stale,
            rotten_days: rotten,
        })
    }

    pub(crate) fn warning_dur(self) -> Duration {
        Duration::days(self.warning_days as i64)
    }

    pub(crate) fn stale_dur(self) -> Duration {
        Duration::days(self.stale_days as i64)
    }

    pub(crate) fn rotten_dur(self) -> Duration {
        Duration::days(self.rotten_days as i64)
    }
}

impl Default for AgingThresholds {
    fn default() -> Self {
        Self {
            warning_days: DEFAULT_WARNING_DAYS,
            stale_days: DEFAULT_STALE_DAYS,
            rotten_days: DEFAULT_ROTTEN_DAYS,
        }
    }
}
