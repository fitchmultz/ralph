# Runner CLI Inventory (Phase 2)

Purpose: capture the *actual* CLI flags/options/behaviors of runner binaries used by Ralph’s execution layer, and propose a unified config schema + mapping strategy for Phase 3.

This document is an approval artifact: update it after running the inventory capture script and reviewing the outputs under `target/tmp/runner_cli_inventory/`.

---

## 0) How to regenerate raw inventory

Run:

```bash
scripts/runner_cli_inventory.sh --out target/tmp/runner_cli_inventory
```

If any runner is not on PATH, pass overrides:

```bash
scripts/runner_cli_inventory.sh --out target/tmp/runner_cli_inventory \
  --bin agent=/path/to/agent \
  --bin codex=/path/to/codex
```

Raw outputs will be in:

```
target/tmp/runner_cli_inventory/<runner>/
```

For each runner, record the version (from `version.txt` and/or help headers) and summarize key options below. Avoid copying any secrets.

---

## 1) Inventory Summary Table

| Runner | Binary path | Version | Prompt input style | Start invocation | Resume invocation | Streaming flag | Output formats supported | Notes |
|---|---|---|---|---|---|---|---|---|
| codex | `/opt/homebrew/bin/codex` | `codex-cli 0.91.0` | Positional `PROMPT` or stdin via `-` | `codex exec --json -m <MODEL> -` | `codex exec resume <SESSION_ID> --json -m <MODEL> <PROMPT>` (or `--last`) | `--json` (JSONL events) | Text (default), JSONL (`--json`) | Has explicit sandbox + approval policy flags |
| opencode | `/opt/homebrew/bin/opencode` | `1.1.36` | Positional `message..` + `--file` attachments | `opencode run --format json --file <PROMPT_FILE> -- <MESSAGE>` | `opencode run --session <ID> --format json -- <MESSAGE>` (or `--continue`) | `--format json` | `default`, `json` | `--help`/`--version` emit logs + ANSI sequences |
| gemini | `/opt/homebrew/bin/gemini` | `0.25.2` | Positional `query..` (stdin may also be read) | `gemini --model <MODEL> --output-format stream-json --approval-mode yolo <QUERY>` | `gemini --resume <ID> --model <MODEL> --output-format stream-json --approval-mode yolo <QUERY>` | `--output-format stream-json` | `text`, `json`, `stream-json` | `--prompt` is deprecated (positional preferred) |
| claude | `/Users/mitchfultz/.local/bin/claude` | `2.1.19 (Claude Code)` | Positional `prompt` (verify stdin behavior separately) | `claude -p --model <MODEL> --output-format stream-json --verbose <PROMPT>` | `claude -p --resume <ID> --model <MODEL> --output-format stream-json --verbose <MESSAGE>` | `--output-format stream-json` (requires `-p`) | `text`, `json`, `stream-json` | Permission modes are a Claude-specific enum |
| cursor (agent) | `/Users/mitchfultz/.local/bin/agent` | `2026.01.23-916f423` | Positional `prompt...` | `agent --print --output-format stream-json --model <MODEL> --sandbox <enabled|disabled> [--plan] "<PROMPT>"` | `agent --print --output-format stream-json --resume <CHAT_ID> --model <MODEL> --sandbox <enabled|disabled> [--plan] "<MESSAGE>"` | `--output-format stream-json` | `text`, `json`, `stream-json` | `--plan`/`--mode=plan` is read-only (no edits) |
| kimi | `/opt/homebrew/bin/kimi` | `kimi-cli` | `--prompt <TEXT>` | `kimi --print --output-format stream-json --model <MODEL> --session <ID> --prompt "<PROMPT>"` | `kimi --print --output-format stream-json --model <MODEL> --session <ID> --prompt "<MESSAGE>"` | `--output-format stream-json` | `text`, `stream-json` | Uses explicit `--session` for reliable resumption |

Definitions:
- **Prompt input style**: stdin vs temp file vs positional argument.
- **Streaming flag**: what we pass to force JSON streaming compatible with Ralph’s parser.
- **Start/Resume invocation**: minimal runnable example with placeholders.

---

## 2) Flags & Behavior: required categories (per runner)

For each runner below, fill:
- Start/resume command shape
- Approval/permission/safety mode flags and defaults
- Verbosity flags and defaults
- Output format / streaming flags and defaults
- Whether stdout is pure JSON (or includes interleaved text)
- Session id field names (thread_id/session_id/etc) if present
- Sandbox / plan mode knobs (if any)

### 2.1 codex

**Raw sources**:
- `target/tmp/runner_cli_inventory/codex/help.base.txt`
- `target/tmp/runner_cli_inventory/codex/help.exec.txt`
- `target/tmp/runner_cli_inventory/codex/help.exec_resume.txt`
- `target/tmp/runner_cli_inventory/codex/version.txt`

**Observed start command shape**:
- `codex exec [OPTIONS] [PROMPT] [COMMAND]`
- If `PROMPT` is omitted or `-` is passed, prompt is read from stdin.
- Streaming JSON events are enabled via `--json`.

**Observed resume command shape**:
- `codex exec resume [SESSION_ID] [PROMPT]`
- `--last` resumes the most recent recorded session without an id.
- `PROMPT` can be passed as `-` to read from stdin.

**Approval / permission / safety mode**
- Supported flags:
  - `-a, --ask-for-approval <APPROVAL_POLICY>`
  - `--dangerously-bypass-approvals-and-sandbox`
- Supported values (approval policy): `untrusted`, `on-failure`, `on-request`, `never`
- Notes:
  - `--full-auto` is a convenience alias for `-a on-request` and `--sandbox workspace-write`.
  - **Ralph behavior**: Ralph intentionally does NOT pass any approval flags (`-a`, `--ask-for-approval`) to Codex. This allows Codex to use the user's global config file (`~/.codex/config.json`) settings. If you want to control approval mode, set it in your Codex config, not in Ralph's config. The only exception is when `sandbox: disabled` is set, in which case `--dangerously-bypass-approvals-and-sandbox` is passed.

**Verbosity**
- No dedicated verbosity flag in `codex exec --help` output; verbosity is config-driven.

**Output format**
- Flags:
  - `--json` (events as JSONL)
- Default: human-readable text output (no `--json`)

**Streaming behavior**
- With `--json`, expect JSONL events on stdout; avoid mixing with non-JSON in the parent process.
- Session id appears to be referred to as “session id (UUID)” in the CLI help; confirm exact JSON field name from emitted events.

**Sandbox**
- Flags:
  - `-s, --sandbox <SANDBOX_MODE>`
- Values: `read-only`, `workspace-write`, `danger-full-access`
- Notes:
  - `--add-dir <DIR>` adds additional writable directories alongside the primary workspace.

**Plan mode**
- No explicit “plan mode” flag in `codex exec --help` output; planning is prompt-driven.

---

### 2.2 opencode

**Raw sources**:
- `target/tmp/runner_cli_inventory/opencode/help.base.txt`
- `target/tmp/runner_cli_inventory/opencode/help.run.txt`
- `target/tmp/runner_cli_inventory/opencode/version.txt`

Key observations (from `opencode run --help`):
- Start: `opencode run [message..]`
- Resume: `--continue` (most recent) or `--session <id>`
- Output format: `--format` with choices `default` or `json` (default is `default`)
- Prompt/files: `--file` attaches one or more files to the message
- Logging: help/version emit an `INFO ... refreshing` line, and base help includes ANSI art; do not assume pure, unadorned stdout

---

### 2.3 gemini

**Raw sources**:
- `target/tmp/runner_cli_inventory/gemini/help.base.txt`
- `target/tmp/runner_cli_inventory/gemini/help.resume.txt` (if present)
- `target/tmp/runner_cli_inventory/gemini/version.txt`

Key observations (from `gemini --help`):
- Prompt: positional `query` (help notes stdin may also be used; confirm runtime behavior separately)
- Resume: `--resume <value>` where value supports `latest` or index numbers
- Output format: `--output-format` choices `text`, `json`, `stream-json`
- Approval/safety: `--approval-mode` choices `default`, `auto_edit`, `yolo` (and `-y, --yolo` shorthand)
- Sandbox: `-s, --sandbox` is a boolean flag (not an enum)

---

### 2.4 claude

**Raw sources**:
- `target/tmp/runner_cli_inventory/claude/help.base.txt`
- `target/tmp/runner_cli_inventory/claude/help.resume.txt` (if present)
- `target/tmp/runner_cli_inventory/claude/version.txt`

Key observations (from `claude --help`):
- Non-interactive mode: `-p, --print` (required for `--output-format` / `--input-format`)
- Output format: `--output-format` choices `text`, `json`, `stream-json` (only works with `--print`)
- Streaming: `--include-partial-messages` (only with `--print` and `--output-format=stream-json`)
- Resume: `-r, --resume [value]` (picker/search term supported) and `-c, --continue` (most recent)
- Permissions: `--permission-mode` choices `acceptEdits`, `bypassPermissions`, `default`, `delegate`, `dontAsk`, `plan`

---

### 2.5 kimi

**Raw sources**:
- `target/tmp/runner_cli_inventory/kimi/help.base.txt`
- `target/tmp/runner_cli_inventory/kimi/version.txt`

**Observed start command shape**:
- `kimi --print --output-format stream-json --model <MODEL> --prompt "<PROMPT>"`

**Observed resume command shape**:
- `kimi --print --output-format stream-json --model <MODEL> --session <SESSION_ID> --prompt "<MESSAGE>"`
- Note: Ralph generates unique session IDs per phase (format: `{task_id}-p{phase}-{timestamp}`) rather than using `--continue` for deterministic session management.

**Approval / permission / safety mode**
- `--yolo/--yes` auto-approves; `--print` implies `--yolo`.

**Verbosity**
- `--verbose` and `--debug`.

**Output format**
- `--output-format stream-json` for JSONL events; default is text.

**Session handling**
- Ralph generates explicit session IDs (format: `{task_id}-p{phase}-{timestamp}`) and passes them via `--session`.
- This approach is more reliable than `--continue` which depends on Kimi's internal `last_session_id` tracking.
- Each phase gets its own session ID, ensuring isolation between planning, implementation, and review phases.
- Session IDs are deterministic and traceable for debugging purposes.

**Sandbox**
- No sandbox flag; relies on system environment + work dir.

**Plan mode**
- No explicit plan flag.

### 2.6 cursor (agent)

**Raw sources**:
- `target/tmp/runner_cli_inventory/agent/help.base.txt`
- `target/tmp/runner_cli_inventory/agent/help.run.txt` (if present)
- `target/tmp/runner_cli_inventory/agent/help.resume.txt` (if present)
- `target/tmp/runner_cli_inventory/agent/version.txt`

Key observations (from `agent --help`):
- Non-interactive mode: `-p, --print`
- Output format: `--output-format` choices `text`, `json`, `stream-json` (only works with `--print`)
- Streaming: `--stream-partial-output` (only with `--print` and `stream-json`)
- Resume: `--resume [chatId]` and `--continue` (most recent); also `agent resume` resumes latest
- Sandbox: `--sandbox` choices `enabled`, `disabled`
- Plan/read-only: `--mode plan|ask` and `--plan` shorthand (plan mode is “no edits” per help)

---

## 3) Delta vs Ralph’s current assumptions

Identify mismatches between what Ralph currently passes and what the runner actually supports.

### 3.1 Current Ralph flags (from `crates/ralph/src/runner/execution/runners.rs`)
- codex uses `exec --json --model ... [-c model_reasoning_effort]`
- opencode uses `run --format json --file <tmp> -- "Follow the attached prompt file verbatim."`
- gemini uses `--output-format stream-json --approval-mode yolo`
- claude uses `-p --permission-mode <...> --output-format stream-json --verbose`
- cursor uses `--sandbox enabled|disabled [--plan] --print --output-format stream-json <prompt>`

### 3.2 Confirmed mismatches
- `opencode` emits log lines (and base help includes ANSI sequences) even for `--help`/`--version`; avoid assuming clean stdout.
- `gemini` sandbox is a boolean (`--sandbox`), while `codex`/`agent` use an enum-like sandbox mode.
- `agent --plan` is explicitly read-only/no-edits; treat it as a mode switch, not just “more planning output”.

### 3.3 Confirmed matches
- `codex exec --json` exists and supports stdin prompt via `-`.
- `codex exec resume` exists and supports `--last`.
- `opencode run --format json` exists and supports file attachments via `--file`.
- `gemini --output-format stream-json` + `--approval-mode yolo` + `--resume` exist.
- `claude -p --output-format stream-json --permission-mode ...` exists.
- `agent --sandbox enabled|disabled --plan --print --output-format stream-json` exists.

---

## 4) Proposal: Unified Runner Config Schema (Phase 3 target)

Goal: make runner behaviors consistent across all five implementations while preserving runner-specific CLI realities.

### 4.1 Proposed normalized options (Ralph-level semantics)

**Output / streaming**
- `output.format`: enum (candidate): `stream_json`, `json`, `text`
- `output.stream_to_terminal`: already exists as `OutputStream`; leave unchanged

**Verbosity**
- `verbosity`: enum (candidate): `quiet`, `normal`, `verbose`

**Approval / permissions / tool safety**
- `approval.mode`: enum (candidate): `prompt` (interactive), `auto`, `yolo`
- `permissions.mode`: keep Claude-compatible enum for now; may generalize later

**Execution environment**
- `sandbox`: enum (candidate): `enabled`, `disabled`
- `plan_mode`: bool

### 4.2 Proposed config shape

```jsonc
{
  "agent": {
    "runner_cli": {
      "defaults": {
        "verbosity": "verbose",
        "output_format": "stream_json",
        "approval_mode": "yolo",
        "sandbox": "disabled",
        "plan_mode": false
      },
      "runners": {
        "codex": { "sandbox": "enabled" },
        "cursor": { "sandbox": "enabled", "plan_mode": true },
        "claude": { "verbosity": "verbose", "approval_mode": "auto" }
      }
    }
  }
}
```

Note: final field names/types must be validated against captured runner help output above.

---

## 5) Proposal: Builder Mapping Strategy (Phase 3 target)

### 5.1 The problem

Ralph currently encodes runner differences directly in `runners.rs`, producing drift:
- output format flags differ per runner
- resume syntax differs (subcommand vs flag vs `-s`)
- approval/permission flags differ and are hardcoded

### 5.2 Proposed architecture

Introduce a single normalized request -> runner-specific spec mapping:

**Normalized request (input to mapping layer)**
- model
- reasoning_effort (codex-only)
- prompt payload (stdin/string/file)
- session action (start vs resume)
- normalized options: output format, verbosity, approval/safety, sandbox, plan_mode

**RunnerCliSpec (per runner)**
- how to start/resume
- how to pass prompt (stdin/file/positional)
- which normalized options are supported and how to encode them
- which output format is required for Ralph stream parser

Then:
- `runners.rs` becomes thin: chooses spec, calls a shared builder
- unsupported options: ignored with a single standardized warning (not silent drift)

### 5.3 Acceptance criteria for Phase 3

- No hardcoded runner-specific flags in `runners.rs` beyond selecting the spec.
- Streaming output and session id extraction stays stable across all runners.
- Resume behavior is consistent at the Ralph API level even if CLI syntax differs.
