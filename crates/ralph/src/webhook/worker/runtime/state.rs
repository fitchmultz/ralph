//! Global dispatcher state, reload/rebuild decisions, and config entry points.
//!
//! Purpose:
//! - Global dispatcher state, reload/rebuild decisions, and config entry points.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::contracts::WebhookConfig;
use std::sync::{Arc, OnceLock, RwLock};

use super::types::{DispatcherSettings, RuntimeMode, WebhookDispatcher};
use crate::webhook::diagnostics;

#[derive(Debug, Default)]
struct DispatcherStartupState {
    last_startup_error: Option<String>,
    consecutive_startup_failures: u32,
}

#[derive(Debug)]
struct DispatcherState {
    mode: RuntimeMode,
    dispatcher: Option<Arc<WebhookDispatcher>>,
    startup: DispatcherStartupState,
}

impl Default for DispatcherState {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::Standard,
            dispatcher: None,
            startup: DispatcherStartupState::default(),
        }
    }
}

static DISPATCHER_STATE: OnceLock<RwLock<DispatcherState>> = OnceLock::new();

fn dispatcher_state() -> &'static RwLock<DispatcherState> {
    DISPATCHER_STATE.get_or_init(|| RwLock::new(DispatcherState::default()))
}

fn with_dispatcher_state_write<T>(mut f: impl FnMut(&mut DispatcherState) -> T) -> T {
    match dispatcher_state().write() {
        Ok(mut guard) => f(&mut guard),
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            f(&mut guard)
        }
    }
}

pub(crate) fn dispatcher_for_config(config: &WebhookConfig) -> Option<Arc<WebhookDispatcher>> {
    dispatcher_for_config_with_factory(config, WebhookDispatcher::new)
}

#[cfg(test)]
pub(crate) fn dispatcher_for_config_with_spawner(
    config: &WebhookConfig,
    spawner: &impl super::types::ThreadSpawner,
) -> Option<Arc<WebhookDispatcher>> {
    dispatcher_for_config_with_factory(config, |settings| {
        WebhookDispatcher::new_with_spawner(settings, spawner)
    })
}

fn dispatcher_for_config_with_factory(
    config: &WebhookConfig,
    mut build_dispatcher: impl FnMut(DispatcherSettings) -> anyhow::Result<Arc<WebhookDispatcher>>,
) -> Option<Arc<WebhookDispatcher>> {
    let mut old_dispatcher = None;
    let dispatcher = with_dispatcher_state_write(|state| {
        let settings = DispatcherSettings::for_mode(config, &state.mode);
        let needs_rebuild = state
            .dispatcher
            .as_ref()
            .is_none_or(|dispatcher| dispatcher.settings != settings);

        if needs_rebuild {
            match build_dispatcher(settings) {
                Ok(dispatcher) => {
                    old_dispatcher = state.dispatcher.replace(dispatcher);
                    state.startup = DispatcherStartupState::default();
                }
                Err(err) => {
                    let reason = format!("{err:#}");
                    let should_log = state.startup.last_startup_error.as_deref() != Some(&reason);
                    state.startup.last_startup_error = Some(reason.clone());
                    state.startup.consecutive_startup_failures =
                        state.startup.consecutive_startup_failures.saturating_add(1);
                    diagnostics::set_queue_capacity(0);
                    if should_log {
                        if state.dispatcher.is_some() {
                            log::warn!(
                                "Webhook dispatcher rebuild failed; keeping the previous runtime active until a later rebuild succeeds: {reason}"
                            );
                        } else {
                            log::warn!(
                                "Webhook dispatcher startup failed; delivery remains unavailable until a later rebuild succeeds: {reason}"
                            );
                        }
                    }
                    return state.dispatcher.as_ref().cloned();
                }
            }
        }

        state.dispatcher.as_ref().cloned()
    });
    drop(old_dispatcher);
    dispatcher
}

/// Initialize the webhook dispatcher with capacity scaled for parallel execution.
pub fn init_worker_for_parallel(config: &WebhookConfig, worker_count: u8) {
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Parallel { worker_count };
    });
    let _ = dispatcher_for_config(config);
}

#[cfg(test)]
pub(crate) fn current_dispatcher_settings_for_tests(
    config: &WebhookConfig,
) -> Option<(usize, usize)> {
    dispatcher_for_config(config).map(|dispatcher| {
        (
            dispatcher.settings.queue_capacity,
            dispatcher.settings.worker_count,
        )
    })
}

#[cfg(test)]
pub(crate) fn reset_dispatcher_for_tests() {
    let mut old_dispatcher = None;
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Standard;
        old_dispatcher = state.dispatcher.take();
        state.startup = DispatcherStartupState::default();
    });
    drop(old_dispatcher);
    crate::webhook::worker::delivery::install_test_transport_for_tests(None);
}
