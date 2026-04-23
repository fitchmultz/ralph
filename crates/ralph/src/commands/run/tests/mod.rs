//! Run command test hub.
//!
//! Responsibilities:
//! - Organize run-command unit tests by behavior area.
//! - Re-export shared fixtures, builders, and logger helpers for sibling suites.
//!
//! Not handled here:
//! - Individual test scenarios, which live in adjacent modules.
//! - Production run orchestration logic.
//!
//! Invariants/assumptions:
//! - This module stays a thin hub; shared helpers live in focused companion files.
//! - Sibling test modules import shared helpers through `super::*` re-exports.

mod builders;
mod logger;
mod support;
mod task_fixtures;

pub(super) use builders::{
    overrides_with_notifications, resolved_with_agent_defaults, resolved_with_notification_config,
    resolved_with_repo_root, test_config_agent, test_overrides_with_phases, test_task_agent,
};
pub(super) use logger::{LoggerState, take_logs};
pub(super) use support::find_definitely_dead_pid;
pub(super) use task_fixtures::{base_task, task_with_id_and_status, task_with_status};

mod agent_settings;
mod auto_resume;
mod dirty_repo;
mod notifications;
mod phase_settings_matrix;
mod phase_settings_wiring;
mod queue_lock;
mod run_loop_fail_fast;
mod stop_signal;
