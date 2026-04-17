//! Purpose: Verify public redaction expansion behavior through the crate API.
//!
//! Responsibilities:
//! - Cover redaction of AWS keys, SSH private keys, sensitive hex tokens, and
//!   multiline key-value secrets.
//! - Preserve public API behavior that complements lower-level unit tests.
//!
//! Scope:
//! - Integration-style redaction checks only.
//!
//! Usage:
//! - Runs with the Ralph integration test suite.
//!
//! Invariants/Assumptions:
//! - Legitimate standalone hashes stay readable unless context marks them as
//!   secret-shaped.

use ralph::redaction::redact_text;

#[test]
fn test_redact_aws_keys() {
    let input =
        "My key is AKIAIOSFODNN7EXAMPLE and secret is wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let output = redact_text(input);
    assert!(!output.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(!output.contains("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"));
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_ssh_private_keys() {
    let input = r#"
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABlwAAAAdzc2gtcn
NhAAAAAwEAAQAAAYEAq9u+vKzYn8y...
-----END OPENSSH PRIVATE KEY-----
"#;
    let output = redact_text(input);
    assert!(
        !output.contains("b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABlwAAAAdzc2gtcn")
    );
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_high_risk_hex_tokens() {
    let digest = "5567702f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f";
    let contextual_secret = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let long_secret = concat!(
        "abcdef0123456789abcdef0123456789",
        "abcdef0123456789abcdef0123456789",
        "abcdef0123456789abcdef0123456789"
    );
    let input =
        format!("digest {digest}; webhook signature {contextual_secret}; raw {long_secret}");
    let output = redact_text(&input);

    assert!(output.contains(digest));
    assert!(!output.contains(contextual_secret));
    assert!(!output.contains(long_secret));
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_multiline_key_value() {
    let input = "private_key: |\n  -----BEGIN RSA PRIVATE KEY-----\n  MIIEpAIBAAKCAQEA75...\n  -----END RSA PRIVATE KEY-----";
    let output = redact_text(input);
    assert!(!output.contains("MIIEpAIBAAKCAQEA75"));
    assert!(output.contains("private_key: [REDACTED]"));
}
