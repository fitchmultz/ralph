//! JSON parsing helpers for runner streaming output.

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
    stream.next().and_then(|res| res.ok())
}

pub(super) fn extract_session_id_from_json(json: &JsonValue) -> Option<String> {
    if json.get("type").and_then(|t| t.as_str()) == Some("session") {
        if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }
    if let Some(id) = json.get("thread_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    if let Some(id) = json.get("session_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    if let Some(id) = json.get("sessionID").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    // Kimi stream-json lines sometimes include tool_calls[].id; capture as a fallback when no
    // explicit session id is present in the output.
    if let Some(tool_calls) = json.get("tool_calls").and_then(|v| v.as_array()) {
        if let Some(first_tool) = tool_calls.first() {
            if let Some(id) = first_tool.get("id").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
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

        if let Some(Ok(json)) = stream.next() {
            if let Some(id) = extract_session_id_from_json(&json) {
                return Some(id);
            }
        }
    }
    None
}
