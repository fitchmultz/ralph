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

#[derive(Debug)]
struct DispatcherState {
    mode: RuntimeMode,
    dispatcher: Option<Arc<WebhookDispatcher>>,
    disabled_reason: Option<String>,
}

impl Default for DispatcherState {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::Standard,
            dispatcher: None,
            disabled_reason: None,
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
    with_dispatcher_state_write(|state| {
        if state.disabled_reason.is_some() {
            log::debug!("Webhooks disabled for this run after dispatcher startup failure");
            return None;
        }

        let settings = DispatcherSettings::for_mode(config, &state.mode);
        let needs_rebuild = state
            .dispatcher
            .as_ref()
            .is_none_or(|dispatcher| dispatcher.settings != settings);

        if needs_rebuild {
            match build_dispatcher(settings) {
                Ok(dispatcher) => state.dispatcher = Some(dispatcher),
                Err(err) => {
                    let reason = format!("{err:#}");
                    state.dispatcher = None;
                    state.disabled_reason = Some(reason.clone());
                    diagnostics::set_queue_capacity(0);
                    log::warn!(
                        "Webhook delivery disabled for this run: failed to start dispatcher runtime: {reason}"
                    );
                    return None;
                }
            }
        }

        state.dispatcher.as_ref().cloned()
    })
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
    with_dispatcher_state_write(|state| {
        state.mode = RuntimeMode::Standard;
        state.dispatcher = None;
        state.disabled_reason = None;
    });
    crate::webhook::worker::delivery::install_test_transport_for_tests(None);
}
