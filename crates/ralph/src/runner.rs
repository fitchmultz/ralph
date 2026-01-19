use crate::contracts::{AgentConfig, Model, ReasoningEffort, Runner, TaskAgent};
use anyhow::{anyhow, bail, Context, Result};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const OPENCODE_PROMPT_FILE_MESSAGE: &str = "Follow the attached prompt file verbatim.";
const GEMINI_PROMPT_PREFIX: &str =
    "If RepoPrompt tools are available, you MUST use them for file search, reading, and edits (do not bypass them).";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3-flash-preview";

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

impl RunnerOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn combined(&self) -> String {
        if self.stdout.is_empty() {
            return self.stderr.clone();
        }
        if self.stderr.is_empty() {
            return self.stdout.clone();
        }
        format!("{}{}", self.stdout, self.stderr)
    }
}

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
}

pub fn default_model_for_runner(runner: Runner) -> Model {
    match runner {
        Runner::Codex => Model::Gpt52Codex,
        Runner::Opencode => Model::Glm47,
        Runner::Gemini => Model::Custom(DEFAULT_GEMINI_MODEL.to_string()),
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

pub fn run_prompt(
    runner: Runner,
    work_dir: &Path,
    bins: RunnerBinaries<'_>,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
) -> Result<RunnerOutput> {
    validate_model_for_runner(runner, &model)?;
    let prepared_prompt = prepare_prompt(runner, prompt);
    match runner {
        Runner::Codex => run_codex(
            work_dir,
            bins.codex,
            model,
            reasoning_effort,
            &prepared_prompt,
        ),
        Runner::Opencode => run_opencode(work_dir, bins.opencode, model, &prepared_prompt),
        Runner::Gemini => run_gemini(work_dir, bins.gemini, model, &prepared_prompt),
    }
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
) -> Result<RunnerOutput> {
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
    run_with_streaming(cmd, Some(prompt.as_bytes()), "codex")
}

fn run_opencode(work_dir: &Path, bin: &str, model: Model, prompt: &str) -> Result<RunnerOutput> {
    let mut tmp = tempfile::Builder::new()
        .prefix("ralph_prompt_")
        .suffix(".md")
        .tempfile()
        .context("create temp prompt file")?;

    tmp.write_all(prompt.as_bytes())
        .context("write prompt file")?;
    tmp.flush().context("flush prompt file")?;

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

    run_with_streaming(cmd, None, bin)
}

fn run_gemini(work_dir: &Path, bin: &str, model: Model, prompt: &str) -> Result<RunnerOutput> {
    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir);
    ensure_self_on_path(&mut cmd);
    cmd.arg("--model")
        .arg(model.as_str())
        .arg("--approval-mode")
        .arg("yolo");
    run_with_streaming(cmd, Some(prompt.as_bytes()), bin)
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
) -> Result<RunnerOutput> {
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

    let mut child = cmd.spawn().with_context(|| format!("spawn {}", bin))?;

    if let Some(payload) = stdin_payload {
        let stdin = child.stdin.as_mut().context("open stdin for child")?;
        stdin.write_all(payload).context("write prompt to stdin")?;
    }

    drop(child.stdin.take());

    #[cfg(unix)]
    {
        let mut guard = ctrlc
            .active_pgid
            .lock()
            .map_err(|_| anyhow::anyhow!("lock ctrl-c state"))?;
        let pid = child.id() as i32;
        *guard = Some(pid);
    }

    let stdout = child.stdout.take().context("capture child stdout")?;
    let stderr = child.stderr.take().context("capture child stderr")?;

    let stdout_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf = Arc::new(Mutex::new(String::new()));

    let stdout_handle = spawn_reader(stdout, StreamSink::Stdout, Arc::clone(&stdout_buf));
    let stderr_handle = spawn_reader(stderr, StreamSink::Stderr, Arc::clone(&stderr_buf));

    let status = wait_for_child(&mut child, ctrlc)?;

    #[cfg(unix)]
    {
        let mut guard = ctrlc
            .active_pgid
            .lock()
            .map_err(|_| anyhow::anyhow!("lock ctrl-c state"))?;
        *guard = None;
    }

    stdout_handle
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
    stderr_handle
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;

    let stdout = {
        let mut guard = stdout_buf
            .lock()
            .map_err(|_| anyhow::anyhow!("lock stdout buffer"))?;
        std::mem::take(&mut *guard)
    };
    let stderr = {
        let mut guard = stderr_buf
            .lock()
            .map_err(|_| anyhow::anyhow!("lock stderr buffer"))?;
        std::mem::take(&mut *guard)
    };

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
) -> thread::JoinHandle<Result<()>> {
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

fn wait_for_child(child: &mut std::process::Child, ctrlc: &CtrlCState) -> Result<ExitStatus> {
    let mut interrupt_sent = false;
    let mut kill_sent = false;
    let start = Instant::now();

    loop {
        if ctrlc.interrupted.load(Ordering::SeqCst) && !interrupt_sent {
            interrupt_sent = true;
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

        if interrupt_sent && !kill_sent && start.elapsed() > Duration::from_secs(2) {
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

        if let Some(status) = child.try_wait().context("poll child status")? {
            return Ok(status);
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
}
