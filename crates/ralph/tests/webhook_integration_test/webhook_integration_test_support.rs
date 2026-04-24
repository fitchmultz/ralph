//! Shared helpers for webhook integration scenarios.
//!
//! Purpose:
//! - Shared helpers for webhook integration scenarios.
//!
//! Responsibilities:
//! - Parse captured HTTP requests and bootstrap the process-global webhook worker once.
//! - Build canonical local webhook configs for scenario modules.
//! - Provide stable task/repo identifiers for matching requests in concurrent test environments.
//!
//! Non-scope:
//! - Scenario assertions for retry, filtering, or payload shape.
//! - Global integration-test helpers already covered by `crate::test_support`.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Matching logic should key on unique task IDs or repo roots to ignore stale worker traffic.
//! - Request readers tolerate partial socket reads until the declared content length is satisfied.

use crate::test_support;
use ralph::contracts::{WebhookConfig, WebhookQueuePolicy};
use ralph::webhook;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Once};
use std::thread;
use std::time::Duration;

pub(super) fn parse_http_json_body(request_bytes: &[u8]) -> Option<serde_json::Value> {
    let request = String::from_utf8_lossy(request_bytes);
    let (_headers, body) = request.split_once("\r\n\r\n")?;
    serde_json::from_str(body).ok()
}

pub(super) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(super) fn parse_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("content-length")
        {
            return value.trim().parse::<usize>().ok();
        }
    }
    None
}

pub(super) fn unique_test_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    )
}

pub(super) fn portable_repo_root(label: &str) -> String {
    test_support::portable_abs_path(format!("webhook/{label}"))
        .display()
        .to_string()
}

pub(super) fn base_webhook_config(port: u16) -> WebhookConfig {
    WebhookConfig {
        enabled: Some(true),
        url: Some(format!("http://127.0.0.1:{port}/webhook")),
        allow_insecure_http: Some(true),
        allow_private_targets: Some(true),
        secret: None,
        events: None,
        timeout_secs: Some(5),
        retry_count: Some(0),
        retry_backoff_ms: Some(10),
        queue_capacity: Some(10),
        queue_policy: Some(WebhookQueuePolicy::DropNew),
        ..Default::default()
    }
}

pub(super) fn ensure_test_worker_initialized() {
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

        let mut config = base_webhook_config(port);
        config.timeout_secs = Some(1);
        config.retry_count = Some(1);
        config.retry_backoff_ms = Some(1);
        config.queue_capacity = Some(1000);

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

pub(super) fn read_http_request_with_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));

    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let start = std::time::Instant::now();

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
