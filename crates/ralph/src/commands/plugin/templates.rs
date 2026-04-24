//! Plugin scaffold script templates.
//!
//! Purpose:
//! - Plugin scaffold script templates.
//!
//! Responsibilities:
//! - Hold the shell-script templates emitted by `ralph plugin init`.
//! - Keep large static template bodies out of command orchestration modules.
//!
//! Not handled here:
//! - Template interpolation or file writing.
//! - Plugin manifest construction.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Templates remain executable shell scripts with `-h/--help` support.
//! - `{plugin_id}` placeholders are replaced before the scripts are written.

pub(super) const RUNNER_SCRIPT_TEMPLATE: &str = r#"#!/bin/bash
# Runner stub for {plugin_id}
#
# Responsibilities:
# - Execute AI agent runs and resumes with prompt input from stdin.
# - Output newline-delimited JSON with text, tool_call, and finish types.
#
# Not handled here:
# - Task planning (handled by Ralph before invocation).
# - File operations outside the working directory.
#
# Assumptions:
# - stdin contains the compiled prompt.
# - Environment RALPH_PLUGIN_CONFIG_JSON contains plugin config.
# - Environment RALPH_RUNNER_CLI_JSON contains CLI options.

set -euo pipefail

PLUGIN_ID="{plugin_id}"

show_help() {
    cat << 'EOF'
Usage: runner.sh <COMMAND> [OPTIONS]

Commands:
  run       Execute a new run
  resume    Resume an existing session
  help      Show this help message

Run Options:
  --model <MODEL>             Model identifier
  --output-format <FORMAT>    Output format (must be stream-json)
  --session <ID>              Session identifier

Resume Options:
  --session <ID>              Session to resume (required)
  --model <MODEL>             Model identifier
  --output-format <FORMAT>    Output format (must be stream-json)
  <MESSAGE>                   Additional message argument

Examples:
  runner.sh run --model gpt-4 --output-format stream-json
  runner.sh resume --session abc123 --model gpt-4 --output-format stream-json "continue"
  runner.sh help

Protocol:
  Input: Prompt text via stdin
  Output: Newline-delimited JSON objects:
    {"type": "text", "content": "Hello"}
    {"type": "tool_call", "name": "write", "arguments": {"path": "file.txt"}}
    {"type": "finish", "session_id": "..."}
EOF
}

COMMAND="${1:-}"

case "$COMMAND" in
    run)
        # Stub: replace with your runner's execution logic.
        # Input prompt is provided via stdin; output must be NDJSON on stdout.
        _PROMPT=$(cat || true)
        echo "{\"type\": \"text\", \"content\": \"Stub runner: run not implemented\"}"
        echo "{\"type\": \"finish\", \"session_id\": \"stub-session\"}"
        echo "Stub runner ($PLUGIN_ID): run not implemented" >&2
        exit 1
        ;;
    resume)
        # Stub: replace with your runner's resume logic.
        echo "{\"type\": \"text\", \"content\": \"Stub runner: resume not implemented\"}"
        echo "{\"type\": \"finish\", \"session_id\": \"stub-session\"}"
        echo "Stub runner ($PLUGIN_ID): resume not implemented" >&2
        exit 1
        ;;
    help|--help|-h)
        show_help
        exit 0
        ;;
    "")
        echo "Error: No command specified" >&2
        show_help >&2
        exit 1
        ;;
    *)
        echo "Error: Unknown command: $COMMAND" >&2
        show_help >&2
        exit 1
        ;;
esac
"#;

pub(super) const PROCESSOR_SCRIPT_TEMPLATE: &str = r#"#!/bin/bash
# Processor stub for {plugin_id}
#
# Responsibilities:
# - Process task lifecycle hooks: validate_task, pre_prompt, post_run.
# - Called by Ralph with hook name and task ID as arguments.
#
# Not handled here:
# - Direct task execution (handled by runners).
# - Queue modification (handled by Ralph).
#
# Assumptions:
# - First argument is the hook name.
# - Second argument is the task ID.
# - Additional arguments may follow depending on hook.

set -euo pipefail

PLUGIN_ID="{plugin_id}"

show_help() {
    cat << 'EOF'
Usage: processor.sh <HOOK> <TASK_ID> [ARGS...]

Hooks:
  validate_task    Validate task structure before execution
                   Args: <TASK_ID> <TASK_JSON_FILE>
  pre_prompt       Called before prompt is sent to runner
                   Args: <TASK_ID> <PROMPT_FILE>
  post_run         Called after runner execution completes
                   Args: <TASK_ID> <OUTPUT_FILE>

Examples:
  processor.sh validate_task RQ-0001 /tmp/task.json
  processor.sh pre_prompt RQ-0001 /tmp/prompt.txt
  processor.sh post_run RQ-0001 /tmp/output.ndjson
  processor.sh help

Exit Codes:
  0    Success
  1    Validation/processing error

Environment:
  RALPH_PLUGIN_CONFIG_JSON    Plugin configuration as JSON string
EOF
}

HOOK="${1:-}"
TASK_ID="${2:-}"

# Shift to leave remaining args for hook processing
shift 2 || true

case "$HOOK" in
    validate_task)
        # Stub: implement validate_task logic.
        # TASK_JSON_FILE="${1:-}"
        # Validate task JSON structure
        exit 0
        ;;
    pre_prompt)
        # Stub: implement pre_prompt logic.
        # PROMPT_FILE="${1:-}"
        # Can modify prompt file in place
        exit 0
        ;;
    post_run)
        # Stub: implement post_run logic.
        # OUTPUT_FILE="${1:-}"
        # Process runner output
        exit 0
        ;;
    help|--help|-h)
        show_help
        exit 0
        ;;
    "")
        echo "Error: No hook specified" >&2
        show_help >&2
        exit 1
        ;;
    *)
        echo "Error: Unknown hook: $HOOK" >&2
        show_help >&2
        exit 1
        ;;
esac
"#;
