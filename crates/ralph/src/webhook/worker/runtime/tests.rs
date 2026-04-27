//! Webhook worker runtime tests.
//!
//! Purpose:
//! - Verify dispatcher startup recovery and deterministic teardown for the webhook runtime.
//!
//! Responsibilities:
//! - Cover recoverable startup failures for thread-spawn and handshake-timeout paths.
//! - Assert dispatcher rebuild/reset joins worker and scheduler threads instead of latching disablement.
//!
//! Not handled here:
//! - HTTP delivery behavior or retry scheduling semantics beyond lifecycle shutdown.
//! - Webhook configuration parsing.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests reset global dispatcher state before and after lifecycle checks.

use super::reset_dispatcher_for_tests;
use super::state::dispatcher_for_config_with_spawner;
use super::types::{ThreadHandle, ThreadSpawner};
use crate::contracts::WebhookConfig;
use serial_test::serial;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct NoopHandle;

impl ThreadHandle for NoopHandle {
    fn join(self: Box<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct FailingThreadSpawner;

impl ThreadSpawner for FailingThreadSpawner {
    fn spawn(
        &self,
        _name: String,
        _task: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn ThreadHandle>> {
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "simulated thread exhaustion",
        ))
    }
}

#[derive(Debug, Default)]
struct SilentThreadSpawner;

impl ThreadSpawner for SilentThreadSpawner {
    fn spawn(
        &self,
        _name: String,
        _task: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn ThreadHandle>> {
        Ok(Box::new(NoopHandle))
    }
}

#[derive(Debug, Default)]
struct TrackingSpawner {
    spawn_calls: AtomicUsize,
    join_calls: Arc<AtomicUsize>,
    exit_calls: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct TrackingHandle {
    handle: std::thread::JoinHandle<()>,
    join_calls: Arc<AtomicUsize>,
}

impl ThreadHandle for TrackingHandle {
    fn join(self: Box<Self>) -> anyhow::Result<()> {
        self.join_calls.fetch_add(1, Ordering::SeqCst);
        self.handle
            .join()
            .map_err(|panic_payload| anyhow::anyhow!("thread panicked: {panic_payload:?}"))
    }
}

impl ThreadSpawner for TrackingSpawner {
    fn spawn(
        &self,
        name: String,
        task: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn ThreadHandle>> {
        self.spawn_calls.fetch_add(1, Ordering::SeqCst);
        let exit_calls = Arc::clone(&self.exit_calls);
        let handle = std::thread::Builder::new().name(name).spawn(move || {
            task();
            exit_calls.fetch_add(1, Ordering::SeqCst);
        })?;
        Ok(Box::new(TrackingHandle {
            handle,
            join_calls: Arc::clone(&self.join_calls),
        }))
    }
}

fn enabled_config() -> WebhookConfig {
    WebhookConfig {
        enabled: Some(true),
        url: Some("https://example.com/ralph-webhook".to_string()),
        ..Default::default()
    }
}

fn queue_config(capacity: u32) -> WebhookConfig {
    WebhookConfig {
        queue_capacity: Some(capacity),
        ..enabled_config()
    }
}

#[test]
#[serial]
fn default_webhook_config_does_not_start_dispatcher() {
    reset_dispatcher_for_tests();

    let tracking = TrackingSpawner::default();
    let dispatcher = dispatcher_for_config_with_spawner(&WebhookConfig::default(), &tracking);

    assert!(dispatcher.is_none());
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 0);

    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn enabled_without_url_does_not_start_dispatcher() {
    reset_dispatcher_for_tests();

    let config = WebhookConfig {
        enabled: Some(true),
        url: None,
        ..Default::default()
    };
    let tracking = TrackingSpawner::default();
    let dispatcher = dispatcher_for_config_with_spawner(&config, &tracking);

    assert!(dispatcher.is_none());
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 0);

    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn inactive_config_tears_down_existing_runtime() {
    reset_dispatcher_for_tests();

    let tracking = TrackingSpawner::default();
    let active = dispatcher_for_config_with_spawner(&enabled_config(), &tracking)
        .expect("active dispatcher");
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 5);
    drop(active);

    let inactive = WebhookConfig {
        enabled: Some(false),
        url: Some("https://example.com/ralph-webhook".to_string()),
        ..Default::default()
    };
    let dispatcher = dispatcher_for_config_with_spawner(&inactive, &tracking);

    assert!(dispatcher.is_none());
    assert_eq!(tracking.join_calls.load(Ordering::SeqCst), 5);
    assert_eq!(tracking.exit_calls.load(Ordering::SeqCst), 5);

    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn thread_spawn_failure_is_recoverable_on_later_attempt() {
    reset_dispatcher_for_tests();

    let config = enabled_config();
    let dispatcher = dispatcher_for_config_with_spawner(&config, &FailingThreadSpawner);
    assert!(dispatcher.is_none());

    let tracking = TrackingSpawner::default();
    let recovered = dispatcher_for_config_with_spawner(&config, &tracking).expect("recovery");
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 5);

    drop(recovered);
    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn startup_handshake_timeout_is_recoverable_on_later_attempt() {
    reset_dispatcher_for_tests();

    let config = enabled_config();
    let dispatcher = dispatcher_for_config_with_spawner(&config, &SilentThreadSpawner);
    assert!(dispatcher.is_none());

    let tracking = TrackingSpawner::default();
    let recovered = dispatcher_for_config_with_spawner(&config, &tracking).expect("recovery");
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 5);

    drop(recovered);
    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn reset_dispatcher_joins_spawned_threads() {
    reset_dispatcher_for_tests();

    let tracking = TrackingSpawner::default();
    let dispatcher =
        dispatcher_for_config_with_spawner(&enabled_config(), &tracking).expect("dispatcher");
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 5);

    drop(dispatcher);
    reset_dispatcher_for_tests();

    assert_eq!(tracking.join_calls.load(Ordering::SeqCst), 5);
    assert_eq!(tracking.exit_calls.load(Ordering::SeqCst), 5);
}

#[test]
#[serial]
fn rebuild_with_changed_settings_replaces_and_joins_old_runtime() {
    reset_dispatcher_for_tests();

    let tracking = TrackingSpawner::default();
    let first = dispatcher_for_config_with_spawner(&queue_config(100), &tracking)
        .expect("first dispatcher");
    assert_eq!(first.settings.queue_capacity, 100);
    drop(first);

    let second = dispatcher_for_config_with_spawner(&queue_config(200), &tracking)
        .expect("second dispatcher");
    assert_eq!(second.settings.queue_capacity, 200);
    assert_eq!(tracking.join_calls.load(Ordering::SeqCst), 5);
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 10);

    drop(second);
    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn rebuild_failure_keeps_existing_runtime_available() {
    reset_dispatcher_for_tests();

    let tracking = TrackingSpawner::default();
    let first = dispatcher_for_config_with_spawner(&queue_config(100), &tracking)
        .expect("first dispatcher");
    assert_eq!(first.settings.queue_capacity, 100);

    let recovered = dispatcher_for_config_with_spawner(&queue_config(200), &FailingThreadSpawner)
        .expect("existing dispatcher should stay available");
    assert_eq!(recovered.settings.queue_capacity, 100);
    assert_eq!(tracking.join_calls.load(Ordering::SeqCst), 0);
    assert_eq!(tracking.spawn_calls.load(Ordering::SeqCst), 5);

    drop(recovered);
    drop(first);
    reset_dispatcher_for_tests();
}
