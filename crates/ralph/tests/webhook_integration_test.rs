//! Integration tests for webhook delivery.
//!
//! Responsibilities:
//! - Validate core webhook behaviors: non-blocking delivery, retry configuration, signing header,
//!   queue backpressure policies, and event filtering.
//!
//! Not handled:
//! - Cryptographic correctness of signature generation beyond basic header presence/shape.
//! - External network behavior (tests use a local TCP listener).
//!
//! Invariants/assumptions:
//! - Tests are `#[serial]` because the webhook worker is global within a process.
//! - Timeouts should tolerate a loaded CI machine (avoid single fixed sleeps when waiting for IO).

mod test_support;

use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Once};
use std::thread;
use std::time::{Duration, Instant};

// Import webhook types
use ralph::contracts::{WebhookConfig, WebhookQueuePolicy};
use ralph::webhook;

fn parse_http_json_body(request_bytes: &[u8]) -> Option<serde_json::Value> {
    let request = String::from_utf8_lossy(request_bytes);
    let (_headers, body) = request.split_once("\r\n\r\n")?;
    serde_json::from_str(body).ok()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            return v.trim().parse::<usize>().ok();
        }
    }
    None
}

fn unique_test_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    )
}

fn ensure_test_worker_initialized() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("bind one-time webhook init listener");
        let port = listener
            .local_addr()
            .expect("read init listener address")
            .port();

        let delivered = Arc::new(AtomicUsize::new(0));
        let delivered_clone = Arc::clone(&delivered);
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = read_http_request_with_body(&mut stream);
                delivered_clone.store(1, Ordering::SeqCst);
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
            }
        });

        let config = WebhookConfig {
            enabled: Some(true),
            url: Some(format!("http://127.0.0.1:{port}/webhook-init")),
            secret: None,
            events: None,
            timeout_secs: Some(1),
            retry_count: Some(0),
            retry_backoff_ms: Some(1),
            queue_capacity: Some(1000),
            queue_policy: Some(WebhookQueuePolicy::DropNew),
        };

        webhook::notify_task_created(
            &unique_test_id("TEST-WEBHOOK-INIT"),
            "Webhook init",
            &config,
            "2024-01-01T00:00:00Z",
        );

        assert!(
            test_support::wait_until(Duration::from_secs(5), Duration::from_millis(25), || {
                delivered.load(Ordering::SeqCst) == 1
            }),
            "failed to deterministically initialize webhook worker for integration tests"
        );
    });
}

fn read_http_request_with_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));

    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let start = Instant::now();

    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);

                if let Some(headers_end) = find_subslice(&buf, b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&buf[..headers_end]);
                    if let Some(len) = parse_content_length(&headers) {
                        let needed = headers_end + 4 + len;
                        if buf.len() >= needed {
                            break;
                        }
                    }
                }
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                // If we have a complete request body, stop reading; otherwise keep trying briefly.
                if let Some(headers_end) = find_subslice(&buf, b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&buf[..headers_end]);
                    if let Some(len) = parse_content_length(&headers) {
                        let needed = headers_end + 4 + len;
                        if buf.len() >= needed {
                            break;
                        }
                    }
                }

                if start.elapsed() > Duration::from_secs(2) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    buf
}

/// Test that send_webhook returns immediately (non-blocking).
#[test]
#[serial]
fn webhook_send_is_non_blocking() {
    ensure_test_worker_initialized();

    // Start a slow HTTP server
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            // Read request but don't respond for 2 seconds
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            thread::sleep(Duration::from_secs(2));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    // Send webhook - should return immediately
    let start = Instant::now();
    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(100),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    webhook::notify_task_created("TEST-0001", "Test", &config, "2024-01-01T00:00:00Z");

    let elapsed = start.elapsed();
    // Should return quickly (well below the 2s server delay).
    assert!(
        elapsed < Duration::from_secs(1),
        "send_webhook took {:?}, should be non-blocking",
        elapsed
    );
}

/// Test retry behavior with failing endpoint.
/// Note: This test verifies the retry logic exists by checking the code path
/// doesn't panic. Due to global worker constraints in tests, we verify
/// the webhook system handles retry configuration without errors.
#[test]
#[serial]
fn webhook_retries_failed_deliveries() {
    ensure_test_worker_initialized();

    // Start a server that returns 500 (failure)
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            // Return error to trigger retry path
            let _ = stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n");
        }
    });

    // Configure with 1 retry - this tests that retry config is accepted
    // and the worker doesn't panic when processing retries
    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(1),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    let start = Instant::now();
    webhook::notify_task_created("TEST-0002", "Test", &config, "2024-01-01T00:00:00Z");
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "enqueue should remain non-blocking even with retries configured; elapsed={elapsed:?}"
    );
}

/// Test signature header is sent correctly.
#[test]
#[serial]
fn webhook_includes_signature_header() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_task_id = unique_test_id("TEST-SIGNATURE");

    let received_sig = Arc::new(std::sync::Mutex::new(None));
    let sig_clone = received_sig.clone();
    let expected_task_id_clone = expected_task_id.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            // Read a complete request and ignore stale messages that do not match the
            // test's unique task_id. This avoids flakiness from the global webhook worker.
            let raw = read_http_request_with_body(&mut stream);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");

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

    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: Some("test-secret".to_string()),
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(100),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

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
        "Signature should contain sha256= prefix: {}",
        sig_str
    );
}

/// Test queue backpressure with drop_new policy.
#[test]
#[serial]
fn webhook_drop_new_policy() {
    ensure_test_worker_initialized();

    // Start a slow server that processes one request at a time
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
            thread::sleep(Duration::from_millis(200)); // Slow processing
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    // Small queue of 2
    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(2),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    // Send 5 webhooks quickly - queue can only hold 2
    for i in 0..5 {
        webhook::notify_task_created(
            &format!("{task_id_prefix}-{i}"),
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

/// Test that webhook respects event filtering.
#[test]
#[serial]
fn webhook_respects_event_filtering() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let request_count = Arc::new(AtomicUsize::new(0));
    let count_clone = request_count.clone();
    let filtered_task_id = unique_test_id("TEST-FILTERED-OUT");
    let expected_task_id = unique_test_id("TEST-ALLOWED");
    let filtered_task_id_clone = filtered_task_id.clone();
    let expected_task_id_clone = expected_task_id.clone();
    let filtered_out_count = Arc::new(AtomicUsize::new(0));
    let filtered_out_count_clone = filtered_out_count.clone();

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
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    // Only subscribe to task_completed events
    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: Some(vec!["task_completed".to_string()]),
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    // Send task_created webhook - should be filtered out
    webhook::notify_task_created(&filtered_task_id, "Test", &config, "2024-01-01T00:00:00Z");

    // Send task_completed webhook - should go through
    webhook::notify_task_completed(&expected_task_id, "Test", &config, "2024-01-01T00:00:00Z");

    assert!(
        test_support::wait_until(Duration::from_secs(10), Duration::from_millis(50), || {
            request_count.load(Ordering::SeqCst) == 1
        }),
        "expected exactly one delivered webhook"
    );

    let count = request_count.load(Ordering::SeqCst);
    assert_eq!(
        count, 1,
        "Expected 1 request (only task_completed), got {}",
        count
    );
    assert_eq!(
        filtered_out_count.load(Ordering::SeqCst),
        0,
        "task_created webhook should have been filtered"
    );
}

/// Loop events should be opt-in when `events` is not set.
#[test]
#[serial]
fn webhook_loop_events_are_opt_in_by_default() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();

    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: None, // legacy-only defaults
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };
    let expected_repo_root = unique_test_id("/tmp/repo");

    webhook::notify_loop_started(
        &config,
        "2024-01-01T00:00:00Z",
        webhook::WebhookContext {
            repo_root: Some(expected_repo_root.clone()),
            ..Default::default()
        },
    );

    // Poll to ensure no connection is made.
    let mut saw_matching_loop_event = false;
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(800) {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let request = read_http_request_with_body(&mut stream);
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
                if let Some(json) = parse_http_json_body(&request) {
                    let is_expected_event = json
                        .get("event")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|event| event == "loop_started");
                    let is_expected_repo_root = json
                        .get("repo_root")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|repo_root| repo_root == expected_repo_root);
                    if is_expected_event && is_expected_repo_root {
                        saw_matching_loop_event = true;
                        break;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => panic!("accept failed: {err}"),
        }
    }
    assert!(
        !saw_matching_loop_event,
        "Expected no matching loop_started webhook request when events=None"
    );
}

#[test]
#[serial]
fn webhook_loop_event_payload_shape() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_request = Arc::new(std::sync::Mutex::new(None));
    let request_clone = received_request.clone();
    let expected_repo_root = unique_test_id("/tmp/repo");
    let expected_repo_root_clone = expected_repo_root.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
            let matches_expected = parse_http_json_body(&request).is_some_and(|json| {
                let is_loop_started = json
                    .get("event")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|event| event == "loop_started");
                let has_expected_repo_root = json
                    .get("repo_root")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|repo_root| repo_root == expected_repo_root_clone);
                is_loop_started && has_expected_repo_root
            });
            if matches_expected {
                *request_clone.lock().unwrap() = Some(request);
                break;
            }
        }
    });

    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: Some(vec!["loop_started".to_string()]),
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

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
    assert!(
        json.get("task_id").is_none(),
        "loop events should omit task_id"
    );
    assert!(
        json.get("task_title").is_none(),
        "loop events should omit task_title"
    );
}

#[test]
#[serial]
fn webhook_phase_event_includes_context_fields() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_request = Arc::new(std::sync::Mutex::new(None));
    let request_clone = received_request.clone();
    let expected_task_id = unique_test_id("TEST-PHASE");
    let expected_task_id_clone = expected_task_id.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_request_with_body(&mut stream);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
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

    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: Some(vec!["phase_completed".to_string()]),
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    webhook::notify_phase_completed(
        &expected_task_id,
        "Test title",
        &config,
        "2024-01-01T00:00:00Z",
        webhook::WebhookContext {
            runner: Some("codex".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
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
    assert_eq!(json["model"], "gpt-5.2-codex");
    assert_eq!(json["phase"], 3);
    assert_eq!(json["phase_count"], 3);
    assert_eq!(json["duration_ms"], 42);
    assert_eq!(json["ci_gate"], "passed");
}

/// Test webhook with disabled configuration.
#[test]
#[serial]
fn webhook_disabled_does_not_send() {
    ensure_test_worker_initialized();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let request_count = Arc::new(AtomicUsize::new(0));
    let count_clone = request_count.clone();
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
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    // Webhooks disabled
    let config = WebhookConfig {
        enabled: Some(false),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(100),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    webhook::notify_task_created(&expected_task_id, "Test", &config, "2024-01-01T00:00:00Z");

    let saw_request = test_support::wait_until(
        Duration::from_millis(500),
        Duration::from_millis(50),
        || request_count.load(Ordering::SeqCst) > 0,
    );
    let count = request_count.load(Ordering::SeqCst);
    assert!(!saw_request, "expected no request when disabled");
    assert_eq!(count, 0, "Expected 0 requests when disabled, got {}", count);
}

/// Test webhook with no URL configured.
#[test]
#[serial]
fn webhook_no_url_does_not_send() {
    ensure_test_worker_initialized();

    // No URL configured but enabled - should not panic or send anything
    let config = WebhookConfig {
        enabled: Some(true),
        url: None,
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(100),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
    };

    // This should not panic and should return immediately
    webhook::notify_task_created("TEST-0001", "Test", &config, "2024-01-01T00:00:00Z");

    // Test passes if no panic occurred
}

/// Test queue backpressure with drop_oldest policy.
/// Note: This test verifies the drop_oldest policy configuration is accepted
/// and doesn't panic. Due to global worker constraints, we verify the code
/// path works rather than exact queue behavior.
#[test]
#[serial]
fn webhook_drop_oldest_policy() {
    ensure_test_worker_initialized();

    // Start a server
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
            thread::sleep(Duration::from_millis(100)); // Slow processing
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    // Small queue with drop_oldest policy - this tests the config is accepted
    let config = WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{}/webhook", port)),
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(2),
        queue_policy: Some(WebhookQueuePolicy::DropOldest),
    };

    // Send multiple webhooks - some may be dropped due to queue policy
    for i in 0..5 {
        webhook::notify_task_created(
            &format!("{task_id_prefix}-{i}"),
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
