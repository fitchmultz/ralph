# Error Handling Guidelines

This document describes the error handling patterns used in Ralph and provides guidance for contributors.

## Philosophy

Ralph uses a two-tier error handling strategy:

1. **General errors**: Use `anyhow::Result` for error propagation
2. **Domain-specific errors**: Use `thiserror` for matchable, structured errors

## When to Use Each Pattern

### Use `anyhow` when:

- Propagating errors up the call stack
- Adding context to errors from external crates
- Quick error creation with `bail!` or `anyhow!`
- CLI argument parsing errors (clap value parsers)
- One-off error cases that don't need matching

**Example:**
```rust
use anyhow::{bail, Context, Result};

fn parse_phase(s: &str) -> anyhow::Result<RunPhase> {
    match s {
        "1" => Ok(RunPhase::Phase1),
        "2" => Ok(RunPhase::Phase2),
        "3" => Ok(RunPhase::Phase3),
        _ => bail!("invalid phase '{s}', expected 1, 2, or 3"),
    }
}

fn read_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read config from {}", path.display()))?;
    serde_json::from_str(&content)
        .context("parse config JSON")
}
```

### Use `thiserror` when:

- Defining error types that callers will match on
- Domain-specific error variants (git errors, runner errors, template errors)
- Errors that need structured data attached
- Public API error types

**Example:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("repo is dirty; commit/stash your changes before running Ralph.{details}")]
    DirtyRepo { details: String },

    #[error("git push failed: no upstream configured for current branch")]
    NoUpstream,

    #[error("git push failed: authentication/permission denied")]
    AuthFailed,
}
```

## Current Error Types

### RunnerError (`crates/ralph/src/runner.rs`)

Runner-specific failures with redaction support for sensitive output.

- 8 variants covering binary missing, spawn failed, non-zero exit, signal termination, timeout, interruption, and I/O errors
- Uses `RedactedString` for stdout/stderr to prevent secret leakage
- `NonZeroExit` and `TerminatedBySignal` include optional `session_id` for resumption
- Implements `thiserror::Error` for structured matching

**Variants:**
- `BinaryMissing` - Runner binary not found on PATH
- `SpawnFailed` - Failed to spawn runner process
- `NonZeroExit { code, stdout, stderr, session_id }` - Process exited with error code
- `TerminatedBySignal { stdout, stderr, session_id }` - Process killed by signal
- `Interrupted` - Execution was interrupted (Ctrl+C)
- `Timeout` - Runner exceeded timeout limit
- `Io` - General I/O error
- `Other` - Any other error (from anyhow::Error)

### GitError (`crates/ralph/src/git/error.rs`)

Git operation failures with actionable error messages.

- Exemplary documentation with invariants
- Actionable error messages with remediation hints
- Classifies push errors into specific variants (NoUpstream, AuthFailed, etc.)

### TemplateError (`crates/ralph/src/template/loader.rs`)

Template loading and parsing failures.

- Simple 3-variant enum for template operations
- Clean, user-facing error messages

### RunAbort (`crates/ralph/src/runutil.rs`)

Flow control errors for runner interruptions. These are not failures but intentional control-flow signals.

- Manual `std::error::Error` impl (justified for control flow semantics)
- Used for interrupted execution (`Ctrl+C`) and user-initiated reverts
- `RunAbortReason` enum classifies: `Interrupted` | `UserRevert`

**Detecting RunAbort in error chains:**

Use `abort_reason()` to detect if an error originated from a `RunAbort`:

```rust
use crate::runutil::{abort_reason, RunAbortReason};

// In the run loop, handle control-flow errors differently
match run_result {
    Ok(output) => output,
    Err(err) => {
        // Check if this is a control-flow abort (not a failure)
        if let Some(reason) = abort_reason(&err) {
            match reason {
                RunAbortReason::Interrupted => {
                    // User pressed Ctrl+C - exit cleanly
                    return Err(anyhow!("Run interrupted"));
                }
                RunAbortReason::UserRevert => {
                    // User chose to revert - this is expected behavior
                    return Err(anyhow!("Run aborted by user revert"));
                }
            }
        }
        // Otherwise, it's an actual error
        return Err(err);
    }
}
```

**Creating a RunAbort:**

```rust
use crate::runutil::{RunAbort, RunAbortReason};

// When the user chooses to revert during error recovery
return Err(anyhow::Error::new(RunAbort::new(
    RunAbortReason::UserRevert,
    "User chose to revert uncommitted changes",
)));
```

## Best Practices

### 1. Error messages should be actionable

Include hints for fixing the issue and reference relevant config or commands.

```rust
// Good
#[error("git push failed: no upstream configured for current branch. Set it with: git push -u origin <branch>")]
NoUpstream,

// Bad
#[error("push failed")]
PushFailed,
```

### 2. Redact sensitive data

Use `RedactedString` for runner output and apply `redact_text()` before logging.

```rust
use crate::redaction::{redact_text, RedactedString};

#[error("runner exited non-zero (code={code})\nstdout: {stdout}")]
NonZeroExit {
    code: i32,
    stdout: RedactedString,  // Automatically redacts secrets
},
```

**Security note**: While `RedactedString` ensures redacted output in console/logs, the `--debug` flag enables raw debug logging to `.ralph/logs/debug.log` that captures unredacted output before redaction is applied. Debug logs should be treated as sensitive and never committed to version control.

### 3. Implement `Send + Sync`

All error types must be thread-safe for anyhow compatibility. `thiserror::Error` derives these automatically.

### 4. Prefer `bail!` over `Err(...)`

More concise and consistent with the codebase.

```rust
// Good
bail!("invalid phase '{s}', expected 1, 2, or 3");

// Avoid
Err(format!("invalid phase '{s}', expected 1, 2, or 3"))
```

### 5. Use `.context()` for enrichment

Add context when crossing module boundaries. Include operation details like file paths and keys.

```rust
let content = fs::read_to_string(&path)
    .with_context(|| format!("read template from {}", path.display()))?;
```

## Decision Matrix

| Scenario | Pattern | Example |
|----------|---------|---------|
| Propagating errors | `anyhow::Result<T>` | `fn foo() -> Result<T>` |
| Quick error return | `bail!` | `bail!("invalid input")` |
| Adding context | `.context()` | `.context("read config")` |
| Matchable domain errors | `thiserror` | `RunnerError`, `GitError` |
| CLI value parsers | `anyhow::Result` | `parse_phase()` |

## Module Documentation Requirements

Error modules should include:

1. **Responsibilities**: What error types are defined here
2. **Invariants**: Thread-safety requirements (`Send + Sync`)
3. **What's NOT handled**: Success cases, non-domain errors

Example from `git/error.rs`:
```rust
//! Git-related error types and error classification.
//!
//! This module defines all error types that can occur during git operations.
//! It provides structured error variants for common failure modes like dirty
//! repositories, authentication failures, and missing upstream configuration.
//!
//! # Invariants
//! - All error types implement `Send + Sync` for anyhow compatibility
//! - Error messages should be actionable and include context where possible
//!
//! # What this does NOT handle
//! - Success cases or happy-path results
//! - Non-git related errors (use anyhow for those)
```
