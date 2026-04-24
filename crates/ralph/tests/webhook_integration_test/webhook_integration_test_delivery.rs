//! Delivery and retry behavior coverage for webhook integration tests.
//!
//! Purpose:
//! - Delivery and retry behavior coverage for webhook integration tests.
//!
//! Responsibilities:
//! - Validate non-blocking enqueue semantics and retry delivery attempts.
//! - Cover queue backpressure behavior for `DropNew` and `DropOldest`.
//!
//! Non-scope:
//! - Event filtering or payload-shape assertions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Each scenario matches requests by a unique task ID prefix to avoid stale worker traffic.

use super::webhook_integration_test_support::{
    base_webhook_config, ensure_test_worker_initialized, parse_http_json_body,
    read_http_request_with_body, unique_test_id,
};
use crate::test_support;
use serial_test::serial;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ralph::contracts::WebhookQueuePolicy;
use ralph::webhook;

#[test]
#[serial]
fn webhook_send_is_non_blocking() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let response_gate = Arc::new(test_support::Gate::new_closed());
    let response_gate_clone = Arc::clone(&response_gate);

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let _ = response_gate_clone.wait(Duration::from_secs(2));
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    let start = Instant::now();
    let config = base_webhook_config(port);
    webhook::notify_task_created("TEST-0001", "Test", &config, "2024-01-01T00:00:00Z");

    let elapsed = start.elapsed();
    response_gate.open();
    assert!(
        elapsed < Duration::from_secs(1),
        "send_webhook took {elapsed:?}, should be non-blocking"
    );
}

#[test]
#[serial]
fn webhook_retries_failed_deliveries() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_task_id = unique_test_id("TEST-RETRY");
    let expected_task_id_clone = expected_task_id.clone();
    let attempt_count = Arc::new(AtomicUsize::new(0));
    let attempt_count_clone = Arc::clone(&attempt_count);

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 500 Internal Server Error\r\n\r\n",
            );

            if parse_http_json_body(&request).is_some_and(|json| {
                json.get("task_id").and_then(serde_json::Value::as_str)
                    == Some(expected_task_id_clone.as_str())
            }) {
                attempt_count_clone.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    let mut config = base_webhook_config(port);
    config.retry_count = Some(1);

    let start = Instant::now();
    webhook::notify_task_created(&expected_task_id, "Test", &config, "2024-01-01T00:00:00Z");
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "enqueue should remain non-blocking; elapsed={elapsed:?}"
    );

    let expected_attempts = 2;
    let attempts_observed =
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            attempt_count.load(Ordering::SeqCst) >= expected_attempts
        });

    assert!(
        attempts_observed,
        "expected at least {expected_attempts} delivery attempts for retry_count=1, got {}",
        attempt_count.load(Ordering::SeqCst)
    );
}

#[test]
#[serial]
fn webhook_drop_new_policy() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_clone = Arc::clone(&request_count);
    let task_id_prefix = unique_test_id("TEST-DROP-NEW");
    let task_id_prefix_clone = task_id_prefix.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            if parse_http_json_body(&request)
                .and_then(|json| {
                    json.get("task_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .is_some_and(|task_id| task_id.starts_with(&task_id_prefix_clone))
            {
                request_count_clone.fetch_add(1, Ordering::SeqCst);
            }
            let response_gate = test_support::Gate::new_closed();
            let _ = response_gate.wait(Duration::from_millis(200));
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    let mut config = base_webhook_config(port);
    config.queue_capacity = Some(2);
    config.queue_policy = Some(WebhookQueuePolicy::DropNew);

    for index in 0..5 {
        webhook::notify_task_created(
            &format!("{task_id_prefix}-{index}"),
            "Test",
            &config,
            "2024-01-01T00:00:00Z",
        );
    }

    assert!(
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            request_count.load(Ordering::SeqCst) >= 1
        }),
        "expected at least one delivered webhook with drop_new policy"
    );
}

#[test]
#[serial]
fn webhook_drop_oldest_policy() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_clone = Arc::clone(&request_count);
    let task_id_prefix = unique_test_id("TEST-DROP-OLDEST");
    let task_id_prefix_clone = task_id_prefix.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            if parse_http_json_body(&request)
                .and_then(|json| {
                    json.get("task_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .is_some_and(|task_id| task_id.starts_with(&task_id_prefix_clone))
            {
                request_count_clone.fetch_add(1, Ordering::SeqCst);
            }
            let response_gate = test_support::Gate::new_closed();
            let _ = response_gate.wait(Duration::from_millis(100));
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    let mut config = base_webhook_config(port);
    config.queue_capacity = Some(2);
    config.queue_policy = Some(WebhookQueuePolicy::DropOldest);

    for index in 0..5 {
        webhook::notify_task_created(
            &format!("{task_id_prefix}-{index}"),
            "Test",
            &config,
            "2024-01-01T00:00:00Z",
        );
    }

    assert!(
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            request_count.load(Ordering::SeqCst) >= 1
        }),
        "expected at least one delivered webhook for drop_oldest policy"
    );
}
