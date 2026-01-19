use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Model, ReasoningEffort, Runner, TaskAgent,
};
use crate::redaction::{redact_text, RedactedString};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value as JsonValue;
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("runner binary not found: {bin}")]
    BinaryMissing {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runner failed to spawn: {bin}")]
    SpawnFailed {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runner exited non-zero (code={code})\nstdout: {stdout}\nstderr: {stderr}")]
    NonZeroExit {
        code: i32,
        stdout: RedactedString,
        stderr: RedactedString,
    },

    #[error("runner terminated by signal\nstdout: {stdout}\nstderr: {stderr}")]
    TerminatedBySignal {
        stdout: RedactedString,
        stderr: RedactedString,
    },

    #[error("runner interrupted")]
    Interrupted,

    #[error("runner timed out")]
    Timeout,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("other error: {0}")]
    Other(#[from] anyhow::Error),
}

const OPENCODE_PROMPT_FILE_MESSAGE: &str = "Follow the attached prompt file verbatim.";
const GEMINI_PROMPT_PREFIX: &str =
    "If RepoPrompt tools are available, you MUST use them for file search, reading, and edits (do not bypass them).";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3-flash-preview";
const DEFAULT_CLAUDE_MODEL: &str = "sonnet";

struct CtrlCState {
    active_pgid: Mutex<Option<i32>>,
    interrupted: AtomicBool,
}

fn ctrlc_state() -> &'static Arc<CtrlCState> {
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

fn ensure_self_on_path(cmd: &mut Command) {
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

pub struct RunnerOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl fmt::Display for RunnerOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "status: {}\nstdout: {}\nstderr: {}",
            self.status,
            redact_text(&self.stdout),
            redact_text(&self.stderr)
        )
    }
}

impl fmt::Debug for RunnerOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerOutput")
            .field("status", &self.status)
            .field("stdout", &redact_text(&self.stdout))
            .field("stderr", &redact_text(&self.stderr))
            .finish()
    }
}

impl RunnerOutput {}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentSettings {
    pub runner: Runner,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
}

pub fn resolve_agent_settings(
    runner_override: Option<Runner>,
    model_override: Option<Model>,
    effort_override: Option<ReasoningEffort>,
    task_agent: Option<&TaskAgent>,
    config_agent: &AgentConfig,
) -> Result<AgentSettings> {
    let runner = runner_override
        .or(task_agent.and_then(|a| a.runner))
        .or(config_agent.runner)
        .unwrap_or_default();

    let model = resolve_model_for_runner(
        runner,
        model_override,
        task_agent.and_then(|a| a.model.clone()),
        config_agent.model.clone(),
    );

    let effort_candidate = effort_override
        .or(task_agent.and_then(|a| a.reasoning_effort))
        .or(config_agent.reasoning_effort);

    let reasoning_effort = if runner == Runner::Codex {
        Some(effort_candidate.unwrap_or_default())
    } else {
        None
    };

    validate_model_for_runner(runner, &model)?;

    Ok(AgentSettings {
        runner,
        model,
        reasoning_effort,
    })
}

pub fn validate_model_for_runner(runner: Runner, model: &Model) -> Result<()> {
    if runner == Runner::Codex {
        match model {
            Model::Gpt52Codex | Model::Gpt52 => {}
            Model::Glm47 => {
                bail!("model zai-coding-plan/glm-4.7 is not supported for codex runner")
            }
            Model::Custom(name) => bail!(
                "model {} is not supported for codex runner (allowed: gpt-5.2-codex, gpt-5.2)",
                name
            ),
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub struct RunnerBinaries<'a> {
    pub codex: &'a str,
    pub opencode: &'a str,
    pub gemini: &'a str,
    pub claude: &'a str,
}

pub fn resolve_binaries(agent: &AgentConfig) -> RunnerBinaries<'_> {
    let codex = agent.codex_bin.as_deref().unwrap_or("codex");
    let opencode = agent.opencode_bin.as_deref().unwrap_or("opencode");
    let gemini = agent.gemini_bin.as_deref().unwrap_or("gemini");
    let claude = agent.claude_bin.as_deref().unwrap_or("claude");
    RunnerBinaries {
        codex,
        opencode,
        gemini,
        claude,
    }
}

pub fn default_model_for_runner(runner: Runner) -> Model {
    match runner {
        Runner::Codex => Model::Gpt52Codex,
        Runner::Opencode => Model::Glm47,
        Runner::Gemini => Model::Custom(DEFAULT_GEMINI_MODEL.to_string()),
        Runner::Claude => Model::Custom(DEFAULT_CLAUDE_MODEL.to_string()),
    }
}

pub fn resolve_model_for_runner(
    runner: Runner,
    override_model: Option<Model>,
    task_model: Option<Model>,
    config_model: Option<Model>,
) -> Model {
    if let Some(model) = override_model {
        return model;
    }
    if let Some(model) = task_model {
        return model;
    }

    match config_model {
        None => default_model_for_runner(runner),
        Some(model) => {
            if runner != Runner::Codex && model == Model::Gpt52Codex {
                default_model_for_runner(runner)
            } else {
                model
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_prompt(
    runner: Runner,
    work_dir: &Path,
    bins: RunnerBinaries<'_>,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
    two_pass_plan: bool,
    permission_mode: Option<ClaudePermissionMode>,
) -> Result<RunnerOutput, RunnerError> {
    validate_model_for_runner(runner, &model).map_err(RunnerError::Other)?;
    let prepared_prompt = prepare_prompt(runner, prompt);
    let output = match runner {
        Runner::Codex => run_codex(
            work_dir,
            bins.codex,
            model,
            reasoning_effort,
            &prepared_prompt,
            timeout,
        )?,
        Runner::Opencode => {
            run_opencode(work_dir, bins.opencode, model, &prepared_prompt, timeout)?
        }
        Runner::Gemini => run_gemini(work_dir, bins.gemini, model, &prepared_prompt, timeout)?,
        Runner::Claude => run_claude(
            work_dir,
            bins.claude,
            model,
            &prepared_prompt,
            timeout,
            two_pass_plan,
            permission_mode,
        )?,
    };

    if !output.status.success() {
        if let Some(code) = output.status.code() {
            return Err(RunnerError::NonZeroExit {
                code,
                stdout: output.stdout.into(),
                stderr: output.stderr.into(),
            });
        } else {
            return Err(RunnerError::TerminatedBySignal {
                stdout: output.stdout.into(),
                stderr: output.stderr.into(),
            });
        }
    }

    Ok(output)
}

fn prepare_prompt(runner: Runner, prompt: &str) -> String {
    if runner == Runner::Gemini {
        let trimmed = prompt.trim_start();
        if trimmed.is_empty() {
            return format!("{GEMINI_PROMPT_PREFIX}\n");
        }
        format!("{GEMINI_PROMPT_PREFIX}\n\n{prompt}")
    } else {
        prompt.to_string()
    }
}

fn run_codex(
    work_dir: &Path,
    bin: &str,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("exec")
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
    run_with_streaming(cmd, Some(prompt.as_bytes()), bin, timeout)
}

fn run_opencode(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
) -> Result<RunnerOutput, RunnerError> {
    let mut tmp = tempfile::Builder::new()
        .prefix("ralph_prompt_")
        .suffix(".md")
        .tempfile()
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

    run_with_streaming(cmd, None, bin, timeout)
}

fn run_gemini(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
) -> Result<RunnerOutput, RunnerError> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("--model")
        .arg(model.as_str())
        .arg("--approval-mode")
        .arg("yolo");
    run_with_streaming(cmd, Some(prompt.as_bytes()), bin, timeout)
}

fn run_claude(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    two_pass_plan: bool,
    permission_mode: Option<ClaudePermissionMode>,
) -> Result<RunnerOutput, RunnerError> {
    if two_pass_plan {
        run_claude_two_pass(work_dir, bin, model, prompt, timeout, permission_mode)
    } else {
        run_claude_direct(work_dir, bin, model, prompt, timeout, permission_mode)
    }
}

fn run_claude_direct(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
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
    run_with_streaming_json(cmd, Some(prompt.as_bytes()), bin, timeout)
}

fn permission_mode_to_arg(mode: ClaudePermissionMode) -> &'static str {
    match mode {
        ClaudePermissionMode::AcceptEdits => "acceptEdits",
        ClaudePermissionMode::BypassPermissions => "bypassPermissions",
    }
}

fn run_claude_two_pass(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
) -> Result<RunnerOutput, RunnerError> {
    log::info!("Claude two-pass mode: generating plan first");

    // Pass 1: Generate plan with bypassPermissions + planning constraint
    let plan_output = match generate_claude_plan(work_dir, bin, &model, prompt, timeout) {
        Ok(plan) => plan,
        Err(e) => {
            log::warn!(
                "Plan generation failed: {}, falling back to direct implementation",
                e
            );
            return run_claude_direct(work_dir, bin, model, prompt, timeout, permission_mode);
        }
    };

    // Extract plan from stream-json output
    let plan_text = match parse_stream_json_plan(&plan_output.stdout) {
        Ok(text) => text,
        Err(e) => {
            log::warn!(
                "Failed to parse plan from stream-json: {}, using raw output",
                e
            );
            plan_output.stdout.trim().to_string()
        }
    };

    // Pass 2: Implement with configured permission mode
    let implementation_prompt = format!("Implement this plan:\n\n{}", plan_text);

    log::info!(
        "Claude two-pass mode: implementing plan ({} bytes)",
        implementation_prompt.len()
    );

    run_claude_direct(
        work_dir,
        bin,
        model,
        &implementation_prompt,
        timeout,
        permission_mode,
    )
}

fn generate_claude_plan(
    work_dir: &Path,
    bin: &str,
    model: &Model,
    prompt: &str,
    timeout: Option<Duration>,
) -> Result<RunnerOutput, RunnerError> {
    // Add planning constraint to the prompt
    let planning_prompt = format!(
        "PLANNING MODE: You are in planning mode. Analyze the codebase and generate a plan, \
        but DO NOT make any edits or changes. Only explore using tools, then output your plan.\n\n{}",
        prompt
    );

    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("-p")
        .arg("--model")
        .arg(model.as_str())
        .arg("--permission-mode")
        .arg("bypassPermissions")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--verbose");

    run_with_streaming_json(cmd, Some(planning_prompt.as_bytes()), bin, timeout)
}

/// Parse stream-json output and extract the result field
fn parse_stream_json_plan(json_output: &str) -> Result<String> {
    let mut last_result = None;

    for line in json_output.lines() {
        if let Ok(json) = serde_json::from_str::<JsonValue>(line) {
            if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
                last_result = Some(result.to_string());
            }
            // Log permission denials
            if let Some(denials) = json.get("permission_denials").and_then(|d| d.as_array()) {
                for denial in denials {
                    if let Some(tool_name) = denial.get("tool_name").and_then(|t| t.as_str()) {
                        if let Some(input) = denial.get("tool_input") {
                            log::warn!("Permission denied: {} (input: {})", tool_name, input);
                        } else {
                            log::warn!("Permission denied: {}", tool_name);
                        }
                    }
                }
            }
        }
    }

    last_result.ok_or_else(|| anyhow!("No result field found in stream-json output"))
}

/// Stream JSON output with visual filtering
fn run_with_streaming_json(
    mut cmd: Command,
    stdin_payload: Option<&[u8]>,
    bin: &str,
    timeout: Option<Duration>,
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

    let stdout_handle = spawn_json_reader(stdout, StreamSink::Stdout, Arc::clone(&stdout_buf));
    let stderr_handle = spawn_reader(stderr, StreamSink::Stderr, Arc::clone(&stderr_buf));

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
                    if let Ok(json) = serde_json::from_str::<JsonValue>(&line_buf) {
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
        }
        Ok(())
    })
}

/// Display meaningful content from JSON, filtering noise
fn display_filtered_json(json: &JsonValue, sink: &StreamSink) -> anyhow::Result<()> {
    // Display the result field (actual text output from Claude)
    if let Some(result) = json.get("result").and_then(|r| r.as_str()) {
        if !result.is_empty() {
            sink.write_all(result.as_bytes())?;
            sink.write_all(b"\n")?;
        }
    }

    // Display tool use events for visibility
    if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
        if event_type == "assistant" {
            if let Some(message) = json.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_array()) {
                    for item in content {
                        if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                            if item_type == "tool_use" {
                                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                    let msg = format!("[Using: {}]\n", name);
                                    sink.write_all(msg.as_bytes())?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Display permission denials
    if let Some(denials) = json.get("permission_denials").and_then(|d| d.as_array()) {
        for denial in denials {
            if let Some(tool_name) = denial.get("tool_name").and_then(|t| t.as_str()) {
                let msg = format!("[Permission denied: {}]\n", tool_name);
                sink.write_all(msg.as_bytes())?;
            }
        }
    }

    Ok(())
}

enum StreamSink {
    Stdout,
    Stderr,
}

impl StreamSink {
    fn write_all(&self, bytes: &[u8]) -> std::io::Result<()> {
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

fn run_with_streaming(
    mut cmd: Command,
    stdin_payload: Option<&[u8]>,
    bin: &str,
    timeout: Option<Duration>,
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

    let stdout_handle = spawn_reader(stdout, StreamSink::Stdout, Arc::clone(&stdout_buf));
    let stderr_handle = spawn_reader(stderr, StreamSink::Stderr, Arc::clone(&stderr_buf));

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

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
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
        }
        Ok(())
    })
}

fn wait_for_child(
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
            Ok(None) => {}
            Err(e) => return Err(RunnerError::Io(e)),
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn effort_as_str(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
    }
}

pub fn parse_model(value: &str) -> Result<Model> {
    let trimmed = value.trim();
    let model = trimmed.parse::<Model>().map_err(|err| anyhow!(err))?;
    Ok(model)
}

pub fn parse_reasoning_effort(value: &str) -> Result<ReasoningEffort> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "minimal" => Ok(ReasoningEffort::Minimal),
        "low" => Ok(ReasoningEffort::Low),
        "medium" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        _ => bail!(
            "unsupported reasoning effort: {} (allowed: minimal, low, medium, high)",
            value.trim()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_model_for_runner_rejects_glm47_on_codex() {
        let err = validate_model_for_runner(Runner::Codex, &Model::Glm47).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("zai-coding-plan/glm-4.7"));
    }

    #[test]
    fn validate_model_for_runner_rejects_custom_on_codex() {
        let model = Model::Custom("gemini-3-pro-preview".to_string());
        let err = validate_model_for_runner(Runner::Codex, &model).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("gemini-3-pro-preview"));
        assert!(msg.contains("gpt-5.2-codex"));
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_gemini() {
        let model = resolve_model_for_runner(Runner::Gemini, None, None, None);
        assert_eq!(model.as_str(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_replaces_codex_default_for_gemini() {
        let model = resolve_model_for_runner(Runner::Gemini, None, None, Some(Model::Gpt52Codex));
        assert_eq!(model.as_str(), DEFAULT_GEMINI_MODEL);
    }

    #[test]
    fn resolve_model_for_runner_defaults_for_claude() {
        let model = resolve_model_for_runner(Runner::Claude, None, None, None);
        assert_eq!(model.as_str(), DEFAULT_CLAUDE_MODEL);
    }

    #[test]
    fn runner_error_nonzero_exit_redacts_output() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "out: API_KEY=secret123".into(),
            stderr: "err: bearer abc123def456".into(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("API_KEY=[REDACTED]"));
        assert!(msg.contains("bearer [REDACTED]"));
        assert!(!msg.contains("secret123"));
        assert!(!msg.contains("abc123def456"));
    }

    #[test]
    fn runner_output_display_redacts_output() {
        let output = RunnerOutput {
            status: ExitStatus::default(), // success usually
            stdout: "out: API_KEY=secret123".to_string(),
            stderr: "err: bearer abc123def456".to_string(),
        };
        let msg = format!("{}", output);
        assert!(msg.contains("API_KEY=[REDACTED]"));
        assert!(msg.contains("bearer [REDACTED]"));
        assert!(!msg.contains("secret123"));
        assert!(!msg.contains("abc123def456"));
    }
}
