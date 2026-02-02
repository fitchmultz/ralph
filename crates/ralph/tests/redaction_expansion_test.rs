//! Unit tests for redaction expansions and sensitive pattern masking.

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
fn test_redact_generic_hex_tokens() {
    // 32 chars or more of hex
    let input = "session_id: 5567702f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f";
    let output = redact_text(input);
    assert!(!output.contains("5567702f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f5a6b7a2f"));
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_multiline_key_value() {
    let input = "private_key: |\n  -----BEGIN RSA PRIVATE KEY-----\n  MIIEpAIBAAKCAQEA75...\n  -----END RSA PRIVATE KEY-----";
    let output = redact_text(input);
    assert!(!output.contains("MIIEpAIBAAKCAQEA75"));
    assert!(output.contains("private_key: [REDACTED]"));
}
