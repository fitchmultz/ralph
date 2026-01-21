//! Execution implementation details for runner; invoked only by runner.rs

use anyhow::{anyhow, Context};
use serde_json::Value as JsonValue;
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

    let stdout_handle = spawn_json_reader(
        stdout,
        StreamSink::Stdout,
        Arc::clone(&stdout_buf),
        output_handler.clone(),
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

    Ok(RunnerOutput {
        status,
        stdout,
        stderr,
    })
}

/// Spawn a reader that parses JSON lines and displays meaningful content
fn spawn_json_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut line_buf = String::new();

        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }

            let text = String::from_utf8_lossy(&buf[..read]);
            for ch in text.chars() {
                if ch == '\n' {
                    if let Some(json) = parse_json_line(&line_buf) {
                        display_filtered_json(&json, &sink)?;
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
            // Call output handler if provided
            if let Some(handler) = &output_handler {
                handler(&text);
            }
        }
        Ok(())
    })
}

fn parse_json_line(line: &str) -> Option<JsonValue> {
    serde_json::from_str::<JsonValue>(line).ok()
}

const CODEX_REASONING_PREFIX: &str = "[Reasoning] ";

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
                                "tool_use" => {
                                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                        lines.push(format!("[Using: {}]", name));
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

fn format_codex_tool_line(item: &JsonValue) -> Option<String> {
    let server = item.get("server").and_then(|s| s.as_str());
    let tool = item.get("tool").and_then(|t| t.as_str());
    let name = match (server, tool) {
        (Some(server), Some(tool)) => format!("{}.{}", server, tool),
        (Some(server), None) => server.to_string(),
        (None, Some(tool)) => tool.to_string(),
        (None, None) => return None,
    };

    Some(format!("[Tool] {}{}", name, status_suffix(item)))
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

/// Display meaningful content from JSON, filtering noise
fn display_filtered_json(json: &JsonValue, sink: &StreamSink) -> anyhow::Result<()> {
    for line in extract_display_lines(json) {
        sink.write_all(line.as_bytes())?;
        sink.write_all(b"\n")?;
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
        .arg("--full-auto")
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
        .arg("--file")
        .arg(tmp.path())
        .arg("--")
        .arg(OPENCODE_PROMPT_FILE_MESSAGE)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    stream_command(cmd, None, bin, timeout, output_handler)
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
        .arg("--approval-mode")
        .arg("yolo");
    stream_command(cmd, Some(prompt.as_bytes()), bin, timeout, output_handler)
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

fn effort_as_str(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
    }
}

/// Run a command with streaming output support.
/// If `output_handler` is provided, output chunks are sent to callback as they arrive.
pub fn stream_command(
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

    let stdout_handle = spawn_reader(
        stdout,
        StreamSink::Stdout,
        Arc::clone(&stdout_buf),
        output_handler.clone(),
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

    Ok(RunnerOutput {
        status,
        stdout,
        stderr,
    })
}

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
        assert_eq!(effort_as_str(ReasoningEffort::Minimal), "minimal");
        assert_eq!(effort_as_str(ReasoningEffort::Low), "low");
        assert_eq!(effort_as_str(ReasoningEffort::Medium), "medium");
        assert_eq!(effort_as_str(ReasoningEffort::High), "high");
    }

    #[test]
    fn parse_json_line_handles_invalid_json() {
        assert!(parse_json_line("{").is_none());
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
                "status": "completed"
            }
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["[Tool] RepoPrompt.get_file_tree (completed)"]
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
                    {"type": "tool_use", "name": "search"}
                ]
            }
        });
        assert_eq!(
            extract_display_lines(&payload),
            vec!["Final answer", "Streamed text", "[Using: search]"]
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
    fn extract_display_lines_unknown_event_is_noop() {
        let payload = json!({"type": "unknown"});
        assert!(extract_display_lines(&payload).is_empty());
    }
}
