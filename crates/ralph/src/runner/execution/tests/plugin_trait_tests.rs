//! Comprehensive tests for the plugin trait system.
//!
//! These tests validate:
//! - RunnerPlugin trait implementations for all 7 built-in runners
//! - ResponseParser trait implementations for all runners
//! - PluginExecutor dispatch logic
//! - Command building correctness
//! - Response extraction accuracy

#![allow(clippy::needless_borrows_for_generic_args)]

use std::path::Path;

use crate::commands::run::PhaseType;
use crate::contracts::{Model, Runner, RunnerApprovalMode, RunnerSandboxMode};
use crate::runner::execution::plugin_trait::{ResumeContext, RunContext, RunnerMetadata};
use crate::runner::{
    OutputStream, ResolvedRunnerCliOptions,
    execution::{BuiltInRunnerPlugin, PluginExecutor, RunnerPlugin},
};

// =============================================================================
// BuiltInRunnerPlugin Tests
// =============================================================================

#[test]
fn all_built_in_plugins_have_correct_runner_mapping() {
    assert_eq!(BuiltInRunnerPlugin::Codex.runner(), Runner::Codex);
    assert_eq!(BuiltInRunnerPlugin::Opencode.runner(), Runner::Opencode);
    assert_eq!(BuiltInRunnerPlugin::Gemini.runner(), Runner::Gemini);
    assert_eq!(BuiltInRunnerPlugin::Claude.runner(), Runner::Claude);
    assert_eq!(BuiltInRunnerPlugin::Kimi.runner(), Runner::Kimi);
    assert_eq!(BuiltInRunnerPlugin::Pi.runner(), Runner::Pi);
    assert_eq!(BuiltInRunnerPlugin::Cursor.runner(), Runner::Cursor);
}

#[test]
fn all_built_in_plugins_have_correct_id() {
    assert_eq!(BuiltInRunnerPlugin::Codex.id(), "codex");
    assert_eq!(BuiltInRunnerPlugin::Opencode.id(), "opencode");
    assert_eq!(BuiltInRunnerPlugin::Gemini.id(), "gemini");
    assert_eq!(BuiltInRunnerPlugin::Claude.id(), "claude");
    assert_eq!(BuiltInRunnerPlugin::Kimi.id(), "kimi");
    assert_eq!(BuiltInRunnerPlugin::Pi.id(), "pi");
    assert_eq!(BuiltInRunnerPlugin::Cursor.id(), "cursor");
}

#[test]
fn all_built_in_plugins_have_metadata() {
    let plugins: [BuiltInRunnerPlugin; 7] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Pi,
        BuiltInRunnerPlugin::Cursor,
    ];

    for plugin in &plugins {
        let metadata: RunnerMetadata = plugin.metadata();
        assert!(
            !metadata.id.is_empty(),
            "Plugin {:?} missing id",
            plugin.runner()
        );
        assert!(
            !metadata.name.is_empty(),
            "Plugin {:?} missing name",
            plugin.runner()
        );
        assert_eq!(metadata.id, plugin.id());
    }
}

#[test]
fn all_built_in_plugins_support_resume() {
    let plugins: [BuiltInRunnerPlugin; 7] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Pi,
        BuiltInRunnerPlugin::Cursor,
    ];

    for plugin in &plugins {
        let metadata: RunnerMetadata = plugin.metadata();
        assert!(
            metadata.supports_resume,
            "Plugin {:?} should support resume",
            plugin.runner()
        );
    }
}

#[test]
fn kimi_requires_managed_session_id() {
    assert!(BuiltInRunnerPlugin::Kimi.requires_managed_session_id());
}

#[test]
fn other_plugins_do_not_require_managed_session_id() {
    assert!(!BuiltInRunnerPlugin::Codex.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Opencode.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Gemini.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Claude.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Pi.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Cursor.requires_managed_session_id());
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
// Response Parsing Tests
// =============================================================================

#[test]
fn codex_response_parser_extracts_agent_message() {
    let plugin = BuiltInRunnerPlugin::Codex;
    let mut buffer = String::new();

    let line =
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Hello from Codex"}}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Codex".to_string()));
}

#[test]
fn kimi_response_parser_extracts_assistant_text() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let mut buffer = String::new();

    let line = r#"{"role":"assistant","content":[{"type":"text","text":"Hello from Kimi"}]}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Kimi".to_string()));
}

#[test]
fn kimi_response_parser_skips_non_assistant_role() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let mut buffer = String::new();

    let line = r#"{"role":"user","content":[{"type":"text","text":"User message"}]}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, None);
}

#[test]
fn claude_response_parser_extracts_assistant_message() {
    let plugin = BuiltInRunnerPlugin::Claude;
    let mut buffer = String::new();

    let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello from Claude"}]}}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Claude".to_string()));
}

#[test]
fn gemini_response_parser_extracts_message() {
    let plugin = BuiltInRunnerPlugin::Gemini;
    let mut buffer = String::new();

    let line = r#"{"type":"message","role":"assistant","content":"Hello from Gemini"}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Gemini".to_string()));
}

#[test]
fn opencode_response_parser_accumulates_streaming_text() {
    let plugin = BuiltInRunnerPlugin::Opencode;
    let mut buffer = String::new();

    let line1 = r#"{"type":"text","part":{"text":"Hello "}}"#;
    let line2 = r#"{"type":"text","part":{"text":"World"}}"#;

    let result1: Option<String> = plugin.parse_response_line(line1, &mut buffer);
    assert_eq!(result1, Some("Hello ".to_string()));

    let result2: Option<String> = plugin.parse_response_line(line2, &mut buffer);
    assert_eq!(result2, Some("Hello World".to_string()));
}

#[test]
fn cursor_response_parser_extracts_message_end() {
    let plugin = BuiltInRunnerPlugin::Cursor;
    let mut buffer = String::new();

    let line =
        r#"{"type":"message_end","message":{"role":"assistant","content":"Hello from Cursor"}}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Cursor".to_string()));
}

#[test]
fn pi_response_parser_extracts_result() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let mut buffer = String::new();

    let line = r#"{"type":"result","result":"Hello from Pi"}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Pi".to_string()));
}

#[test]
fn response_parsers_handle_invalid_json() {
    let plugins: [BuiltInRunnerPlugin; 7] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Cursor,
        BuiltInRunnerPlugin::Pi,
    ];

    for plugin in &plugins {
        let mut buffer = String::new();
        let result: Option<String> = plugin.parse_response_line("not valid json", &mut buffer);
        assert_eq!(
            result,
            None,
            "Plugin {:?} should return None for invalid JSON",
            plugin.runner()
        );
    }
}

#[test]
fn response_parsers_handle_empty_lines() {
    let plugins: [BuiltInRunnerPlugin; 3] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Claude,
    ];

    for plugin in &plugins {
        let mut buffer = String::new();
        let result: Option<String> = plugin.parse_response_line("", &mut buffer);
        assert_eq!(
            result,
            None,
            "Plugin {:?} should return None for empty lines",
            plugin.runner()
        );
    }
}

// =============================================================================
// PluginExecutor Tests
// =============================================================================

#[test]
fn plugin_executor_creates_with_all_built_ins() {
    let executor = PluginExecutor::new();

    for runner in [
        Runner::Codex,
        Runner::Opencode,
        Runner::Gemini,
        Runner::Claude,
        Runner::Kimi,
        Runner::Pi,
        Runner::Cursor,
    ] {
        let metadata: RunnerMetadata = executor.metadata(&runner);
        assert!(
            !metadata.id.is_empty(),
            "Runner {:?} should have metadata",
            runner
        );
    }
}

#[test]
fn plugin_executor_external_plugin_metadata() {
    let executor = PluginExecutor::new();
    let runner = Runner::Plugin("test.plugin".to_string());
    let metadata: RunnerMetadata = executor.metadata(&runner);

    assert_eq!(metadata.id, "test.plugin");
    assert!(
        metadata.supports_resume,
        "External plugins assume resume support"
    );
}

#[test]
fn plugin_executor_requires_managed_session_id() {
    let executor = PluginExecutor::new();

    assert!(executor.requires_managed_session_id(&Runner::Kimi));
    assert!(!executor.requires_managed_session_id(&Runner::Codex));
    assert!(!executor.requires_managed_session_id(&Runner::Claude));
    assert!(!executor.requires_managed_session_id(&Runner::Gemini));
    assert!(!executor.requires_managed_session_id(&Runner::Opencode));
    assert!(!executor.requires_managed_session_id(&Runner::Pi));
    assert!(!executor.requires_managed_session_id(&Runner::Cursor));
}

#[test]
fn plugin_executor_external_plugins_do_not_require_managed_session() {
    let executor = PluginExecutor::new();
    let runner = Runner::Plugin("external".to_string());

    assert!(
        !executor.requires_managed_session_id(&runner),
        "External plugins should manage their own session IDs"
    );
}

#[test]
fn plugin_executor_extract_final_response_codex() {
    let executor = PluginExecutor::new();
    let runner = Runner::Codex;

    let stdout =
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Final response"}}"#;
    let result = executor.extract_final_response(&runner, stdout);

    assert_eq!(result, Some("Final response".to_string()));
}

#[test]
fn plugin_executor_extract_final_response_kimi() {
    let executor = PluginExecutor::new();
    let runner = Runner::Kimi;

    let stdout = r#"{"role":"assistant","content":[{"type":"text","text":"Kimi response"}]}"#;
    let result = executor.extract_final_response(&runner, stdout);

    assert_eq!(result, Some("Kimi response".to_string()));
}

#[test]
fn plugin_executor_extract_final_response_multiline() {
    let executor = PluginExecutor::new();
    let runner = Runner::Claude;

    let stdout = r#"
{"type":"progress","message":"Processing..."}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"First part"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Second part"}]}}
"#;
    let result = executor.extract_final_response(&runner, stdout);

    // Should return the last assistant message
    assert_eq!(result, Some("Second part".to_string()));
}

// =============================================================================
// Helper Functions
// =============================================================================

fn create_run_context<'a>(prompt: &'a str, session_id: Option<&'a str>) -> RunContext<'a> {
    RunContext {
        work_dir: Path::new("."),
        bin: "test-runner",
        model: Model::Gpt53,
        prompt,
        timeout: None,
        output_handler: None,
        output_stream: OutputStream::HandlerOnly,
        runner_cli: ResolvedRunnerCliOptions::default(),
        reasoning_effort: None,
        permission_mode: None,
        phase_type: None,
        session_id: session_id.map(|s| s.to_string()),
    }
}

fn create_resume_context<'a>(session_id: &'a str, message: &'a str) -> ResumeContext<'a> {
    ResumeContext {
        work_dir: Path::new("."),
        bin: "test-runner",
        model: Model::Gpt53,
        session_id,
        message,
        timeout: None,
        output_handler: None,
        output_stream: OutputStream::HandlerOnly,
        runner_cli: ResolvedRunnerCliOptions::default(),
        reasoning_effort: None,
        permission_mode: None,
        phase_type: None,
    }
}
