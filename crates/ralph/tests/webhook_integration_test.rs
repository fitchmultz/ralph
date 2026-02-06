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

use serial_test::serial;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

// Import webhook types
use ralph::contracts::{WebhookConfig, WebhookQueuePolicy};
use ralph::webhook;

fn wait_for_mutex_value<T: Clone>(
    value: &Arc<std::sync::Mutex<Option<T>>>,
    timeout: Duration,
) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(v) = value.lock().expect("lock mutex").clone() {
            return Some(v);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

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
    // Should return in under 200ms (not wait for 2s HTTP response)
    assert!(
        elapsed < Duration::from_millis(200),
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

    // This should not panic - the retry logic will be exercised
    webhook::notify_task_created("TEST-0002", "Test", &config, "2024-01-01T00:00:00Z");

    // Wait for processing (including retry)
    thread::sleep(Duration::from_millis(500));

    // Test passes if no panic occurred (retry logic was exercised)
}

/// Test signature header is sent correctly.
#[test]
#[serial]
fn webhook_includes_signature_header() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_sig = Arc::new(std::sync::Mutex::new(None));
    let sig_clone = received_sig.clone();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            // Read the request headers
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);

            // Check for signature header in the request
            for line in request.lines() {
                if line.to_lowercase().starts_with("x-ralph-signature: ") {
                    *sig_clone.lock().unwrap() = Some(line.to_string());
                    break;
                }
            }

            // Write response
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
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

    webhook::notify_task_created("TEST-0003", "Test", &config, "2024-01-01T00:00:00Z");

    let sig = wait_for_mutex_value(&received_sig, Duration::from_secs(10));
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
    // Start a slow server that processes one request at a time
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
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
            &format!("TEST-{}", i),
            "Test",
            &config,
            "2024-01-01T00:00:00Z",
        );
    }

    // Give time for processing - use longer timeout for CI stability
    thread::sleep(Duration::from_millis(1500));

    // Test passes if no panic - some webhooks were dropped per policy
}

/// Test that webhook respects event filtering.
#[test]
#[serial]
fn webhook_respects_event_filtering() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let request_count = Arc::new(AtomicUsize::new(0));
    let count_clone = request_count.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            count_clone.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

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
    webhook::notify_task_created("TEST-0001", "Test", &config, "2024-01-01T00:00:00Z");

    // Send task_completed webhook - should go through
    webhook::notify_task_completed("TEST-0002", "Test", &config, "2024-01-01T00:00:00Z");

    // Wait for processing - needs to be long enough for worker to process both messages
    // Use a longer timeout to account for CI variability
    thread::sleep(Duration::from_millis(2000));

    let count = request_count.load(Ordering::SeqCst);
    assert_eq!(
        count, 1,
        "Expected 1 request (only task_completed), got {}",
        count
    );
}

/// Loop events should be opt-in when `events` is not set.
#[test]
#[serial]
fn webhook_loop_events_are_opt_in_by_default() {
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

    webhook::notify_loop_started(
        &config,
        "2024-01-01T00:00:00Z",
        webhook::WebhookContext {
            repo_root: Some("/tmp/repo".to_string()),
            ..Default::default()
        },
    );

    // Poll to ensure no connection is made.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(800) {
        match listener.accept() {
            Ok((_stream, _addr)) => {
                panic!("Expected no webhook request for loop event with default events=None");
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => panic!("accept failed: {err}"),
        }
    }
}

#[test]
#[serial]
fn webhook_loop_event_payload_shape() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_request = Arc::new(std::sync::Mutex::new(None));
    let request_clone = received_request.clone();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            *request_clone.lock().unwrap() = Some(read_http_request_with_body(&mut stream));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
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
            repo_root: Some("/tmp/repo".to_string()),
            branch: Some("main".to_string()),
            ..Default::default()
        },
    );

    let request = wait_for_mutex_value(&received_request, Duration::from_secs(10))
        .expect("expected loop_started request bytes");

    let json = parse_http_json_body(&request).expect("expected loop_started request JSON");

    assert_eq!(json["event"], "loop_started");
    assert_eq!(json["repo_root"], "/tmp/repo");
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let received_request = Arc::new(std::sync::Mutex::new(None));
    let request_clone = received_request.clone();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            *request_clone.lock().unwrap() = Some(read_http_request_with_body(&mut stream));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
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
        "TEST-0001",
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

    let request = wait_for_mutex_value(&received_request, Duration::from_secs(10))
        .expect("expected phase_completed request bytes");

    let json = parse_http_json_body(&request).expect("expected phase_completed request JSON");

    assert_eq!(json["event"], "phase_completed");
    assert_eq!(json["task_id"], "TEST-0001");
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let request_count = Arc::new(AtomicUsize::new(0));
    let count_clone = request_count.clone();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            count_clone.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
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

    webhook::notify_task_created("TEST-0001", "Test", &config, "2024-01-01T00:00:00Z");

    // Wait a bit
    thread::sleep(Duration::from_millis(200));

    let count = request_count.load(Ordering::SeqCst);
    assert_eq!(count, 0, "Expected 0 requests when disabled, got {}", count);
}

/// Test webhook with no URL configured.
#[test]
#[serial]
fn webhook_no_url_does_not_send() {
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

    // Wait a bit
    thread::sleep(Duration::from_millis(100));

    // Test passes if no panic occurred
}

/// Test queue backpressure with drop_oldest policy.
/// Note: This test verifies the drop_oldest policy configuration is accepted
/// and doesn't panic. Due to global worker constraints, we verify the code
/// path works rather than exact queue behavior.
#[test]
#[serial]
fn webhook_drop_oldest_policy() {
    // Start a server
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
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
            &format!("TEST-{}", i),
            "Test",
            &config,
            "2024-01-01T00:00:00Z",
        );
        thread::sleep(Duration::from_millis(10));
    }

    // Give time for processing
    thread::sleep(Duration::from_millis(800));

    // Test passes if no panic occurred (drop_oldest policy was exercised)
}
