//! Execution implementation details for runner; invoked only by runner.rs

use anyhow::{anyhow, Context};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

// Re-export types/constants from parent module for convenience
use super::{
    ClaudePermissionMode, Model, OutputHandler, ReasoningEffort, RunnerError, RunnerOutput,
    OPENCODE_PROMPT_FILE_MESSAGE, TEMP_RETENTION,
};

use crate::fsutil;

pub(super) struct CtrlCState {
    pub(super) active_pgid: Mutex<Option<i32>>,
    pub(super) interrupted: AtomicBool,
}

pub(super) fn ctrlc_state() -> &'static Arc<CtrlCState> {
    static STATE: OnceLock<Arc<CtrlCState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let state = Arc::new(CtrlCState {
            active_pgid: Mutex::new(None),
            interrupted: AtomicBool::new(false),
        });
        let handler_state = Arc::clone(&state);
        let _ = ctrlc::set_handler(move || {
            handler_state.interrupted.store(true, Ordering::SeqCst);
            let pgid = handler_state
                .active_pgid
                .lock()
                .ok()
                .and_then(|guard| *guard);
            if let Some(pgid) = pgid {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-pgid, libc::SIGINT);
                }
            }
        });
        state
    })
}

pub(super) fn ensure_self_on_path(cmd: &mut Command) {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return,
    };
    let dir = match exe.parent() {
        Some(dir) => dir.to_path_buf(),
        None => return,
    };

    let mut paths = Vec::new();
    paths.push(dir);

    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }

    if let Ok(joined) = std::env::join_paths(paths) {
        cmd.env("PATH", joined);
    }
}

pub(super) enum StreamSink {
    Stdout,
    Stderr,
}

impl StreamSink {
    pub(super) fn write_all(&self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            StreamSink::Stdout => {
                let mut out = std::io::stdout().lock();
                out.write_all(bytes)?;
                out.flush()
            }
            StreamSink::Stderr => {
                let mut err = std::io::stderr().lock();
                err.write_all(bytes)?;
                err.flush()
            }
        }
    }
}

pub(super) fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }
            sink.write_all(&buf[..read])
                .context("stream child output")?;
            let text = String::from_utf8_lossy(&buf[..read]);
            let mut guard = buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("lock output buffer"))?;
            guard.push_str(&text);
            // Call output handler if provided
            if let Some(handler) = &output_handler {
                handler(&text);
            }
        }
        Ok(())
    })
}

pub(super) fn wait_for_child(
    child: &mut std::process::Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    let mut interrupt_sent = false;
    let mut kill_sent = false;
    let start = Instant::now();
    let mut interrupt_time = None;

    loop {
        let now = Instant::now();

        if let Some(timeout) = timeout {
            if now.duration_since(start) > timeout && !interrupt_sent {
                log::warn!("Runner timed out after {:?}; sending interrupt", timeout);
                interrupt_sent = true;
                interrupt_time = Some(now);
                #[cfg(unix)]
                {
                    let pgid = ctrlc.active_pgid.lock().ok().and_then(|guard| *guard);
                    if let Some(pgid) = pgid {
                        unsafe {
                            libc::kill(-pgid, libc::SIGINT);
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = child.kill();
                }
            }
        }

        if ctrlc.interrupted.load(Ordering::SeqCst) && !interrupt_sent {
            interrupt_sent = true;
            interrupt_time = Some(now);
            #[cfg(unix)]
            {
                let pgid = ctrlc.active_pgid.lock().ok().and_then(|guard| *guard);
                if let Some(pgid) = pgid {
                    unsafe {
                        libc::kill(-pgid, libc::SIGINT);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }
        }

        if interrupt_sent && !kill_sent {
            let elapsed_since_interrupt = now.duration_since(interrupt_time.unwrap());
            if elapsed_since_interrupt > Duration::from_secs(2) {
                kill_sent = true;
                #[cfg(unix)]
                {
                    let pgid = ctrlc.active_pgid.lock().ok().and_then(|guard| *guard);
                    if let Some(pgid) = pgid {
                        unsafe {
                            libc::kill(-pgid, libc::SIGKILL);
                        }
                    }
                }
                let _ = child.kill();
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(timeout) = timeout {
                    if now.duration_since(start) > timeout {
                        return Err(RunnerError::Timeout);
                    }
                }
                return Ok(status);
            }
            Ok(None) => {} // Continue waiting
            Err(e) => return Err(RunnerError::Io(e)),
        }

        thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn permission_mode_to_arg(mode: ClaudePermissionMode) -> &'static str {
    match mode {
        ClaudePermissionMode::AcceptEdits => "acceptEdits",
        ClaudePermissionMode::BypassPermissions => "bypassPermissions",
    }
}

/// Stream JSON output with visual filtering
pub(super) fn run_with_streaming_json(
    mut cmd: Command,
    stdin_payload: Option<&[u8]>,
    bin: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_payload.is_some() {
        cmd.stdin(Stdio::piped());
    }

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let ctrlc = ctrlc_state();
    ctrlc.interrupted.store(false, Ordering::SeqCst);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RunnerError::BinaryMissing {
                bin: bin.to_string(),
                source: e,
            }
        } else {
            RunnerError::SpawnFailed {
                bin: bin.to_string(),
                source: e,
            }
        }
    })?;

    if let Some(payload) = stdin_payload {
        let mut stdin = child.stdin.take().ok_or_else(|| {
            RunnerError::Other(anyhow!("failed to open stdin for child: {}", bin))
        })?;
        stdin.write_all(payload).map_err(RunnerError::Io)?;
        drop(stdin);
    }

    #[cfg(unix)]
    {
        let mut guard = ctrlc
            .active_pgid
            .lock()
            .map_err(|_| RunnerError::Other(anyhow!("lock ctrl-c state")))?;
        let pid = child.id() as i32;
        *guard = Some(pid);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RunnerError::Other(anyhow!("capture child stdout")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| RunnerError::Other(anyhow!("capture child stderr")))?;

    let stdout_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf = Arc::new(Mutex::new(String::new()));

    let session_id_buf = Arc::new(Mutex::new(None));
    let stdout_handle = spawn_json_reader(
        stdout,
        StreamSink::Stdout,
        Arc::clone(&stdout_buf),
        output_handler.clone(),
        Arc::clone(&session_id_buf),
    );
    let stderr_handle = spawn_reader(
        stderr,
        StreamSink::Stderr,
        Arc::clone(&stderr_buf),
        output_handler,
    );

    let status = wait_for_child(&mut child, ctrlc, timeout)?;

    #[cfg(unix)]
    {
        let mut guard = ctrlc
            .active_pgid
            .lock()
            .map_err(|_| RunnerError::Other(anyhow!("lock ctrl-c state")))?;
        *guard = None;
    }

    stdout_handle
        .join()
        .map_err(|_| RunnerError::Other(anyhow!("stdout reader panicked")))?
        .map_err(RunnerError::Other)?;
    stderr_handle
        .join()
        .map_err(|_| RunnerError::Other(anyhow!("stderr reader panicked")))?
        .map_err(RunnerError::Other)?;

    let stdout = {
        let mut guard = stdout_buf
            .lock()
            .map_err(|_| RunnerError::Other(anyhow!("lock stdout buffer")))?;
        std::mem::take(&mut *guard)
    };
    let stderr = {
        let mut guard = stderr_buf
            .lock()
            .map_err(|_| RunnerError::Other(anyhow!("lock stderr buffer")))?;
        std::mem::take(&mut *guard)
    };

    if ctrlc.interrupted.load(Ordering::SeqCst) {
        return Err(RunnerError::Interrupted);
    }

    let session_id = session_id_buf
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .or_else(|| extract_session_id_from_text(&stdout));

    Ok(RunnerOutput {
        status,
        stdout,
        stderr,
        session_id,
    })
}

/// Spawn a reader that parses JSON lines and displays meaningful content
fn spawn_json_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
    session_id_buf: Arc<Mutex<Option<String>>>,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut line_buf = String::new();
        let mut tool_name_by_id: HashMap<String, String> = HashMap::new();

        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }

            let text = String::from_utf8_lossy(&buf[..read]);
            for ch in text.chars() {
                if ch == '\n' {
                    if let Some(mut json) = parse_json_line(&line_buf) {
                        if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
                            if event_type == "tool_use" {
                                if let (Some(tool_id), Some(tool_name)) = (
                                    json.get("tool_id").and_then(|v| v.as_str()),
                                    json.get("tool_name").and_then(|v| v.as_str()),
                                ) {
                                    tool_name_by_id
                                        .insert(tool_id.to_string(), tool_name.to_string());
                                }
                            } else if event_type == "tool_result" {
                                let tool_id = json.get("tool_id").and_then(|v| v.as_str());
                                if let Some(tool_id) = tool_id {
                                    if let Some(tool_name) = tool_name_by_id.remove(tool_id) {
                                        if let Some(obj) = json.as_object_mut() {
                                            obj.insert(
                                                "tool_name".to_string(),
                                                JsonValue::String(tool_name),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(id) = extract_session_id_from_json(&json) {
                            if let Ok(mut guard) = session_id_buf.lock() {
                                *guard = Some(id);
                            }
                        }
                        display_filtered_json(&json, &sink, output_handler.as_ref())?;
                    } else if !line_buf.trim().is_empty() {
                        let mut line = line_buf.clone();
                        sink.write_all(line.as_bytes())?;
                        sink.write_all(b"\n")?;
                        if let Some(handler) = &output_handler {
                            line.push('\n');
                            handler(&line);
                        }
                    }
                    line_buf.clear();
                } else {
                    line_buf.push(ch);
                }
            }

            // Lock buffer and append
            let mut guard = buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("lock output buffer"))?;
            guard.push_str(&text);
        }

        if !line_buf.trim().is_empty() {
            let mut line = line_buf.clone();
            sink.write_all(line.as_bytes())?;
            sink.write_all(b"\n")?;
            if let Some(handler) = &output_handler {
                line.push('\n');
                handler(&line);
            }
        }
        Ok(())
    })
}

fn parse_json_line(line: &str) -> Option<JsonValue> {
    serde_json::from_str::<JsonValue>(line).ok()
}

fn extract_session_id_from_json(json: &JsonValue) -> Option<String> {
    if let Some(id) = json.get("thread_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    if let Some(id) = json.get("session_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    if let Some(id) = json.get("sessionID").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    None
}

fn extract_session_id_from_text(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            if let Some(id) = extract_session_id_from_json(&json) {
                return Some(id);
            }
        }
    }
    None
}

const CODEX_REASONING_PREFIX: &str = "[Reasoning] ";
const TOOL_VALUE_MAX_LEN: usize = 160;

fn extract_display_lines(json: &JsonValue) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
        if !result.is_empty() {
            lines.push(result.to_string());
        }
    }

    if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
        if event_type == "assistant" {
            if let Some(message) = json.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_array()) {
                    for item in content {
                        if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                            match item_type {
                                "text" => {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        lines.push(text.to_string());
                                    }
                                }
                                "thinking" | "analysis" | "reasoning" => {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                        if !text.is_empty() {
                                            lines.push(format!(
                                                "{}{}",
                                                CODEX_REASONING_PREFIX, text
                                            ));
                                        }
                                    }
                                }
                                "tool_use" => {
                                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                        let suffix = item
                                            .get("input")
                                            .and_then(format_tool_details)
                                            .map(|details| format!(" {}", details))
                                            .unwrap_or_default();
                                        lines.push(format!("[Tool] {}{}", name, suffix));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        if event_type == "item.completed" || event_type == "item.started" {
            if let Some(item) = json.get("item") {
                if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                    match item_type {
                        "agent_message" => {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    lines.push(text.to_string());
                                    lines.push(String::new());
                                }
                            }
                        }
                        "reasoning" => {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    lines.push(format!("{}{}", CODEX_REASONING_PREFIX, text));
                                }
                            }
                        }
                        "mcp_tool_call" => {
                            if let Some(line) = format_codex_tool_line(item) {
                                lines.push(line);
                            }
                        }
                        "command_execution" => {
                            if let Some(line) = format_codex_command_line(item) {
                                lines.push(line);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if event_type == "text" {
            if let Some(text) = json
                .get("part")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
            {
                lines.push(text.to_string());
            }
        }

        if event_type == "tool_use" {
            if let Some(tool) = json
                .get("part")
                .and_then(|p| p.get("tool"))
                .and_then(|t| t.as_str())
            {
                let status = json
                    .get("part")
                    .and_then(|p| p.get("state"))
                    .and_then(|s| s.get("status"))
                    .and_then(|s| s.as_str());
                let suffix = status
                    .map(|value| format!(" ({value})"))
                    .unwrap_or_default();
                let details = json
                    .get("part")
                    .and_then(|p| {
                        p.get("state")
                            .and_then(|s| s.get("input"))
                            .or_else(|| p.get("input"))
                    })
                    .and_then(format_tool_details)
                    .map(|details| format!(" {}", details))
                    .unwrap_or_default();
                lines.push(format!("[Tool] {tool}{suffix}{details}"));
            }
        }

        if event_type == "message" {
            let role = json.get("role").and_then(|r| r.as_str());
            if role == Some("assistant") {
                if let Some(content) = json.get("content") {
                    match content {
                        JsonValue::String(text) => {
                            if !text.is_empty() {
                                lines.push(text.clone());
                            }
                        }
                        JsonValue::Array(items) => {
                            for item in items {
                                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                    if !text.is_empty() {
                                        lines.push(text.to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if event_type == "tool_use" {
            if let Some(tool) = json.get("tool_name").and_then(|t| t.as_str()) {
                let details = json
                    .get("parameters")
                    .and_then(format_tool_details)
                    .map(|details| format!(" {}", details))
                    .unwrap_or_default();
                lines.push(format!("[Tool] {tool}{details}"));
            }
        }

        if event_type == "tool_result" {
            if let Some(tool) = json.get("tool_name").and_then(|t| t.as_str()) {
                let status = json
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("completed");
                lines.push(format!("[Tool] {tool} ({status})"));
            }
        }
    }

    if let Some(denials) = json.get("permission_denials").and_then(|d| d.as_array()) {
        for denial in denials {
            if let Some(tool_name) = denial.get("tool_name").and_then(|t| t.as_str()) {
                lines.push(format!("[Permission denied: {}]", tool_name));
            }
        }
    }

    lines
}

pub(super) fn extract_final_assistant_response(stdout: &str) -> Option<String> {
    let mut final_message: Option<String> = None;
    let mut streaming_buffer = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(json) = parse_json_line(line) else {
            continue;
        };

        let Some(event_type) = json.get("type").and_then(|t| t.as_str()) else {
            continue;
        };

        match event_type {
            "item.completed" => {
                if let Some(text) = extract_codex_agent_message(&json) {
                    final_message = Some(text);
                    streaming_buffer.clear();
                }
            }
            "assistant" => {
                if let Some(text) = extract_claude_assistant_text(&json) {
                    final_message = Some(text);
                    streaming_buffer.clear();
                }
            }
            "message" => {
                if let Some(text) = extract_gemini_assistant_text(&json) {
                    final_message = Some(text);
                    streaming_buffer.clear();
                }
            }
            "text" => {
                if let Some(text) = extract_opencode_text(&json) {
                    if !text.is_empty() {
                        streaming_buffer.push_str(text);
                        final_message = Some(streaming_buffer.clone());
                    }
                }
            }
            _ => {}
        }
    }

    final_message
}

fn extract_codex_agent_message(json: &JsonValue) -> Option<String> {
    let item = json.get("item")?;
    if item.get("type").and_then(|t| t.as_str()) != Some("agent_message") {
        return None;
    }
    let text = item.get("text").and_then(|t| t.as_str())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_claude_assistant_text(json: &JsonValue) -> Option<String> {
    let message = json.get("message")?;
    let content = message.get("content")?.as_array()?;
    let mut parts = Vec::new();
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) != Some("text") {
            continue;
        }
        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn extract_gemini_assistant_text(json: &JsonValue) -> Option<String> {
    if json.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return None;
    }
    let content = json.get("content")?;
    extract_text_content(content)
}

fn extract_opencode_text(json: &JsonValue) -> Option<&str> {
    json.get("part")
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
}

fn extract_text_content(content: &JsonValue) -> Option<String> {
    match content {
        JsonValue::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        JsonValue::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

fn format_codex_tool_line(item: &JsonValue) -> Option<String> {
    let server = item.get("server").and_then(|s| s.as_str());
    let tool = item.get("tool").and_then(|t| t.as_str());
    let name = match (server, tool) {
        (Some(server), Some(tool)) => format!("{}.{}", server, tool),
        (Some(server), None) => server.to_string(),
        (None, Some(tool)) => tool.to_string(),
        (None, None) => return None,
    };

    let details = item
        .get("arguments")
        .or_else(|| item.get("args"))
        .or_else(|| item.get("input"))
        .and_then(format_tool_details)
        .map(|details| format!(" {}", details))
        .unwrap_or_default();
    Some(format!("[Tool] {}{}{}", name, status_suffix(item), details))
}

fn format_codex_command_line(item: &JsonValue) -> Option<String> {
    let command = item.get("command").and_then(|c| c.as_str())?;
    let mut suffix = status_suffix(item);
    if let Some(exit_code) = item.get("exit_code").and_then(|code| code.as_i64()) {
        suffix = format!("{} (exit {})", suffix.trim_end(), exit_code);
    }
    Some(format!("[Command] {}{}", command, suffix))
}

fn status_suffix(item: &JsonValue) -> String {
    item.get("status")
        .and_then(|s| s.as_str())
        .map(|status| format!(" ({})", status))
        .unwrap_or_default()
}

fn format_tool_details(input: &JsonValue) -> Option<String> {
    let object = input.as_object()?;
    let mut parts = Vec::new();

    if let Some(action) = lookup_string(object, &["action", "op", "fn"]) {
        parts.push(format!("action={}", action));
    }

    if let Some(path) = lookup_string(object, &["path", "file_path", "filePath"]) {
        parts.push(format!("path={}", path));
    }

    if let Some(paths) = lookup_array_len(object, &["paths", "file_paths", "files"]) {
        parts.push(format!("paths={}", paths));
    }

    if let Some(command) = lookup_string(object, &["command", "cmd"]) {
        let value = normalize_tool_value(&command);
        parts.push(format!("cmd={}", truncate_tool_value(&value)));
    }

    if let Some(pattern) = lookup_string(object, &["pattern", "glob", "query"]) {
        let value = normalize_tool_value(&pattern);
        parts.push(format!("pattern={}", truncate_tool_value(&value)));
    }

    if let Some(content) = lookup_string(object, &["content", "text", "message"]) {
        let value = normalize_tool_value(&content);
        parts.push(format!("content_len={}", content.len()));
        if !value.is_empty() {
            parts.push(format!("content={}", truncate_tool_value(&value)));
        }
    }

    if let Some(edits) = lookup_array_len(object, &["edits", "slices"]) {
        parts.push(format!("edits={}", edits));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn lookup_string(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(text) = value.as_str() {
                return Some(text.to_string());
            }
            if value.is_number() || value.is_boolean() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn lookup_array_len(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<usize> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(array) = value.as_array() {
                return Some(array.len());
            }
        }
    }
    None
}

fn normalize_tool_value(value: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in value.trim().chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            last_space = false;
            out.push(ch);
        }
    }
    out
}

fn truncate_tool_value(value: &str) -> String {
    if value.len() <= TOOL_VALUE_MAX_LEN {
        return value.to_string();
    }
    let mut out = String::new();
    for ch in value.chars().take(TOOL_VALUE_MAX_LEN - 1) {
        out.push(ch);
    }
    out.push('…');
    out
}

/// Display meaningful content from JSON, filtering noise
fn display_filtered_json(
    json: &JsonValue,
    sink: &StreamSink,
    output_handler: Option<&OutputHandler>,
) -> anyhow::Result<()> {
    for mut line in extract_display_lines(json) {
        sink.write_all(line.as_bytes())?;
        sink.write_all(b"\n")?;
        if let Some(handler) = output_handler {
            line.push('\n');
            handler(&line);
        }
    }

    Ok(())
}

pub fn run_codex(
    work_dir: &Path,
    bin: &str,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("exec")
        .arg("--json")
        .arg("--model")
        .arg(model.as_str());

    if let Some(effort) = reasoning_effort {
        cmd.arg("-c").arg(format!(
            "model_reasoning_effort=\"{}\"",
            effort_as_str(effort)
        ));
    }

    cmd.arg("-");
    run_with_streaming_json(cmd, Some(prompt.as_bytes()), bin, timeout, output_handler)
}

#[allow(clippy::too_many_arguments)]
pub fn run_codex_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    thread_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("exec")
        .arg("resume")
        .arg(thread_id)
        .arg("--json")
        .arg("--model")
        .arg(model.as_str());

    if let Some(effort) = reasoning_effort {
        cmd.arg("-c").arg(format!(
            "model_reasoning_effort=\"{}\"",
            effort_as_str(effort)
        ));
    }

    cmd.arg(message);
    run_with_streaming_json(cmd, None, bin, timeout, output_handler)
}

pub fn run_opencode(
    work_dir: &Path,
    bin: &str,
    model: &Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    if let Err(err) = fsutil::cleanup_default_temp_dirs(TEMP_RETENTION) {
        log::warn!("temp cleanup failed: {:#}", err);
    }

    let temp_dir = fsutil::create_ralph_temp_dir("prompt")
        .map_err(|e| RunnerError::Other(anyhow!("create temp dir: {}", e)))?;
    let mut tmp = tempfile::Builder::new()
        .prefix("prompt_")
        .suffix(".md")
        .tempfile_in(temp_dir.path())
        .map_err(|e| RunnerError::Other(anyhow!("create temp prompt file: {}", e)))?;

    tmp.write_all(prompt.as_bytes())
        .map_err(|e| RunnerError::Other(anyhow!("write prompt file: {}", e)))?;
    tmp.flush()
        .map_err(|e| RunnerError::Other(anyhow!("flush prompt file: {}", e)))?;

    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("run")
        .arg("--model")
        .arg(model.as_str())
        .arg("--format")
        .arg("json")
        .arg("--file")
        .arg(tmp.path())
        .arg("--")
        .arg(OPENCODE_PROMPT_FILE_MESSAGE)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    run_with_streaming_json(cmd, None, bin, timeout, output_handler)
}

pub fn run_opencode_resume(
    work_dir: &Path,
    bin: &str,
    model: &Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("run")
        .arg("-s")
        .arg(session_id)
        .arg("--model")
        .arg(model.as_str())
        .arg("--format")
        .arg("json")
        .arg("--")
        .arg(message);
    run_with_streaming_json(cmd, None, bin, timeout, output_handler)
}

pub fn run_gemini(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("--model")
        .arg(model.as_str())
        .arg("--output-format")
        .arg("stream-json")
        .arg("--approval-mode")
        .arg("yolo");
    run_with_streaming_json(cmd, Some(prompt.as_bytes()), bin, timeout, output_handler)
}

pub fn run_gemini_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("--resume")
        .arg(session_id)
        .arg("--model")
        .arg(model.as_str())
        .arg("--output-format")
        .arg("stream-json")
        .arg("--approval-mode")
        .arg("yolo")
        .arg(message);
    run_with_streaming_json(cmd, None, bin, timeout, output_handler)
}

pub fn run_claude(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mode = permission_mode.unwrap_or(ClaudePermissionMode::BypassPermissions);
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("-p") // Print mode (headless, skips workspace trust)
        .arg("--model")
        .arg(model.as_str())
        .arg("--permission-mode")
        .arg(permission_mode_to_arg(mode))
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose");
    run_with_streaming_json(cmd, Some(prompt.as_bytes()), bin, timeout, output_handler)
}

#[allow(clippy::too_many_arguments)]
pub fn run_claude_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
) -> Result<RunnerOutput, RunnerError> {
    let mode = permission_mode.unwrap_or(ClaudePermissionMode::BypassPermissions);
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("--resume")
        .arg(session_id)
        .arg("--model")
        .arg(model.as_str())
        .arg("--permission-mode")
        .arg(permission_mode_to_arg(mode))
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose")
        .arg("-p")
        .arg(message);
    run_with_streaming_json(cmd, None, bin, timeout, output_handler)
}

fn effort_as_str(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::XHigh => "xhigh",
    }
}

/// Run a command with streaming output support.
/// If `output_handler` is provided, output chunks are sent to callback as they arrive.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permission_mode_to_arg_mapping() {
        assert_eq!(
            permission_mode_to_arg(ClaudePermissionMode::AcceptEdits),
            "acceptEdits"
        );
        assert_eq!(
            permission_mode_to_arg(ClaudePermissionMode::BypassPermissions),
            "bypassPermissions"
        );
    }

    #[test]
    fn effort_as_str_mapping() {
        assert_eq!(effort_as_str(ReasoningEffort::Low), "low");
        assert_eq!(effort_as_str(ReasoningEffort::Medium), "medium");
        assert_eq!(effort_as_str(ReasoningEffort::High), "high");
        assert_eq!(effort_as_str(ReasoningEffort::XHigh), "xhigh");
    }

    #[test]
    fn parse_json_line_handles_invalid_json() {
        assert!(parse_json_line("{").is_none());
    }

    #[test]
    fn extract_session_id_from_json_codex_thread_id() {
        let payload = json!({
            "thread_id": "thread-123"
        });
        assert_eq!(
            extract_session_id_from_json(&payload),
            Some("thread-123".to_string())
        );
    }

    #[test]
    fn extract_session_id_from_json_claude_session_id() {
        let payload = json!({
            "session_id": "session-abc"
        });
        assert_eq!(
            extract_session_id_from_json(&payload),
            Some("session-abc".to_string())
        );
    }

    #[test]
    fn extract_session_id_from_json_gemini_session_id() {
        let payload = json!({
            "session_id": "gemini-xyz"
        });
        assert_eq!(
            extract_session_id_from_json(&payload),
            Some("gemini-xyz".to_string())
        );
    }

    #[test]
    fn extract_session_id_from_json_opencode_session_id() {
        let payload = json!({
            "sessionID": "open-789"
        });
        assert_eq!(
            extract_session_id_from_json(&payload),
            Some("open-789".to_string())
        );
    }

    #[test]
    fn extract_session_id_from_text_reads_json_lines() {
        let stdout = "{\"session_id\":\"sess-001\"}\n{\"result\":\"ok\"}\n";
        assert_eq!(
            extract_session_id_from_text(stdout),
            Some("sess-001".to_string())
        );
    }

    #[test]
    fn extract_display_lines_codex_agent_message() {
        let payload = json!({
            "type": "item.completed",
            "item": {"type": "agent_message", "text": "Hi!"}
        });
        assert_eq!(extract_display_lines(&payload), vec!["Hi!", ""]);
    }

    #[test]
    fn extract_display_lines_codex_reasoning() {
        let payload = json!({
            "type": "item.completed",
            "item": {"type": "reasoning", "text": "Working it out"}
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["[Reasoning] Working it out"]
        );
    }

    #[test]
    fn extract_display_lines_codex_tool_call() {
        let payload = json!({
            "type": "item.completed",
            "item": {
                "type": "mcp_tool_call",
                "server": "RepoPrompt",
                "tool": "get_file_tree",
                "status": "completed",
                "arguments": {
                    "path": "/tmp/project",
                    "pattern": "*.rs"
                }
            }
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["[Tool] RepoPrompt.get_file_tree (completed) path=/tmp/project pattern=*.rs"]
        );
    }

    #[test]
    fn extract_display_lines_codex_command_execution() {
        let payload = json!({
            "type": "item.started",
            "item": {
                "type": "command_execution",
                "command": "/bin/zsh -lc ls",
                "status": "in_progress",
                "exit_code": null
            }
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["[Command] /bin/zsh -lc ls (in_progress)"]
        );
    }

    #[test]
    fn extract_display_lines_claude_result_and_tool_use() {
        let payload = json!({
            "result": "Final answer",
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "text", "text": "Streamed text"},
                    {"type": "tool_use", "name": "Read", "input": {"file_path": "/tmp/a.txt"}}
                ]
            }
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec![
                "Final answer",
                "Streamed text",
                "[Tool] Read path=/tmp/a.txt"
            ]
        );
    }

    #[test]
    fn extract_display_lines_permission_denial() {
        let payload = json!({
            "permission_denials": [
                {"tool_name": "write"}
            ]
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["[Permission denied: write]"]
        );
    }

    #[test]
    fn display_filtered_json_calls_output_handler() {
        let payload = json!({
            "type": "text",
            "part": { "text": "hello" }
        });
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let handler: OutputHandler = Arc::new(Box::new({
            let captured = Arc::clone(&captured);
            move |text: &str| {
                captured
                    .lock()
                    .expect("capture lock")
                    .push(text.to_string());
            }
        }));

        display_filtered_json(&payload, &StreamSink::Stdout, Some(&handler))
            .expect("display filtered json");

        let guard = captured.lock().expect("capture lock");
        assert_eq!(guard.as_slice(), &["hello\n".to_string()]);
    }

    #[test]
    fn extract_display_lines_opencode_text() {
        let payload = json!({
            "type": "text",
            "part": { "text": "hello" }
        });
        assert_eq!(extract_display_lines(&payload), vec!["hello"]);
    }

    #[test]
    fn extract_display_lines_opencode_tool_use() {
        let payload = json!({
            "type": "tool_use",
            "part": {
                "tool": "read",
                "state": {
                    "status": "completed",
                    "input": { "filePath": "/tmp/example.txt" }
                }
            }
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["[Tool] read (completed) path=/tmp/example.txt"]
        );
    }

    #[test]
    fn extract_display_lines_gemini_tool_use_and_result() {
        let tool_use = json!({
            "type": "tool_use",
            "tool_name": "read_file",
            "parameters": { "file_path": "notes.txt" }
        });
        assert_eq!(
            extract_display_lines(&tool_use),
            vec!["[Tool] read_file path=notes.txt"]
        );

        let tool_result = json!({
            "type": "tool_result",
            "tool_name": "read_file",
            "status": "success"
        });
        assert_eq!(
            extract_display_lines(&tool_result),
            vec!["[Tool] read_file (success)"]
        );
    }

    #[test]
    fn extract_display_lines_gemini_message_assistant() {
        let payload = json!({
            "type": "message",
            "role": "assistant",
            "content": "hi"
        });
        assert_eq!(extract_display_lines(&payload), vec!["hi"]);
    }

    #[test]
    fn extract_display_lines_unknown_event_is_noop() {
        let payload = json!({"type": "unknown"});
        assert!(extract_display_lines(&payload).is_empty());
    }

    #[test]
    fn extract_final_assistant_response_codex_agent_message() {
        let stdout = concat!(
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Draft"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Final answer"}}"#,
            "\n"
        );
        assert_eq!(
            extract_final_assistant_response(stdout),
            Some("Final answer".to_string())
        );
    }

    #[test]
    fn extract_final_assistant_response_claude_assistant_message() {
        let stdout = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"First line"},{"type":"tool_use","name":"Read"}]}}"#,
            "\n"
        );
        assert_eq!(
            extract_final_assistant_response(stdout),
            Some("First line".to_string())
        );
    }

    #[test]
    fn extract_final_assistant_response_gemini_message_assistant() {
        let stdout = concat!(
            r#"{"type":"message","role":"assistant","content":[{"text":"Hello"},{"text":"World"}]}"#,
            "\n"
        );
        assert_eq!(
            extract_final_assistant_response(stdout),
            Some("Hello\nWorld".to_string())
        );
    }

    #[test]
    fn extract_final_assistant_response_opencode_text_stream() {
        let stdout = concat!(
            r#"{"type":"text","part":{"text":"Hello "}}"#,
            "\n",
            r#"{"type":"text","part":{"text":"world"}}"#,
            "\n"
        );
        assert_eq!(
            extract_final_assistant_response(stdout),
            Some("Hello world".to_string())
        );
    }

    #[test]
    fn extract_final_assistant_response_none_when_missing() {
        let stdout = concat!(r#"{"type":"tool_use","tool_name":"read"}"#, "\n");
        assert_eq!(extract_final_assistant_response(stdout), None);
    }
}
