//! Payload-shape coverage for delivered webhook events.
//!
//! Purpose:
//! - Payload-shape coverage for delivered webhook events.
//!
//! Responsibilities:
//! - Validate signed requests and structured loop/phase payload fields.
//! - Keep payload assertions isolated from delivery and filtering mechanics.
//!
//! Non-scope:
//! - Queue backpressure or disabled-configuration behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Each test waits for a matching request before asserting on JSON fields or headers.

use super::webhook_integration_test_support::{
    base_webhook_config, ensure_test_worker_initialized, parse_http_json_body, portable_repo_root,
    read_http_request_with_body, unique_test_id,
};
use crate::test_support;
use serial_test::serial;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ralph::contracts::WebhookEventSubscription;
use ralph::webhook;

#[test]
#[serial]
fn webhook_includes_signature_header() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_task_id = unique_test_id("TEST-SIGNATURE");

    let received_sig = Arc::new(Mutex::new(None));
    let sig_clone = Arc::clone(&received_sig);
    let expected_task_id_clone = expected_task_id.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let raw = read_http_request_with_body(&mut stream);
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");

            let Some(body_json) = parse_http_json_body(&raw) else {
                continue;
            };
            if body_json.get("task_id").and_then(serde_json::Value::as_str)
                != Some(expected_task_id_clone.as_str())
            {
                continue;
            }

            let request = String::from_utf8_lossy(&raw);
            for line in request.lines() {
                let line = line.trim_end_matches('\r');
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                if key.trim().eq_ignore_ascii_case("x-ralph-signature") {
                    *sig_clone.lock().unwrap() = Some(value.trim().to_string());
                    break;
                }
            }
            break;
        }
    });

    let mut config = base_webhook_config(port);
    config.secret = Some("test-secret".to_string());
    config.retry_backoff_ms = Some(100);

    webhook::notify_task_created(&expected_task_id, "Test", &config, "2024-01-01T00:00:00Z");

    let sig = test_support::wait_for_mutex_value(
        &received_sig,
        Duration::from_secs(10),
        Duration::from_millis(50),
    );
    assert!(sig.is_some(), "X-Ralph-Signature header should be present");
    let sig_str = sig.expect("signature must be present").to_lowercase();
    assert!(
        sig_str.contains("sha256="),
        "Signature should contain sha256= prefix: {sig_str}"
    );
}

#[test]
#[serial]
fn webhook_loop_event_payload_shape() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_request = Arc::new(Mutex::new(None));
    let request_clone = Arc::clone(&received_request);
    let expected_repo_root = portable_repo_root("loop-event-payload-shape");
    let expected_repo_root_clone = expected_repo_root.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
            let matches_expected = parse_http_json_body(&request).is_some_and(|json| {
                json.get("event")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|event| event == "loop_started")
                    && json
                        .get("repo_root")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|repo_root| repo_root == expected_repo_root_clone)
            });
            if matches_expected {
                *request_clone.lock().unwrap() = Some(request);
                break;
            }
        }
    });

    let mut config = base_webhook_config(port);
    config.events = Some(vec![WebhookEventSubscription::LoopStarted]);

    webhook::notify_loop_started(
        &config,
        "2024-01-01T00:00:00Z",
        webhook::WebhookContext {
            repo_root: Some(expected_repo_root.clone()),
            branch: Some("main".to_string()),
            ..Default::default()
        },
    );

    let request = test_support::wait_for_mutex_value(
        &received_request,
        Duration::from_secs(10),
        Duration::from_millis(50),
    )
    .expect("expected loop_started request bytes");

    let json = parse_http_json_body(&request).expect("expected loop_started request JSON");
    assert_eq!(json["event"], "loop_started");
    assert_eq!(json["repo_root"], expected_repo_root);
    assert_eq!(json["branch"], "main");
    assert!(json.get("task_id").is_none());
    assert!(json.get("task_title").is_none());
}

#[test]
#[serial]
fn webhook_phase_event_includes_context_fields() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_request = Arc::new(Mutex::new(None));
    let request_clone = Arc::clone(&received_request);
    let expected_task_id = unique_test_id("TEST-PHASE");
    let expected_task_id_clone = expected_task_id.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\n\r\n");
            let matches_expected_task = parse_http_json_body(&request).is_some_and(|json| {
                json.get("task_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|task_id| task_id == expected_task_id_clone)
            });
            if matches_expected_task {
                *request_clone.lock().unwrap() = Some(request);
                break;
            }
        }
    });

    let mut config = base_webhook_config(port);
    config.events = Some(vec![WebhookEventSubscription::PhaseCompleted]);

    webhook::notify_phase_completed(
        &expected_task_id,
        "Test title",
        &config,
        "2024-01-01T00:00:00Z",
        webhook::WebhookContext {
            runner: Some("codex".to_string()),
            model: Some("gpt-5.3-codex".to_string()),
            phase: Some(3),
            phase_count: Some(3),
            duration_ms: Some(42),
            ci_gate: Some("passed".to_string()),
            ..Default::default()
        },
    );

    let request = test_support::wait_for_mutex_value(
        &received_request,
        Duration::from_secs(10),
        Duration::from_millis(50),
    )
    .expect("expected phase_completed request bytes");

    let json = parse_http_json_body(&request).expect("expected phase_completed request JSON");
    assert_eq!(json["event"], "phase_completed");
    assert_eq!(json["task_id"], expected_task_id);
    assert_eq!(json["task_title"], "Test title");
    assert_eq!(json["runner"], "codex");
    assert_eq!(json["model"], "gpt-5.3-codex");
    assert_eq!(json["phase"], 3);
    assert_eq!(json["phase_count"], 3);
    assert_eq!(json["duration_ms"], 42);
    assert_eq!(json["ci_gate"], "passed");
}
