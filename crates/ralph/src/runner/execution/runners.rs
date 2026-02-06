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

use std::path::Path;
use std::time::Duration;

use super::super::{
    Model, OutputHandler, OutputStream, RunnerError, RunnerOutput, runner_execution_error,
    runner_execution_error_with_source,
};
use super::cli_options::ResolvedRunnerCliOptions;
use super::cli_spec;
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;
use crate::commands::run::PhaseType;
use crate::constants::paths::{ENV_MODEL_USED, ENV_RUNNER_USED};
use crate::contracts::Runner;

type RunnerCommandParts = (
    std::process::Command,
    Option<Vec<u8>>,
    Vec<Box<dyn std::any::Any + Send + Sync>>,
);

/// Apply analytics environment variables to track which runner/model was actually used.
fn apply_analytics_env(
    builder: RunnerCommandBuilder,
    runner: &Runner,
    model: &Model,
) -> RunnerCommandBuilder {
    builder
        .env(ENV_RUNNER_USED, runner.id())
        .env(ENV_MODEL_USED, model.as_str())
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
    let (cmd, payload, _guards) = apply_analytics_env(
        RunnerCommandBuilder::new(bin, work_dir),
        &Runner::Opencode,
        model,
    )
    .arg("run")
    .model(model)
    .opencode_format()
    .with_temp_prompt_file(prompt)
    .map_err(|err| {
        runner_execution_error_with_source(&Runner::Opencode, bin, "create temp prompt file", err)
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
    let (cmd, payload, _guards) = apply_analytics_env(
        RunnerCommandBuilder::new(bin, work_dir),
        &Runner::Opencode,
        model,
    )
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
    let builder = apply_analytics_env(builder, &Runner::Gemini, &model);
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
    let builder = apply_analytics_env(builder, &Runner::Gemini, &model);
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
    let builder = apply_analytics_env(builder, &Runner::Cursor, &model);
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
    let builder = apply_analytics_env(builder, &Runner::Cursor, &model);
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

fn build_pi_command(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    session_path: Option<&Path>,
) -> Result<RunnerCommandParts, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = apply_analytics_env(builder, &Runner::Pi, &model);
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
            &Runner::Pi,
            "pi",
            "resolve PI_CODING_AGENT_DIR or HOME for session lookup",
        )
    })?;
    let sessions_dir = base.join("sessions");
    let workspace_dir = sessions_dir.join(pi_session_dir_name(work_dir));
    let suffix = format!("_{session_id}.jsonl");

    let entries = std::fs::read_dir(&workspace_dir).map_err(|err| {
        runner_execution_error_with_source(
            &Runner::Pi,
            "pi",
            &format!("read pi session dir {}", workspace_dir.display()),
            err,
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|err| {
            runner_execution_error_with_source(&Runner::Pi, "pi", "read pi session entry", err)
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
        &Runner::Pi,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

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
    fn resolve_pi_session_path_finds_matching_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let sessions_dir = temp_dir.path().join("sessions");
        let workspace_dir = sessions_dir.join(pi_session_dir_name(Path::new("/tmp/project")));
        fs::create_dir_all(&workspace_dir).expect("create sessions dir");

        let session_id = "abc-123";
        let session_file = workspace_dir.join(format!("2026-01-01T00-00-00Z_{session_id}.jsonl"));
        fs::write(&session_file, "{}").expect("write session file");

        unsafe { std::env::set_var("PI_CODING_AGENT_DIR", temp_dir.path()) };
        let resolved = resolve_pi_session_path(Path::new("/tmp/project"), session_id)
            .expect("resolve session path");
        assert_eq!(resolved, session_file);
        unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };
    }

    #[test]
    fn pi_session_dir_name_normalizes_path() {
        let name = pi_session_dir_name(Path::new("/Users/mitchfultz/Projects/AI/ralph"));
        assert_eq!(name, "--Users-mitchfultz-Projects-AI-ralph--");
    }
}
