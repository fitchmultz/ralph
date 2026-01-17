use crate::contracts::{Model, ReasoningEffort, Runner};
use anyhow::{bail, Context, Result};
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

pub fn validate_model_for_runner(runner: Runner, model: Model) -> Result<()> {
    if runner == Runner::Codex && model == Model::Glm47 {
        bail!("model glm-4.7 is not supported for codex runner");
    }
    Ok(())
}

pub fn run_prompt(
    runner: Runner,
    work_dir: &Path,
    codex_bin: &str,
    opencode_bin: &str,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
) -> Result<RunnerOutput> {
    validate_model_for_runner(runner, model)?;
    match runner {
        Runner::Codex => run_codex(work_dir, codex_bin, model, reasoning_effort, prompt),
        Runner::Opencode => run_opencode(work_dir, opencode_bin, model, prompt),
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
        .arg(model_as_str(model));

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
        .arg(model_as_str(model))
        .arg("--file")
        .arg(tmp.path())
        .arg("--")
        .arg(OPENCODE_PROMPT_FILE_MESSAGE)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    run_with_streaming(cmd, None, bin)
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

fn model_as_str(model: Model) -> &'static str {
    match model {
        Model::Gpt52Codex => "gpt-5.2-codex",
        Model::Gpt52 => "gpt-5.2",
        Model::Glm47 => "glm-4.7",
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
    match trimmed {
        "gpt-5.2-codex" => Ok(Model::Gpt52Codex),
        "gpt-5.2" => Ok(Model::Gpt52),
        "glm-4.7" => Ok(Model::Glm47),
        _ => bail!(
            "unsupported model: {} (allowed: gpt-5.2-codex, gpt-5.2, glm-4.7)",
            trimmed
        ),
    }
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
        let err = validate_model_for_runner(Runner::Codex, Model::Glm47).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("glm-4.7"));
    }
}
