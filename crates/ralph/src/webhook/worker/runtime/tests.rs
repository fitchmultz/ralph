use super::reset_dispatcher_for_tests;
use super::state::dispatcher_for_config_with_spawner;
use super::types::ThreadSpawner;
use crate::contracts::WebhookConfig;
use serial_test::serial;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct FailingThreadSpawner;

impl ThreadSpawner for FailingThreadSpawner {
    fn spawn(&self, _name: String, _task: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "simulated thread exhaustion",
        ))
    }
}

#[derive(Debug, Default)]
struct SilentThreadSpawner;

impl ThreadSpawner for SilentThreadSpawner {
    fn spawn(&self, _name: String, _task: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct CountingSpawner {
    calls: AtomicUsize,
}

impl ThreadSpawner for CountingSpawner {
    fn spawn(&self, _name: String, _task: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other(
            "dispatcher should stay disabled after startup failure",
        ))
    }
}

#[test]
#[serial]
fn thread_spawn_failure_disables_webhooks_for_run_without_panic() {
    reset_dispatcher_for_tests();
    let config = WebhookConfig::default();

    let dispatcher = dispatcher_for_config_with_spawner(&config, &FailingThreadSpawner);
    assert!(dispatcher.is_none());

    let counting_spawner = CountingSpawner::default();
    let retry = dispatcher_for_config_with_spawner(&config, &counting_spawner);
    assert!(retry.is_none());
    assert_eq!(counting_spawner.calls.load(Ordering::SeqCst), 0);

    reset_dispatcher_for_tests();
}

#[test]
#[serial]
fn startup_handshake_timeout_disables_webhooks_without_panic() {
    reset_dispatcher_for_tests();
    let config = WebhookConfig::default();

    let dispatcher = dispatcher_for_config_with_spawner(&config, &SilentThreadSpawner);
    assert!(dispatcher.is_none());

    reset_dispatcher_for_tests();
}
