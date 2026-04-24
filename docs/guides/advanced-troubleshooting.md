# Advanced Troubleshooting and Reference
Status: Active
Owner: Maintainers
Source of truth: this document for advanced troubleshooting and quick-reference commands
Parent: [Advanced Usage Guide](advanced.md)

## Troubleshooting Complex Issues

### Session Recovery

**Problem:** Session resume fails
```bash
# Check session state
jq '.' .ralph/cache/session.jsonc

# Force fresh start
ralph run loop --force

# Or clear session manually
rm .ralph/cache/session.jsonc
```

### Parallel Run Issues

**Problem:** "workspace_root not gitignored"
```bash
# Add to .gitignore
echo ".workspaces/" >> .gitignore
# Or use .git/info/exclude for local-only
echo ".workspaces/" >> .git/info/exclude
```

**Problem:** Base branch mismatch
```bash
# Check current branch
git branch --show-current

# View state file target branch
jq '.target_branch' .ralph/cache/parallel/state.json

# If no in-flight tasks, auto-heal by running
# Otherwise, checkout original base branch
```

**Problem:** Worker blocked in parallel integration
```bash
# Inspect worker lifecycle + error context
ralph run parallel status --json | jq '.workers[] | select(.lifecycle == "blocked_push")'

# Retry a blocked worker explicitly
ralph run parallel retry --task RQ-0001
```

### Queue Lock Issues

**Problem:** Stale queue lock
```bash
# Check lock status
ls -la .ralph/lock/

# Safe unlock (verifies PID not running)
ralph queue unlock

# Force with caution
ralph run one --force
```

### Plugin Debugging

**Problem:** Plugin not executing
```bash
# Verify plugin discovered
ralph plugin list

# Check validation
ralph plugin validate --id my.plugin

# Test runner directly
echo "test" | ~/.config/ralph/plugins/my.plugin/runner.sh run --model test

# Check environment
env | grep RALPH_
```

### Phase Violations

**Problem:** Phase 1 made code changes
```bash
# Check what changed
git status
git diff

# With git_revert_mode: ask
# Choose: revert, keep+continue, or continue with message

# Force proceed (if you know changes are acceptable)
ralph run one --force --allow-dirty
```

### CI Gate Failures

**Problem:** CI repeatedly fails
```bash
# Run the configured CI gate manually to see output
make agent-ci

# Check CI command config
ralph config show | grep ci_gate

# Temporarily disable (not recommended for production)
ralph run one --no-ci-gate
```

### Memory and Resource Issues

**Problem:** High memory usage during parallel runs
```json
{
  "parallel": {
    "workers": 2  // Reduce from default
  }
}
```

**Problem:** Slow task processing
```bash
# Use a fast-local profile for simple tasks
ralph run one --profile fast-local

# Skip phases when appropriate
ralph run one --phases 1
```

### Webhook Delivery Issues

**Problem:** Webhooks not sending
```bash
# Test webhook directly
ralph webhook test --url https://your-endpoint.com/webhook

# Check config
ralph config show | grep -A 10 webhook

# Debug with logs
RUST_LOG=debug ralph run one 2>&1 | grep -i webhook
```

### Dependency Resolution

**Problem:** Task stuck waiting for dependencies
```bash
# Check dependency graph
ralph queue graph --task RQ-0001

# View blocking tasks
ralph queue list --status doing

# Check done.json for completed dependencies
jq '.tasks[] | select(.id == "RQ-0000")' .ralph/done.jsonc
```

### Recovery Patterns

**Complete reset procedure:**
```bash
# 1. Stop any running daemon
ralph daemon stop

# 2. Clear all state
rm -f .ralph/cache/session.jsonc
rm -f .ralph/cache/parallel/state.json
rm -f .ralph/cache/daemon.json
rm -f .ralph/cache/stop_requested

# 3. Clear locks (if safe)
ralph queue unlock

# 4. Validate queue
ralph queue validate

# 5. Restart daemon if needed
ralph daemon start
```

**Debug mode for troubleshooting:**
```bash
# Enable debug logging
ralph --debug run one --id RQ-0001

# View debug logs
tail -f .ralph/logs/debug.log

# Clean up after
cat .ralph/logs/debug.log  # Review for secrets
rm -rf .ralph/logs/        # Secure deletion
```

---

## Quick Reference

### Common Command Patterns

```bash
# Quick single task with your local profile
ralph run one --profile fast-local

# Full workflow with review
ralph run one --profile deep-review

# Parallel execution
ralph run loop --parallel 4 --max-tasks 10

# Dry-run to check what would run
ralph run loop --dry-run

# Non-interactive CI mode
ralph run loop --non-interactive --max-tasks 5

# Resume interrupted session
ralph run loop --resume

# Wait for dependencies
ralph run loop --wait-when-blocked --wait-timeout-seconds 3600
```

### Config Quick Reference

| Setting | Config Path | CLI Override |
|---------|-------------|--------------|
| Runner | `agent.runner` | `--runner` |
| Model | `agent.model` | `--model` |
| Phases | `agent.phases` | `--phases` |
| Profile | N/A | `--profile` |
| Parallel workers | `parallel.workers` | `--parallel` |
| CI gate | `agent.ci_gate.enabled` | `--ci-gate-on/off` |
| Git publish | `agent.git_publish_mode` | `--git-publish-mode <off|commit|commit_and_push>` |

### File Locations

| File | Default Location |
|------|------------------|
| Queue | `.ralph/queue.jsonc` |
| Done archive | `.ralph/done.jsonc` |
| Project config | `.ralph/config.jsonc` |
| Global config | `~/.config/ralph/config.jsonc` |
| Session state | `.ralph/cache/session.jsonc` |
| Parallel state | `.ralph/cache/parallel/state.json` |
| Daemon logs | `.ralph/logs/daemon.log` |
| Debug logs | `.ralph/logs/debug.log` |
| Prompt overrides | `.ralph/prompts/*.md` |
| Plugins (project) | `.ralph/plugins/<id>/` |
| Plugins (global) | `~/.config/ralph/plugins/<id>/` |

