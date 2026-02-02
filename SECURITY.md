# Security Policy

Purpose: Document vulnerability reporting, data handling guidelines, and redaction behavior for Ralph.

## Reporting Vulnerabilities

If you discover a security vulnerability in Ralph, please report it responsibly:

1. **Do not open a public issue** for security vulnerabilities.
2. Email the maintainer directly at the contact address in the repository's git history or author metadata.
3. Include a clear description of the vulnerability and steps to reproduce.
4. Allow reasonable time for assessment and remediation before public disclosure.

## Data Handling Guidelines

When sharing logs, error reports, or debugging output from Ralph, be aware that certain data may contain sensitive information:

### What to Avoid Sharing Publicly

- **Runner outputs**: AI runner output (Codex, OpenCode, Gemini, Claude, Cursor) may contain secrets from your codebase, environment variables, or API keys.
- **Debug logs**: When `--debug` is enabled, raw runner output is written to `.ralph/logs/debug.log` without redaction. These logs may contain secrets.
- **Safeguard dumps**: Error recovery dumps may contain sensitive data from the runner output or environment.
- **Queue files with raw runner output**: Task notes that include runner output should be reviewed before sharing.

### Before Sharing

1. Review any output for API keys, tokens, passwords, or other credentials.
2. Use redacted safeguard dumps (the default) instead of raw dumps when sharing error reports.
3. Sanitize queue files before committing to public repositories if they contain sensitive task context.

## Redaction Behavior

Ralph includes built-in redaction for sensitive data. The following patterns are masked with `[REDACTED]`:

**Important**: Redaction is pattern-based and best-effort. It may miss secrets in unexpected formats, encoded data, or novel patterns. Always review output before sharing, even when redaction is applied.

### Redacted Patterns

- **API keys**: Values matching `API_KEY`, `api_key`, `APIKEY`, `apikey`, `PRIVATE_KEY`, `private_key`, `PRIVATEKEY`, `privatekey` patterns.
- **Bearer tokens**: Content following `Bearer ` in Authorization headers.
- **AWS keys**: Access keys starting with `AKIA` followed by 16 alphanumeric characters.
- **AWS secrets**: 40-character base64-like strings (alphanumeric + `/+=`).
- **SSH keys**: PEM-encoded keys (content between `-----BEGIN` and `-----END` markers).
- **Hex tokens**: Hexadecimal strings of 32+ characters.
- **Sensitive environment variables**: Values of environment variables matching `*KEY*`, `*SECRET*`, `*TOKEN*`, `*PASSWORD*` patterns (e.g., `API_TOKEN`, `AUTH_SECRET`, `DB_PASSWORD1`).

### Safeguard Dumps

Safeguard dumps are used for troubleshooting and error recovery. There are two modes:

#### Redacted Dumps (Default)

The `safeguard_text_dump_redacted()` function is the safe default. It applies redaction to all sensitive patterns before writing to disk.

```rust
// This is the recommended approach for error reporting
let path = safeguard_text_dump_redacted("error_context", &content)?;
```

#### Raw Dumps (Opt-in Required)

The `safeguard_text_dump()` function writes raw, unredacted content. This requires explicit opt-in via:

- Environment variable: `RALPH_RAW_DUMP=1` or `RALPH_RAW_DUMP=true`
- Debug mode: Passing `--debug` flag to Ralph commands

```bash
# Enable raw dumps via environment variable
RALPH_RAW_DUMP=1 ralph run one
RALPH_RAW_DUMP=true ralph run one

# Or use --debug flag (also enables raw dumps and raw debug logs)
ralph run one --debug
```

**Security Warning**: Raw dumps may contain secrets. Only use raw dumps when necessary for debugging and keep them secure. Never commit raw dumps to version control.

## Debug Logging

When the `--debug` flag is used, Ralph writes detailed logs to `.ralph/logs/debug.log`:

- Log records (redacted in console output, **raw in debug log**)
- Raw runner stdout/stderr streams

**Important**: Debug logs contain raw, unredacted runner output captured before redaction is applied. Even when console output is redacted, debug logs may contain secrets. Do not share debug logs publicly without reviewing them for secrets.

**Best practices for debug logs:**
- Only use `--debug` when necessary for troubleshooting
- Treat `.ralph/logs/debug.log` as sensitive data
- Clean up debug logs after use:

```bash
rm -rf .ralph/logs/
```
- Never commit debug logs to version control (add `.ralph/logs/` to `.gitignore`)

## Supported Versions

Security updates are provided for the latest released version of Ralph. Users should keep their installation up to date:

```bash
cargo install ralph --force
```

## Security Best Practices

1. **Review runner output**: AI runners may echo environment variables or file contents that contain secrets. Review before copying into task notes.
2. **Use redacted dumps**: When reporting issues, use the default redacted safeguard dumps. Remember that redaction is best-effort and may miss secrets.
3. **Limit debug mode**: Only use `--debug` when necessary and clean up `.ralph/logs/` afterward. Debug logs contain raw, unredacted output.
4. **Safeguard dump locations**: Redacted dumps are written to temporary directories (e.g., `/tmp/ralph/`). Clean these up periodically as they persist via `TempDir::keep()`.
5. **Sanitize queue files**: Before committing `.ralph/queue.json` or `.ralph/done.json` to version control, ensure no secrets are present in task notes or evidence fields.
6. **Environment variable hygiene**: Be cautious about what environment variables are set when running AI agents, as runners may have access to the full environment.

## Implementation Details

Redaction logic is implemented in:

- `crates/ralph/src/redaction.rs`: Core redaction patterns and `RedactedString` wrapper.
- `crates/ralph/src/fsutil.rs`: Safeguard dump functions with redaction by default.
- `crates/ralph/src/debuglog.rs`: Debug logging for raw runner output.

For questions about security practices or to report concerns, contact the maintainer directly.
