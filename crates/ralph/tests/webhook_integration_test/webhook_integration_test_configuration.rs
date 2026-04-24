//! Configuration guard coverage for webhook integration tests.
//!
//! Purpose:
//! - Configuration guard coverage for webhook integration tests.
//!
//! Responsibilities:
//! - Verify disabled or incomplete configs do not attempt delivery.
//! - Keep noop-configuration assertions separate from delivery/payload scenarios.
//!
//! Non-scope:
//! - Retry and backpressure behavior when delivery is enabled.
//! - Payload-shape assertions for successful requests.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Disabled/no-URL scenarios should remain non-panicking and non-delivering.

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
use std::time::Duration;

use ralph::webhook;

#[test]
#[serial]
fn webhook_disabled_does_not_send() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let request_count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&request_count);
    let expected_task_id = unique_test_id("TEST-DISABLED");
    let expected_task_id_clone = expected_task_id.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            if parse_http_json_body(&request)
                .and_then(|json| {
                    json.get("task_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .is_some_and(|task_id| task_id == expected_task_id_clone)
            {
                count_clone.fetch_add(1, Ordering::SeqCst);
            }
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    let mut config = base_webhook_config(port);
    config.enabled = Some(false);
    config.retry_backoff_ms = Some(100);

    webhook::notify_task_created(&expected_task_id, "Test", &config, "2024-01-01T00:00:00Z");

    let saw_request = test_support::wait_until(
        Duration::from_millis(500),
        Duration::from_millis(50),
        || request_count.load(Ordering::SeqCst) > 0,
    );
    assert!(!saw_request, "expected no request when disabled");
    assert_eq!(request_count.load(Ordering::SeqCst), 0);
}

#[test]
#[serial]
fn webhook_no_url_does_not_send() {
    ensure_test_worker_initialized();

    let mut config = base_webhook_config(65535);
    config.url = None;
    config.retry_backoff_ms = Some(100);

    webhook::notify_task_created("TEST-0001", "Test", &config, "2024-01-01T00:00:00Z");
}
