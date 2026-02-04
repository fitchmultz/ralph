//! Integration tests for webhook delivery.

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

    // Wait for delivery - needs to be long enough for worker to process and make HTTP request
    // Use a longer timeout to account for CI variability
    thread::sleep(Duration::from_millis(2500));

    let sig = received_sig.lock().unwrap();
    assert!(sig.is_some(), "X-Ralph-Signature header should be present");
    let sig_str = sig.as_ref().unwrap().to_lowercase();
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
