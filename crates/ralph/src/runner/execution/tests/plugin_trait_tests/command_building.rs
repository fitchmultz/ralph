//! Plugin command-building regression coverage.
//!
//! Purpose:
//! - Plugin command-building regression coverage.
//!
//! Responsibilities:
//! - Verify runner-specific run/resume argv construction for built-in plugins.
//! - Lock down approval, sandbox, session, and phase-aware command flags.
//!
//! Non-scope:
//! - Response parsing or executor metadata behavior.
//! - Subprocess integration beyond command assembly.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Tests reuse parent helper contexts to model CLI defaults.
//! - Command assertions focus on stable argv semantics instead of arg ordering beyond required flags.

use super::*;
use std::{ffi::OsString, io::Write as _, sync::Mutex};

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarRestore {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarRestore {
    fn capture(key: &'static str) -> Self {
        Self {
            key,
            original: std::env::var_os(key),
        }
    }
}

impl Drop for EnvVarRestore {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

// =============================================================================
// Command Building Tests - Codex
// =============================================================================

#[test]
fn codex_build_run_command_basic() {
    let plugin = BuiltInRunnerPlugin::Codex;
    let ctx = create_run_context("test prompt", None);

    let (cmd, payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        args.contains(&"exec".to_string()),
        "Codex should use exec subcommand"
    );
    assert!(
        args.contains(&"--json".to_string()),
        "Codex should use --json flag"
    );
    assert!(
        args.contains(&"-".to_string()),
        "Codex should read from stdin"
    );
    assert!(payload.is_some(), "Codex should have stdin payload");
}

#[test]
fn codex_build_resume_command_includes_thread_id() {
    let plugin = BuiltInRunnerPlugin::Codex;
    let ctx = create_resume_context("thread-123", "continue please");

    let (cmd, _payload, _guards) = plugin.build_resume_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"exec".to_string()));
    assert!(args.contains(&"resume".to_string()));
    assert!(args.contains(&"thread-123".to_string()));
    assert!(args.contains(&"continue please".to_string()));
}

#[test]
fn codex_build_run_command_with_sandbox_disabled() {
    let plugin = BuiltInRunnerPlugin::Codex;
    let mut ctx = create_run_context("test prompt", None);
    ctx.runner_cli.sandbox = RunnerSandboxMode::Disabled;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()),
        "Codex should bypass sandbox when disabled"
    );
}

#[test]
fn codex_build_run_command_with_sandbox_enabled() {
    let plugin = BuiltInRunnerPlugin::Codex;
    let mut ctx = create_run_context("test prompt", None);
    ctx.runner_cli.sandbox = RunnerSandboxMode::Enabled;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--sandbox".to_string()));
    assert!(args.contains(&"workspace-write".to_string()));
}

// =============================================================================
// Command Building Tests - Kimi
// =============================================================================

#[test]
fn kimi_build_run_command_includes_session_id() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let ctx = create_run_context("test prompt", Some("sess-123"));

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--session".to_string()));
    assert!(args.contains(&"sess-123".to_string()));
    assert!(args.contains(&"--print".to_string()));
    assert!(args.contains(&"--prompt".to_string()));
    assert!(args.contains(&"test prompt".to_string()));
}

#[test]
fn kimi_build_run_command_without_session() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let ctx = create_run_context("test prompt", None);

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        !args.contains(&"--session".to_string()),
        "Kimi should not include --session when no session_id provided"
    );
}

#[test]
fn kimi_build_run_command_with_yolo_mode() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let mut ctx = create_run_context("test prompt", None);
    ctx.runner_cli.approval_mode = RunnerApprovalMode::Yolo;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        args.contains(&"--yolo".to_string()),
        "Kimi should use --yolo flag for yolo mode"
    );
}

// =============================================================================
// Command Building Tests - Claude
// =============================================================================

#[test]
fn claude_build_run_command_basic() {
    let plugin = BuiltInRunnerPlugin::Claude;
    let ctx = create_run_context("test prompt", None);

    let (cmd, payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--verbose".to_string()));
    assert!(args.contains(&"-p".to_string()));
    assert!(payload.is_some());
}

#[test]
fn claude_build_resume_command_includes_session() {
    let plugin = BuiltInRunnerPlugin::Claude;
    let ctx = create_resume_context("sess-abc", "continue");

    let (cmd, _payload, _guards) = plugin.build_resume_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--resume".to_string()));
    assert!(args.contains(&"sess-abc".to_string()));
    assert!(args.contains(&"continue".to_string()));
}

// =============================================================================
// Command Building Tests - Gemini
// =============================================================================

#[test]
fn gemini_build_run_command_with_approval_mode() {
    let plugin = BuiltInRunnerPlugin::Gemini;
    let mut ctx = create_run_context("test prompt", None);
    ctx.runner_cli.approval_mode = RunnerApprovalMode::Yolo;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--approval-mode".to_string()));
    assert!(args.contains(&"yolo".to_string()));
}

#[test]
fn gemini_build_resume_command_includes_resume_flag() {
    let plugin = BuiltInRunnerPlugin::Gemini;
    let ctx = create_resume_context("sess-gem", "continue");

    let (cmd, _payload, _guards) = plugin.build_resume_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--resume".to_string()));
    assert!(args.contains(&"sess-gem".to_string()));
}

// =============================================================================
// Command Building Tests - Cursor
// =============================================================================

#[test]
fn cursor_build_run_command_phase_aware_defaults() {
    let plugin = BuiltInRunnerPlugin::Cursor;
    let mut ctx = create_run_context("test prompt", None);
    ctx.phase_type = Some(PhaseType::Planning);

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--sandbox".to_string()));
    // In planning phase, sandbox defaults to "enabled"
    assert!(args.contains(&"enabled".to_string()));
    assert!(args.contains(&"--plan".to_string()));
}

#[test]
fn cursor_build_run_command_implementation_phase() {
    let plugin = BuiltInRunnerPlugin::Cursor;
    let mut ctx = create_run_context("test prompt", None);
    ctx.phase_type = Some(PhaseType::Implementation);
    ctx.runner_cli.approval_mode = RunnerApprovalMode::Yolo;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        args.contains(&"--force".to_string()),
        "Yolo mode should add --force"
    );
    assert!(args.contains(&"--sandbox".to_string()));
    // In implementation phase, sandbox defaults to "disabled"
    assert!(args.contains(&"disabled".to_string()));
}

// =============================================================================
// Command Building Tests - Opencode
// =============================================================================

#[test]
fn opencode_build_run_command_uses_temp_file() {
    let plugin = BuiltInRunnerPlugin::Opencode;
    let ctx = create_run_context("test prompt content", None);

    let (cmd, _payload, guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"run".to_string()));
    assert!(args.contains(&"--format".to_string()));
    assert!(args.contains(&"json".to_string()));
    // Opencode should have temp file guards
    assert!(!guards.is_empty(), "Opencode should have temp file guards");
}

#[test]
fn opencode_build_resume_command_includes_session_flag() {
    let plugin = BuiltInRunnerPlugin::Opencode;
    let ctx = create_resume_context("sess-open", "continue");

    let (cmd, _payload, _guards) = plugin.build_resume_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"-s".to_string()));
    assert!(args.contains(&"sess-open".to_string()));
}

// =============================================================================
// Command Building Tests - Pi
// =============================================================================

#[test]
fn pi_build_run_command_basic() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let ctx = create_run_context("test prompt", None);

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--mode".to_string()));
    assert!(args.contains(&"json".to_string()));
    assert!(args.contains(&"test prompt".to_string()));
}

#[test]
fn pi_build_run_command_uses_process_title_wrapper() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let mut fake_pi = tempfile::Builder::new()
        .prefix("fake_pi_")
        .tempfile()
        .expect("create fake pi");
    writeln!(fake_pi, "#!/usr/bin/env node").expect("write shebang");
    let fake_pi_path = fake_pi.path().to_string_lossy().to_string();
    let mut ctx = create_run_context("test prompt", None);
    ctx.bin = &fake_pi_path;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    assert_eq!(cmd.get_program().to_string_lossy(), "node");
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let wrapper_path = args.first().expect("node wrapper path");
    assert!(
        wrapper_path.contains("ralph_pi_wrapper_"),
        "Pi should be launched through Ralph's process-title wrapper"
    );

    let pi_bin = cmd
        .get_envs()
        .find_map(|(key, value)| {
            if key == "RALPH_PI_BIN" {
                value.map(|value| value.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .expect("RALPH_PI_BIN env missing");
    assert_eq!(pi_bin, fake_pi_path);

    let pi_entrypoint = cmd
        .get_envs()
        .find_map(|(key, value)| {
            if key == "RALPH_PI_ENTRYPOINT" {
                value.map(|value| value.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .expect("RALPH_PI_ENTRYPOINT env missing");
    assert_eq!(
        pi_entrypoint,
        fake_pi.path().canonicalize().unwrap().to_string_lossy()
    );

    let wrapper_source = std::fs::read_to_string(wrapper_path).expect("read wrapper source");
    assert!(wrapper_source.contains("Object.defineProperty(process, \"title\""));
    assert!(
        wrapper_source.contains("await import(pathToFileURL(realpathSync(piEntrypoint)).href)")
    );
}

#[test]
fn pi_build_run_command_wraps_path_resolved_node_binary() {
    let _lock = PATH_ENV_LOCK.lock().expect("lock PATH mutation");
    let _restore = EnvVarRestore::capture("PATH");
    let plugin = BuiltInRunnerPlugin::Pi;
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let bin_dir = temp_dir.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("create bin dir");
    let fake_pi_path = bin_dir.join("pi");
    std::fs::write(&fake_pi_path, "#!/usr/bin/env node\n").expect("write fake pi");

    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(bin_dir.clone()).chain(std::env::split_paths(&current_path)),
    )
    .expect("join PATH");
    unsafe { std::env::set_var("PATH", path) };

    let mut ctx = create_run_context("test prompt", None);
    ctx.bin = "pi";

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    assert_eq!(cmd.get_program().to_string_lossy(), "node");
    let pi_bin = cmd
        .get_envs()
        .find_map(|(key, value)| {
            if key == "RALPH_PI_BIN" {
                value.map(|value| value.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .expect("RALPH_PI_BIN env missing");
    assert_eq!(pi_bin, "pi");

    let pi_entrypoint = cmd
        .get_envs()
        .find_map(|(key, value)| {
            if key == "RALPH_PI_ENTRYPOINT" {
                value.map(|value| value.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .expect("RALPH_PI_ENTRYPOINT env missing");
    assert_eq!(
        pi_entrypoint,
        fake_pi_path.canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn pi_build_run_command_preserves_direct_custom_binary_when_not_node_script() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let mut fake_pi = tempfile::Builder::new()
        .prefix("fake_pi_native_")
        .tempfile()
        .expect("create fake native pi");
    writeln!(fake_pi, "#!/bin/sh").expect("write shebang");
    let fake_pi_path = fake_pi.path().to_string_lossy().to_string();
    let mut ctx = create_run_context("test prompt", None);
    ctx.bin = &fake_pi_path;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    assert_eq!(cmd.get_program().to_string_lossy(), fake_pi_path);
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(!args.iter().any(|arg| arg.contains("ralph_pi_wrapper_")));
    assert!(args.contains(&"--mode".to_string()));
    assert!(args.contains(&"json".to_string()));
}

#[test]
fn pi_build_run_command_with_yolo_mode() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let mut ctx = create_run_context("test prompt", None);
    ctx.runner_cli.approval_mode = RunnerApprovalMode::Yolo;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        args.contains(&"--print".to_string()),
        "Pi should use --print for yolo mode"
    );
}

#[test]
fn pi_build_run_command_with_sandbox() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let mut ctx = create_run_context("test prompt", None);
    ctx.runner_cli.sandbox = RunnerSandboxMode::Enabled;

    let (cmd, _payload, _guards) = plugin.build_run_command(ctx).unwrap();

    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(args.contains(&"--sandbox".to_string()));
}

// =============================================================================
