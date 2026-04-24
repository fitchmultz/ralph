//! Event-subscription filtering coverage for webhook integration tests.
//!
//! Purpose:
//! - Event-subscription filtering coverage for webhook integration tests.
//!
//! Responsibilities:
//! - Verify event filtering suppresses unsubscribed task and loop events.
//! - Keep request-matching logic focused on subscription behavior.
//!
//! Non-scope:
//! - Retry or backpressure delivery semantics.
//! - Detailed payload-shape assertions for delivered events.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Tests use unique task IDs and repo roots to distinguish matching requests from worker noise.

use super::webhook_integration_test_support::{
    base_webhook_config, ensure_test_worker_initialized, parse_http_json_body, portable_repo_root,
    read_http_request_with_body, unique_test_id,
};
use crate::test_support;
use serial_test::serial;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ralph::contracts::WebhookEventSubscription;
use ralph::webhook;

#[test]
#[serial]
fn webhook_respects_event_filtering() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let request_count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&request_count);
    let filtered_task_id = unique_test_id("TEST-FILTERED-OUT");
    let expected_task_id = unique_test_id("TEST-ALLOWED");
    let filtered_task_id_clone = filtered_task_id.clone();
    let expected_task_id_clone = expected_task_id.clone();
    let filtered_out_count = Arc::new(AtomicUsize::new(0));
    let filtered_out_count_clone = Arc::clone(&filtered_out_count);

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            if let Some(task_id) = parse_http_json_body(&request).and_then(|json| {
                json.get("task_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            }) {
                if task_id == expected_task_id_clone {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                } else if task_id == filtered_task_id_clone {
                    filtered_out_count_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    let mut config = base_webhook_config(port);
    config.events = Some(vec![WebhookEventSubscription::TaskCompleted]);

    webhook::notify_task_created(&filtered_task_id, "Test", &config, "2024-01-01T00:00:00Z");
    webhook::notify_task_completed(&expected_task_id, "Test", &config, "2024-01-01T00:00:00Z");

    assert!(
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            request_count.load(Ordering::SeqCst) == 1
        }),
        "expected exactly one delivered webhook"
    );

    assert_eq!(request_count.load(Ordering::SeqCst), 1);
    assert_eq!(filtered_out_count.load(Ordering::SeqCst), 0);
}

#[test]
#[serial]
fn webhook_loop_events_are_opt_in_by_default() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_clone = Arc::clone(&requests);

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            requests_clone.lock().unwrap().push(request);
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    let config = base_webhook_config(port);
    let expected_repo_root = portable_repo_root("loop-events-opt-in");

    webhook::notify_loop_started(
        &config,
        "2024-01-01T00:00:00Z",
        webhook::WebhookContext {
            repo_root: Some(expected_repo_root.clone()),
            ..Default::default()
        },
    );

    let saw_matching_loop_event = test_support::wait_until(
        Duration::from_millis(800),
        Duration::from_millis(25),
        || {
            requests.lock().unwrap().iter().any(|request| {
                parse_http_json_body(request).is_some_and(|json| {
                    json.get("event")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|event| event == "loop_started")
                        && json
                            .get("repo_root")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|repo_root| repo_root == expected_repo_root)
                })
            })
        },
    );

    assert!(
        !saw_matching_loop_event,
        "Expected no matching loop_started webhook request when events=None"
    );
}
