//! JSON parsing helpers for runner streaming output.
//!
//! Purpose:
//! - JSON parsing helpers for runner streaming output.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use serde_json::Value as JsonValue;

pub(super) fn parse_json_line(line: &str) -> Option<JsonValue> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<JsonValue>(trimmed) {
        return Some(value);
    }

    // Some runners interleave logs or ANSI control sequences with JSON. As a best-effort
    // compatibility layer, attempt to parse the first JSON value starting at the first '{'.
    let json_start = trimmed.find('{')?;
    let potential_json = &trimmed[json_start..];
    let mut stream = serde_json::Deserializer::from_str(potential_json).into_iter::<JsonValue>();
    stream.next().and_then(|res| {
        res.inspect_err(|e| log::trace!("JSON stream parse error: {}", e))
            .ok()
    })
}

pub(super) fn extract_session_id_from_json(json: &JsonValue) -> Option<&str> {
    if json.get("type").and_then(|t| t.as_str()) == Some("session")
        && let Some(id) = json.get("id").and_then(|v| v.as_str())
    {
        return Some(id);
    }
    if let Some(id) = json.get("thread_id").and_then(|v| v.as_str()) {
        return Some(id);
    }
    if let Some(id) = json.get("session_id").and_then(|v| v.as_str()) {
        return Some(id);
    }
    if let Some(id) = json.get("sessionID").and_then(|v| v.as_str()) {
        return Some(id);
    }
    None
}

pub(super) fn extract_session_id_from_text(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Find the start of JSON object
        let json_start = match line.find('{') {
            Some(idx) => idx,
            None => continue,
        };

        // Attempt to parse the first JSON object found in the line
        let potential_json = &line[json_start..];
        let mut stream =
            serde_json::Deserializer::from_str(potential_json).into_iter::<JsonValue>();

        if let Some(Ok(json)) = stream.next()
            && let Some(id) = extract_session_id_from_json(&json)
        {
            return Some(id.to_owned());
        }
    }
    None
}
