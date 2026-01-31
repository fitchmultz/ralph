//! Runner-specific command assembly for execution.
//!
//! Responsibilities:
//! - Assemble runner-specific commands and payloads for execution.
//! - Delegate execution to the shared streaming process runner.
//!
//! Does not handle:
//! - Validating models or global runner configuration.
//! - Persisting runner output or mutating queue state.
//!
//! Assumptions/invariants:
//! - Caller supplies validated model/runner inputs and a writable work dir.
//! - Command builder guards are kept alive for the duration of execution.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value as JsonValue;

use super::super::{
    runner_execution_error, runner_execution_error_with_source, ClaudePermissionMode, Model,
    OutputHandler, OutputStream, ReasoningEffort, RunnerError, RunnerOutput,
};
use super::cli_options::ResolvedRunnerCliOptions;
use super::cli_spec;
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;
use crate::commands::run::PhaseType;
use crate::contracts::Runner;

type RunnerCommandParts = (
    std::process::Command,
    Option<Vec<u8>>,
    Vec<Box<dyn std::any::Any + Send + Sync>>,
);

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_codex(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_codex_global_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("exec")
        .legacy_json_format()
        .model(&model)
        .reasoning_effort(reasoning_effort)
        .arg("-")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Codex,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_codex_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    thread_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_codex_global_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("exec")
        .arg("resume")
        .arg(thread_id)
        .legacy_json_format()
        .model(&model)
        .reasoning_effort(reasoning_effort)
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Codex,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_opencode(
    work_dir: &Path,
    bin: &str,
    _runner_cli: ResolvedRunnerCliOptions,
    model: &Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("run")
        .model(model)
        .opencode_format()
        .with_temp_prompt_file(prompt)
        .map_err(|err| {
            runner_execution_error_with_source(
                Runner::Opencode,
                bin,
                "create temp prompt file",
                err,
            )
        })?
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Opencode,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_opencode_resume(
    work_dir: &Path,
    bin: &str,
    _runner_cli: ResolvedRunnerCliOptions,
    model: &Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("run")
        .arg("-s")
        .arg(session_id)
        .model(model)
        .opencode_format()
        .arg("--")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Opencode,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_gemini(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_gemini_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .model(&model)
        .output_format("stream-json")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Gemini,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_gemini_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_gemini_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .output_format("stream-json")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Gemini,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_claude(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_claude_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--verbose")
        .arg("-p")
        .model(&model)
        .permission_mode(permission_mode)
        .output_format("stream-json")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Claude,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_claude_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_claude_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--resume")
        .arg(session_id)
        .arg("--verbose")
        .model(&model)
        .permission_mode(permission_mode)
        .output_format("stream-json")
        .arg("-p")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Claude,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_kimi(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) =
        build_kimi_command(work_dir, bin, runner_cli, model, prompt, None);

    let mut output = run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Kimi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )?;

    if output
        .session_id
        .as_deref()
        .map(|id| id.starts_with("tool_"))
        .unwrap_or(true)
    {
        if let Some(session_id) = resolve_kimi_session_id(work_dir) {
            output.session_id = Some(session_id);
        }
    }

    Ok(output)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_kimi_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    _session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) =
        build_kimi_continue_command(work_dir, bin, runner_cli, model, message);

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Kimi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_pi(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = build_pi_command(work_dir, bin, runner_cli, model, prompt, None)?;

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Pi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_pi_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let session_path = resolve_pi_session_path(work_dir, session_id)?;
    let (cmd, payload, _guards) = build_pi_command(
        work_dir,
        bin,
        runner_cli,
        model,
        message,
        Some(&session_path),
    )?;

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Pi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_cursor(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    phase_type: PhaseType,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir).model(&model);
    let builder = cli_spec::apply_cursor_options(builder, runner_cli, phase_type);

    let (cmd, payload, _guards) = builder
        .arg("--print")
        .output_format("stream-json")
        // Cursor agent CLI expects the prompt as a positional argument.
        .arg(prompt)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Cursor,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_cursor_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    phase_type: PhaseType,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir)
        .arg("--resume")
        .arg(session_id)
        .model(&model);
    let builder = cli_spec::apply_cursor_options(builder, runner_cli, phase_type);

    let (cmd, payload, _guards) = builder
        .arg("--print")
        .output_format("stream-json")
        // Cursor agent CLI expects the continuation message as a positional argument.
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Cursor,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

fn build_kimi_command(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    session_id: Option<&str>,
) -> RunnerCommandParts {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_kimi_options(builder, runner_cli);
    let builder = if let Some(session_id) = session_id {
        builder.arg("--session").arg(session_id)
    } else {
        builder
    };
    builder
        .model(&model)
        .arg("--print")
        .arg("--prompt")
        .arg(prompt)
        .output_format("stream-json")
        .build()
}

fn build_kimi_continue_command(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
) -> RunnerCommandParts {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_kimi_options(builder, runner_cli);
    builder
        .arg("--continue")
        .model(&model)
        .arg("--print")
        .arg("--prompt")
        .arg(prompt)
        .output_format("stream-json")
        .build()
}

fn build_pi_command(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    session_path: Option<&Path>,
) -> Result<RunnerCommandParts, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_pi_options(builder, runner_cli);
    let builder = if let Some(path) = session_path {
        builder
            .arg("--session")
            .arg(path.to_string_lossy().as_ref())
    } else {
        builder
    };

    Ok(builder
        .model(&model)
        .arg("--mode")
        .arg("json")
        .arg(prompt)
        .build())
}

fn resolve_pi_session_path(
    work_dir: &Path,
    session_id: &str,
) -> Result<std::path::PathBuf, RunnerError> {
    let direct = std::path::Path::new(session_id);
    if direct.is_file() {
        return Ok(direct.to_path_buf());
    }

    let base = pi_agent_root().ok_or_else(|| {
        runner_execution_error(
            Runner::Pi,
            "pi",
            "resolve PI_CODING_AGENT_DIR or HOME for session lookup",
        )
    })?;
    let sessions_dir = base.join("sessions");
    let workspace_dir = sessions_dir.join(pi_session_dir_name(work_dir));
    let suffix = format!("_{session_id}.jsonl");

    let entries = std::fs::read_dir(&workspace_dir).map_err(|err| {
        runner_execution_error_with_source(
            Runner::Pi,
            "pi",
            &format!("read pi session dir {}", workspace_dir.display()),
            err,
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|err| {
            runner_execution_error_with_source(Runner::Pi, "pi", "read pi session entry", err)
        })?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(&suffix))
            .unwrap_or(false)
        {
            return Ok(path);
        }
    }

    Err(runner_execution_error(
        Runner::Pi,
        "pi",
        &format!("pi session file not found for id {session_id}"),
    ))
}

fn pi_agent_root() -> Option<std::path::PathBuf> {
    if let Some(value) = std::env::var_os("PI_CODING_AGENT_DIR") {
        return Some(std::path::PathBuf::from(value));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".pi").join("agent"))
}

fn pi_session_dir_name(work_dir: &Path) -> String {
    let mut path = work_dir.to_string_lossy().to_string();
    if let Some(stripped) = path.strip_prefix("\\\\?\\") {
        path = stripped.to_string();
    }
    let trimmed = path.trim_start_matches(['/', '\\']);
    let normalized = trimmed.replace(['/', '\\'], "-");
    format!("--{}--", normalized)
}

fn resolve_kimi_session_id(work_dir: &Path) -> Option<String> {
    let config_path = kimi_config_path()?;
    resolve_kimi_session_id_from_file(work_dir, &config_path)
}

fn resolve_kimi_session_id_from_file(work_dir: &Path, path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let json: JsonValue = serde_json::from_str(&contents).ok()?;
    let work_dirs = json.get("work_dirs")?.as_array()?;
    let work_dir = work_dir.to_string_lossy();

    for entry in work_dirs {
        let entry_path = entry.get("path").and_then(|v| v.as_str())?;
        if entry_path == work_dir {
            return entry
                .get("last_session_id")
                .and_then(|v| v.as_str())
                .map(|value| value.to_string());
        }
    }

    None
}

fn kimi_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".kimi").join("kimi.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn build_kimi_command_includes_prompt_and_optional_session() {
        let opts = ResolvedRunnerCliOptions::default();
        let (cmd, payload, _guards) = build_kimi_command(
            Path::new("."),
            "kimi",
            opts,
            Model::Glm47,
            "hello",
            Some("sess-123"),
        );
        let args = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--prompt".to_string()));
        assert!(args.contains(&"hello".to_string()));
        assert!(args.contains(&"--session".to_string()));
        assert!(args.contains(&"sess-123".to_string()));
        assert!(payload.is_none());
    }

    #[test]
    fn build_pi_command_uses_mode_json_and_prompt() {
        let opts = ResolvedRunnerCliOptions::default();
        let (cmd, payload, _guards) =
            build_pi_command(Path::new("."), "pi", opts, Model::Glm47, "hello", None)
                .expect("pi command build");
        let args = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.contains(&"--mode".to_string()));
        assert!(args.contains(&"json".to_string()));
        assert!(args.contains(&"hello".to_string()));
        assert!(payload.is_none());
    }

    #[test]
    fn build_kimi_continue_command_includes_continue_flag() {
        let opts = ResolvedRunnerCliOptions::default();
        let (cmd, payload, _guards) =
            build_kimi_continue_command(Path::new("."), "kimi", opts, Model::Glm47, "hello");
        let args = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.contains(&"--continue".to_string()));
        assert!(args.contains(&"--prompt".to_string()));
        assert!(args.contains(&"hello".to_string()));
        assert!(payload.is_none());
    }

    #[test]
    fn resolve_pi_session_path_finds_matching_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let sessions_dir = temp_dir.path().join("sessions");
        let workspace_dir = sessions_dir.join(pi_session_dir_name(Path::new("/tmp/project")));
        fs::create_dir_all(&workspace_dir).expect("create sessions dir");

        let session_id = "abc-123";
        let session_file = workspace_dir.join(format!("2026-01-01T00-00-00Z_{session_id}.jsonl"));
        fs::write(&session_file, "{}").expect("write session file");

        std::env::set_var("PI_CODING_AGENT_DIR", temp_dir.path());
        let resolved = resolve_pi_session_path(Path::new("/tmp/project"), session_id)
            .expect("resolve session path");
        assert_eq!(resolved, session_file);
        std::env::remove_var("PI_CODING_AGENT_DIR");
    }

    #[test]
    fn pi_session_dir_name_normalizes_path() {
        let name = pi_session_dir_name(Path::new("/Users/mitchfultz/Projects/AI/ralph"));
        assert_eq!(name, "--Users-mitchfultz-Projects-AI-ralph--");
    }

    #[test]
    fn resolve_kimi_session_id_from_file_matches_path() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("kimi.json");
        let contents = r#"
{
  "work_dirs": [
    {"path": "/tmp/alpha", "last_session_id": "sess-alpha"},
    {"path": "/tmp/beta", "last_session_id": "sess-beta"}
  ]
}
"#;
        fs::write(&config_path, contents).expect("write kimi.json");

        let resolved = resolve_kimi_session_id_from_file(Path::new("/tmp/beta"), &config_path)
            .expect("session id");
        assert_eq!(resolved, "sess-beta");
    }

    #[test]
    fn resolve_kimi_session_id_from_file_skips_null_session() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_path = temp_dir.path().join("kimi.json");
        let contents = r#"
{
  "work_dirs": [
    {"path": "/tmp/alpha", "last_session_id": null}
  ]
}
"#;
        fs::write(&config_path, contents).expect("write kimi.json");

        assert_eq!(
            resolve_kimi_session_id_from_file(Path::new("/tmp/alpha"), &config_path),
            None
        );
    }
}
